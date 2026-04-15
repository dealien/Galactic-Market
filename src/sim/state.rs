use std::collections::HashMap;

/// A city in the simulation.
#[derive(Debug, Clone)]
pub struct City {
    pub id: i32,
    pub body_id: i32,
    pub name: String,
    /// Resident population, used to calculate consumption demand.
    pub population: i64,
    pub port_tier: i32,
    pub port_fee_per_unit: f64,
    pub port_max_throughput: i64,
}

/// A celestial body (planet, moon, station).
#[derive(Debug, Clone)]
pub struct CelestialBody {
    pub id: i32,
    pub system_id: i32,
    pub name: String,
}

/// A star system containing celestial bodies.
#[derive(Debug, Clone)]
pub struct StarSystem {
    pub id: i32,
    pub sector_id: i32,
    pub name: String,
}

/// A sector of space containing star systems.
#[derive(Debug, Clone)]
pub struct Sector {
    pub id: i32,
    pub empire_id: i32,
    pub name: String,
}

/// A jump lane between star systems.
#[derive(Debug, Clone)]
pub struct SystemLane {
    pub system_a_id: i32,
    pub system_b_id: i32,
    pub distance_ly: f64,
    pub lane_type: String,
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
    /// Status: "active", "bankrupt", "liquidated"
    pub status: String,
    /// The last tick this company successfully cleared a trade.
    pub last_trade_tick: u64,
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

/// An outstanding loan for a company.
#[derive(Debug, Clone)]
pub struct Loan {
    pub id: i32,
    pub company_id: i32,
    pub principal: f64,
    pub interest_rate: f64,
    pub balance: f64,
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

/// An active in-transit shipment between cities.
#[derive(Debug, Clone)]
pub struct TradeRoute {
    pub id: i32,
    pub company_id: i32,
    pub origin_city_id: i32,
    pub dest_city_id: i32,
    pub resource_type_id: i32,
    pub quantity: i64,
    pub arrival_tick: u64,
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

    /// All celestial bodies keyed by body ID.
    pub celestial_bodies: HashMap<i32, CelestialBody>,

    /// All star systems keyed by system ID.
    pub star_systems: HashMap<i32, StarSystem>,

    /// All jump lanes keyed by (sys_a, sys_b) tuple.
    pub system_lanes: HashMap<(i32, i32), SystemLane>,

    /// Cached shortest path distances between star systems.
    pub system_distances: HashMap<(i32, i32), f64>,

    /// All sectors keyed by sector ID.
    pub sectors: HashMap<i32, Sector>,

    /// All companies keyed by company ID.
    pub companies: HashMap<i32, Company>,

    /// Outstanding loans keyed by loan ID.
    pub loans: HashMap<i32, Loan>,

    /// All resource deposits keyed by deposit ID.
    pub deposits: HashMap<i32, Deposit>,

    /// Company inventories keyed by `(company_id, city_id, resource_type_id)`.
    pub inventories: HashMap<(i32, i32, i32), Inventory>,

    /// Facilities keyed by facility ID.
    pub facilities: HashMap<i32, Facility>,

    /// Recipes keyed by recipe ID.
    pub recipes: HashMap<i32, Recipe>,

    /// Active in-transit shipments keyed by route ID.
    pub trade_routes: HashMap<i32, TradeRoute>,

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

    /// Monotonic counter for generating trade route IDs during a tick.
    next_trade_route_id: i32,

    /// Monotonic counter for generating facility IDs.
    pub next_facility_id: i32,
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
            celestial_bodies: HashMap::new(),
            star_systems: HashMap::new(),
            system_lanes: HashMap::new(),
            system_distances: HashMap::new(),
            sectors: HashMap::new(),
            companies: HashMap::new(),
            loans: HashMap::new(),
            deposits: HashMap::new(),
            inventories: HashMap::new(),
            facilities: HashMap::new(),
            recipes: HashMap::new(),
            trade_routes: HashMap::new(),
            market_orders: HashMap::new(),
            market_history_buffer: Vec::new(),
            city_consumer_ids: HashMap::new(),
            price_cache: HashMap::new(),
            ema_prices: HashMap::new(),
            resource_types: HashMap::new(),
            next_order_id: 1,
            next_trade_route_id: 1,
            next_facility_id: 1,
        }
    }

    /// Generate a unique order ID for this tick.
    pub fn next_order_id(&mut self) -> i32 {
        let id = self.next_order_id;
        self.next_order_id += 1;
        id
    }

    /// Generate a unique trade route ID for this tick.
    pub fn next_trade_route_id(&mut self) -> i32 {
        let id = self.next_trade_route_id;
        self.next_trade_route_id += 1;
        id
    }

    /// Generate a unique facility ID.
    pub fn next_facility_id(&mut self) -> i32 {
        let id = self.next_facility_id;
        self.next_facility_id += 1;
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
