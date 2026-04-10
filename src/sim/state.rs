use std::collections::HashMap;

/// A city in the simulation.
#[derive(Debug, Clone)]
pub struct City {
    pub id: i32,
    pub body_id: i32,
    pub name: String,
    /// Resident population, used to calculate consumption demand.
    pub population: i64,
}

/// A resource type in the simulation.
#[derive(Debug, Clone)]
pub struct ResourceType {
    pub id: i32,
    pub name: String,
    pub category: String,
}

/// An economic actor (freelancer, company, corp, megacorp).
#[derive(Debug, Clone)]
pub struct Company {
    pub id: i32,
    pub name: String,
    pub company_type: String,
    pub home_city_id: i32,
    pub cash: f64,
    pub debt: f64,
    pub next_eval_tick: u64,
}

/// A resource deposit on a celestial body.
#[derive(Debug, Clone)]
pub struct Deposit {
    pub id: i32,
    pub body_id: i32,
    pub resource_type_id: i32,
    pub size_total: i64,
    pub size_remaining: i64,
    pub extraction_cost_per_unit: f64,
}

/// A company's stockpile at a specific city.
#[derive(Debug, Clone)]
pub struct Inventory {
    pub company_id: i32,
    pub city_id: i32,
    pub resource_type_id: i32,
    pub quantity: i64,
}

impl Inventory {
    /// Canonical composite key for the inventory HashMap.
    pub fn key(company_id: i32, city_id: i32, resource_type_id: i32) -> (i32, i32, i32) {
        (company_id, city_id, resource_type_id)
    }
}

/// A production or extraction facility.
#[derive(Debug, Clone)]
pub struct Facility {
    pub id: i32,
    pub city_id: i32,
    pub company_id: i32,
    pub facility_type: String,
    pub capacity: i32,
    pub setup_ticks_remaining: u32,
    pub target_resource_id: Option<i32>,
    pub production_ratios: Option<HashMap<String, f64>>,
}

/// A recipe defining a production transformation.
#[derive(Debug, Clone)]
pub struct Recipe {
    pub id: i32,
    pub name: String,
    pub output_resource_id: i32,
    pub output_qty: i32,
    pub facility_type: String,
    pub inputs: Vec<RecipeInput>,
}

/// One input requirement for a recipe.
#[derive(Debug, Clone)]
pub struct RecipeInput {
    pub resource_type_id: i32,
    pub quantity: i32,
}

/// An active buy or sell order in a city's market.
#[derive(Debug, Clone)]
pub struct MarketOrder {
    pub id: i32,
    pub city_id: i32,
    pub company_id: i32,
    pub resource_type_id: i32,
    pub order_type: String, // "buy" | "sell"
    pub price: f64,
    pub quantity: i64,
    pub created_tick: u64,
}

/// One tick's OHLCV record for a resource in a city.
#[derive(Debug, Clone)]
pub struct MarketHistory {
    pub city_id: i32,
    pub resource_type_id: i32,
    pub tick: u64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: i64,
}

/// The full in-memory simulation state.
///
/// Loaded from the database at startup; mutated in-memory each tick; flushed
/// periodically back to PostgreSQL.
pub struct SimState {
    /// Current simulation tick.
    pub tick: u64,

    /// All cities keyed by city ID.
    pub cities: HashMap<i32, City>,

    /// All companies keyed by company ID.
    pub companies: HashMap<i32, Company>,

    /// All resource deposits keyed by deposit ID.
    pub deposits: HashMap<i32, Deposit>,

    /// Company inventories keyed by `(company_id, city_id, resource_type_id)`.
    pub inventories: HashMap<(i32, i32, i32), Inventory>,

    /// Facilities keyed by facility ID.
    pub facilities: HashMap<i32, Facility>,

    /// Recipes keyed by recipe ID.
    pub recipes: HashMap<i32, Recipe>,

    /// Active market orders keyed by order ID. Cleared each tick after matching.
    pub market_orders: HashMap<i32, MarketOrder>,

    /// In-memory buffer of market history deltas — flushed every N ticks.
    pub market_history_buffer: Vec<MarketHistory>,

    /// Maps city_id → consumer company_id for fast lookup in the consumption phase.
    pub city_consumer_ids: HashMap<i32, i32>,

    /// Cached last clearing prices per (city_id, resource_type_id).
    pub price_cache: HashMap<(i32, i32), f64>,

    /// Exponential Moving Average (EMA) price cache.
    pub ema_prices: HashMap<(i32, i32), f64>,

    /// Metadata for all resource types.
    pub resource_types: HashMap<i32, ResourceType>,

    /// Monotonic counter for generating order IDs during a tick.
    next_order_id: i32,
}

impl Default for SimState {
    fn default() -> Self {
        Self::new()
    }
}

impl SimState {
    /// Create a new, empty simulation state at tick 0.
    pub fn new() -> Self {
        Self {
            tick: 0,
            cities: HashMap::new(),
            companies: HashMap::new(),
            deposits: HashMap::new(),
            inventories: HashMap::new(),
            facilities: HashMap::new(),
            recipes: HashMap::new(),
            market_orders: HashMap::new(),
            market_history_buffer: Vec::new(),
            city_consumer_ids: HashMap::new(),
            price_cache: HashMap::new(),
            ema_prices: HashMap::new(),
            resource_types: HashMap::new(),
            next_order_id: 1,
        }
    }

    /// Generate a unique order ID for this tick.
    pub fn next_order_id(&mut self) -> i32 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }
}

/// A high-level snapshot of the simulation state for debugging and logging.
#[derive(Debug, Default)]
pub struct TickSummary {
    pub tick: u64,
    pub total_cash: f64,
    pub total_debt: f64,
    pub total_inventory: i64,
    pub active_orders: usize,
    pub trade_volume: i64,
    pub avg_ore_price: f64,
    pub ingot_prices: HashMap<String, f64>,
}

impl SimState {
    /// Calculate a summary of the current simulation state.
    pub fn generate_summary(&self) -> TickSummary {
        let mut summary = TickSummary {
            tick: self.tick,
            active_orders: self.market_orders.len(),
            ..Default::default()
        };

        for c in self.companies.values() {
            summary.total_cash += c.cash;
            summary.total_debt += c.debt;
        }

        for inv in self.inventories.values() {
            summary.total_inventory += inv.quantity;
        }

        // Use the persistent price cache for averages across all cities
        let mut ore_total = 0.0;
        let mut ore_count = 0;

        for (&(_city_id, res_id), &price) in &self.price_cache {
            if let Some(res) = self.resource_types.get(&res_id) {
                if res.category == "Raw Material" {
                    ore_total += price;
                    ore_count += 1;
                } else if res.category == "Refined Material" {
                    summary.ingot_prices.insert(res.name.clone(), price);
                }
            }
        }

        if ore_count > 0 {
            summary.avg_ore_price = ore_total / ore_count as f64;
        }

        // Volume from the latest buffer entries
        summary.trade_volume = self.market_history_buffer.iter().map(|h| h.volume).sum();

        summary
    }
}
