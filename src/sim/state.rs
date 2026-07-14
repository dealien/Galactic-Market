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

/// Food balance analysis for a city (updated each tick).
/// Used by merchants to prioritize food routing to starving cities.
#[derive(Debug, Clone)]
pub struct CityFoodBalance {
    pub city_id: i32,
    pub food_surplus: i64,      // Production - consumption (inventory - needs)
    pub fulfillment_ratio: f64, // (food_inventory / population).min(2.0)
    pub needs_relief: bool,     // fulfillment_ratio < 0.4
    pub has_surplus: bool,      // food_surplus > 0
}

/// A celestial body (planet, moon, station).
#[derive(Debug, Clone)]
pub struct CelestialBody {
    pub id: i32,
    pub system_id: i32,
    pub name: String,
    pub fertility: f64,
}

/// A trade opportunity for a merchant (cached for performance).
/// Updated once every 5 ticks per merchant.
#[derive(Debug, Clone)]
pub struct MerchantOpportunity {
    pub resource_type_id: i32,
    pub origin_city_id: i32,
    pub dest_city_id: i32,
    pub buy_price: f64,
    pub sell_price: f64,
    pub profit_margin: f64,
    pub transport_cost: f64,
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

/// An interstellar empire or faction.
#[derive(Debug, Clone)]
pub struct Empire {
    pub id: i32,
    pub name: String,
    pub government_type: String,
    pub tax_rate_base: f64,
}

/// Diplomatic standing between two empires.
#[derive(Debug, Clone)]
pub struct DiplomaticRelation {
    pub empire_a_id: i32,
    pub empire_b_id: i32,
    pub tension: f64,
    pub status: String, // neutral, war, alliance
}

/// A military unit (fleet or garrison) belonging to an empire.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::state::MilitaryUnit;
///
/// let unit = MilitaryUnit {
///     id: 7,
///     empire_id: 3,
///     unit_type: "fleet".to_string(),
///     strength: 120.0,
///     system_id: 42,
///     status: "stationed".to_string(),
///     morale: 0.95,
/// };
///
/// assert_eq!(unit.empire_id, 3);
/// ```
#[derive(Debug, Clone)]
pub struct MilitaryUnit {
    pub id: i32,
    pub empire_id: i32,
    /// "fleet" (mobile offense/defense) or "garrison" (stationary defense).
    pub unit_type: String,
    /// Numeric combat power.
    pub strength: f64,
    /// Star system where the unit is located.
    pub system_id: i32,
    /// "stationed", "deployed", or "in_combat".
    pub status: String,
    /// Morale multiplier (0.0–1.0); affects combat effectiveness.
    pub morale: f64,
}

/// An alliance/treaty between N empires.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::state::Treaty;
///
/// let treaty = Treaty {
///     id: 11,
///     alliance_name: "Core Pact".to_string(),
///     member_empire_ids: vec![1, 2],
///     formed_tick: 100,
///     dissolved_tick: None,
/// };
///
/// assert!(treaty.dissolved_tick.is_none());
/// ```
#[derive(Debug, Clone)]
pub struct Treaty {
    pub id: i32,
    pub alliance_name: String,
    pub member_empire_ids: Vec<i32>,
    pub formed_tick: u64,
    /// None if still active.
    pub dissolved_tick: Option<u64>,
}

/// An active or concluded war between empires.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::state::War;
///
/// let war = War {
///     id: 21,
///     aggressor_id: 1,
///     defender_id: 2,
///     participants: vec![
///         (1, "aggressor".to_string()),
///         (2, "defender".to_string()),
///     ],
///     theaters: vec![10, 11],
///     start_tick: 250,
///     end_tick: None,
///     status: "active".to_string(),
///     cumulative_losses: 0.0,
/// };
///
/// assert_eq!(war.status, "active");
/// ```
#[derive(Debug, Clone)]
pub struct War {
    pub id: i32,
    pub aggressor_id: i32,
    pub defender_id: i32,
    /// (empire_id, role) tuples.
    ///
    /// Role values:
    /// - "aggressor"
    /// - "defender"
    /// - "aggressor_ally"
    /// - "defender_ally"
    /// - "ally" (legacy/ambiguous persisted data)
    pub participants: Vec<(i32, String)>,
    /// System IDs that are contested theaters of war.
    pub theaters: Vec<i32>,
    pub start_tick: u64,
    /// None if still active.
    pub end_tick: Option<u64>,
    /// "active", "ceasefire", or "concluded".
    pub status: String,
    /// Total military-strength losses accumulated across all ticks of this war.
    /// Compared against `WAR_EXHAUSTION_THRESHOLD` to trigger a war-exhaustion ending.
    pub cumulative_losses: f64,
}

/// A system occupied by a foreign empire.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::state::Occupation;
///
/// let occupation = Occupation {
///     system_id: 5,
///     occupier_empire_id: 2,
///     since_tick: 300,
/// };
///
/// assert_eq!(occupation.occupier_empire_id, 2);
/// ```
#[derive(Debug, Clone)]
pub struct Occupation {
    pub system_id: i32,
    pub occupier_empire_id: i32,
    pub since_tick: u64,
}

/// Tracks empire control within a sector.
///
/// # Examples
///
/// ```rust
/// use std::collections::HashMap;
///
/// use galactic_market::sim::state::SectorControl;
///
/// let control = SectorControl {
///     sector_id: 9,
///     empire_system_counts: HashMap::from([(1, 3_usize), (2, 1_usize)]),
///     total_systems: 4,
///     is_split: true,
/// };
///
/// assert!(control.is_split);
/// ```
#[derive(Debug, Clone)]
pub struct SectorControl {
    pub sector_id: i32,
    /// Maps empire_id → count of systems controlled in this sector.
    pub empire_system_counts: HashMap<i32, usize>,
    /// Total systems in the sector.
    pub total_systems: usize,
    /// Whether this sector is split between multiple empires.
    pub is_split: bool,
}

/// An active event affecting the simulation.
///
/// For blockade_lane events, target_id contains a tuple (sys_a, sys_b) representing
/// the blocked jump lane. For other event types, target_id is a city_id (stored as tuple (id, 0)).
#[derive(Debug, Clone)]
pub struct ActiveEvent {
    pub id: i32,
    pub event_type: String,
    /// For blockade_lane: (sys_a, sys_b). For others: (city_id, 0) or None.
    pub target_id: Option<(i32, i32)>,
    pub severity: f64,
    pub start_tick: u64,
    pub end_tick: u64,
    pub flavor_text: Option<String>,
}

/// A definition of a possible random event from JSON.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EventDefinition {
    pub id: String,
    pub weight: u32,
    pub severity_range: [f64; 2],
    pub effects: Vec<EventEffectDefinition>,
    pub flavor_text: String,
}

/// A mechanical effect defined in JSON.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct EventEffectDefinition {
    #[serde(rename = "type")]
    pub effect_type: String,
    pub duration_range: [u64; 2],
}

/// A resource type in the simulation.
#[derive(Debug, Clone)]
pub struct ResourceType {
    pub id: i32,
    pub name: String,
    pub category: String,
    /// True for resources (food, water) whose absence triggers population crises.
    pub is_vital: bool,
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

/// A deposit account held by a company at a bank.
#[derive(Debug, Clone)]
pub struct BankAccount {
    pub id: i32,
    pub company_id: i32,
    pub bank_company_id: i32,
    pub balance: f64,
    pub interest_rate: f64,
}

/// An outstanding loan for a company.
#[derive(Debug, Clone)]
pub struct Loan {
    pub id: i32,
    pub company_id: i32,
    pub lender_company_id: Option<i32>,
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
    /// Issue #9: Labor cost per production run (deducted from company cash).
    pub labor_cost_per_run: f64,
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
    pub order_kind: String, // "limit" | "market"
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

    /// Reverse index: company_id → [loan_ids] for O(1) lookup of all loans for a company.
    /// Maintained to avoid O(loans) filtering in reconciliation.
    pub company_to_loans: HashMap<i32, Vec<i32>>,

    /// All bank accounts keyed by account ID.
    pub bank_accounts: HashMap<i32, BankAccount>,

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

    /// Count of connected components in the jump lane network during last pathfinding.
    pub last_connected_components: usize,

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

    /// Prime rates set by Central Banks, keyed by Empire ID.
    pub prime_rates: HashMap<i32, f64>,

    /// Monotonic counter for generating order IDs during a tick.
    next_order_id: i32,

    /// Monotonic counter for generating trade route IDs during a tick.
    next_trade_route_id: i32,

    /// Monotonic counter for generating facility IDs.
    pub next_facility_id: i32,

    /// Monotonic counter for generating loan IDs.
    pub next_loan_id: i32,

    /// All empires keyed by empire ID.
    pub empires: HashMap<i32, Empire>,

    /// Diplomatic relations keyed by (emp_a, emp_b) tuple.
    pub diplomatic_relations: HashMap<(i32, i32), DiplomaticRelation>,

    /// Military units keyed by unit ID.
    pub military_units: HashMap<i32, MilitaryUnit>,

    /// Active and dissolved treaties keyed by treaty ID.
    pub treaties: HashMap<i32, Treaty>,

    /// Active and concluded wars keyed by war ID.
    pub wars: HashMap<i32, War>,

    /// Occupied systems keyed by system_id.
    pub occupied_systems: HashMap<i32, Occupation>,

    /// Sector control status keyed by sector_id.
    pub sector_control: HashMap<i32, SectorControl>,

    /// Monotonic counter for generating military unit IDs.
    pub next_military_unit_id: i32,

    /// Monotonic counter for generating treaty IDs.
    pub next_treaty_id: i32,

    /// Monotonic counter for generating war IDs.
    pub next_war_id: i32,

    /// Active events keyed by event ID.
    pub active_events: HashMap<i32, ActiveEvent>,

    /// Generic event definitions from JSON.
    pub event_definitions: Vec<EventDefinition>,

    /// Monotonic counter for generating event IDs.
    pub next_event_id: i32,

    /// Version counter incremented whenever the set of active blockade_lane events
    /// changes. `build_system_distances` uses this to avoid recomputing all-pairs
    /// shortest paths every tick when no blockades have changed.
    pub blockade_version: u64,

    /// The blockade_version that was current the last time `system_distances` was
    /// computed.
    pub distances_blockade_version: u64,

    /// Issue #9: Wage pools per city (accumulated during tick, drawn down by consumption).
    /// Keyed by city_id; represents total wages earned this tick.
    pub city_wage_pools: HashMap<i32, f64>,

    /// Issue #9: Empire treasury balances (tax revenue accumulator).
    /// Keyed by empire_id; represents accumulated taxes minus spending.
    pub empire_treasuries: HashMap<i32, f64>,

    /// Issue #10: Reverse index: company_id → empire_id for fast tax routing.
    pub company_to_empire: HashMap<i32, i32>,

    /// Phase 2: Food balance analysis per city (updated each tick).
    /// Used by merchants to prioritize routing to starving cities.
    pub city_food_balance: HashMap<i32, CityFoodBalance>,

    /// Phase 2d: Merchant opportunity cache for performance (updated every 5 ticks).
    /// Keyed by merchant_id; cached opportunities sorted by profit_margin descending.
    pub merchant_opportunities: HashMap<i32, Vec<MerchantOpportunity>>,

    /// Phase 2d: Last tick when opportunities were computed for each merchant.
    /// Used to control cache invalidation (recompute every 5 ticks).
    pub merchant_last_scan: HashMap<i32, u64>,
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
            company_to_loans: HashMap::new(),
            bank_accounts: HashMap::new(),
            deposits: HashMap::new(),
            inventories: HashMap::new(),
            facilities: HashMap::new(),
            recipes: HashMap::new(),
            trade_routes: HashMap::new(),
            market_orders: HashMap::new(),
            last_connected_components: 1,
            market_history_buffer: Vec::new(),
            city_consumer_ids: HashMap::new(),
            price_cache: HashMap::new(),
            ema_prices: HashMap::new(),
            resource_types: HashMap::new(),
            prime_rates: HashMap::new(),
            next_order_id: 1,
            next_trade_route_id: 1,
            next_facility_id: 1,
            next_loan_id: 1,
            empires: HashMap::new(),
            diplomatic_relations: HashMap::new(),
            military_units: HashMap::new(),
            treaties: HashMap::new(),
            wars: HashMap::new(),
            occupied_systems: HashMap::new(),
            sector_control: HashMap::new(),
            next_military_unit_id: 1,
            next_treaty_id: 1,
            next_war_id: 1,
            active_events: HashMap::new(),
            event_definitions: Vec::new(),
            next_event_id: 1,
            blockade_version: 0,
            distances_blockade_version: u64::MAX, // Force initial computation
            city_wage_pools: HashMap::new(),
            empire_treasuries: HashMap::new(),
            company_to_empire: HashMap::new(),
            city_food_balance: HashMap::new(),
            merchant_opportunities: HashMap::new(),
            merchant_last_scan: HashMap::new(),
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
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::state::SimState;
    ///
    /// let mut state = SimState::new();
    /// let id = state.next_facility_id();
    ///
    /// assert_eq!(id, 1);
    /// ```
    pub fn next_facility_id(&mut self) -> i32 {
        let id = self.next_facility_id;
        self.next_facility_id += 1;
        id
    }

    /// Generate a unique loan ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::state::SimState;
    ///
    /// let mut state = SimState::new();
    /// let id = state.next_loan_id();
    ///
    /// assert_eq!(id, 1);
    /// ```
    pub fn next_loan_id(&mut self) -> i32 {
        let id = self.next_loan_id;
        self.next_loan_id += 1;
        id
    }

    /// Generate a unique military unit ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::state::SimState;
    ///
    /// let mut state = SimState::new();
    /// let id = state.next_military_unit_id();
    ///
    /// assert_eq!(id, 1);
    /// ```
    pub fn next_military_unit_id(&mut self) -> i32 {
        let id = self.next_military_unit_id;
        self.next_military_unit_id += 1;
        id
    }

    /// Generate a unique treaty ID.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use galactic_market::sim::state::SimState;
    ///
    /// let mut state = SimState::new();
    /// let id = state.next_treaty_id();
    ///
    /// assert_eq!(id, 1);
    /// ```
    pub fn next_treaty_id(&mut self) -> i32 {
        let id = self.next_treaty_id;
        self.next_treaty_id += 1;
        id
    }

    /// Generate a unique war ID.
    ///
    /// # Examples
    ///
    /// ```
    /// use galactic_market::sim::SimState;
    ///
    /// let mut state = SimState::new();
    /// let war_id_1 = state.next_war_id();
    /// let war_id_2 = state.next_war_id();
    ///
    /// assert_eq!(war_id_1, 1);
    /// assert_eq!(war_id_2, 2);
    /// ```
    pub fn next_war_id(&mut self) -> i32 {
        let id = self.next_war_id;
        self.next_war_id += 1;
        id
    }

    /// Add a loan and update the company_to_loans reverse index.
    pub fn add_loan(&mut self, loan: crate::sim::state::Loan) {
        let company_id = loan.company_id;
        let loan_id = loan.id;
        self.loans.insert(loan_id, loan);
        self.company_to_loans
            .entry(company_id)
            .or_default()
            .push(loan_id);
    }

    /// Remove a loan and update the company_to_loans reverse index.
    pub fn remove_loan(&mut self, loan_id: i32) -> Option<crate::sim::state::Loan> {
        if let Some(loan) = self.loans.remove(&loan_id) {
            if let Some(loans) = self.company_to_loans.get_mut(&loan.company_id) {
                loans.retain(|id| *id != loan_id);
            }
            return Some(loan);
        }
        None
    }

    /// Get all loan IDs for a company efficiently (O(1) lookup) without cloning.
    pub fn get_company_loans(&self, company_id: i32) -> &[i32] {
        self.company_to_loans
            .get(&company_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    // === Issue #9: Wage Pool & Taxation Helpers ===

    /// Get the current wage pool for a city.
    pub fn get_wage_pool(&self, city_id: i32) -> f64 {
        self.city_wage_pools.get(&city_id).copied().unwrap_or(0.0)
    }

    /// Add wages to a city's wage pool.
    pub fn add_to_wage_pool(&mut self, city_id: i32, amount: f64) {
        *self.city_wage_pools.entry(city_id).or_insert(0.0) += amount;
    }

    /// Withdraw wages from a city's wage pool (e.g., for consumption).
    pub fn withdraw_from_wage_pool(&mut self, city_id: i32, amount: f64) -> f64 {
        let pool = self.city_wage_pools.entry(city_id).or_insert(0.0);
        let available = *pool;
        if available >= amount {
            *pool -= amount;
            amount
        } else {
            *pool = 0.0;
            available
        }
    }

    /// Reset all wage pools to zero (called at start of each tick after loading state).
    pub fn reset_wage_pools(&mut self) {
        for pool in self.city_wage_pools.values_mut() {
            *pool = 0.0;
        }
    }

    /// Issue #9: Accumulate port fees and local taxes for a city (flows to empire treasury later).
    /// For now, we track this separate from wage pools to maintain accounting clarity.
    /// Cities have a separate tax_collected_this_tick field in the database.
    pub fn add_city_tax(&mut self, city_id: i32, amount: f64) {
        // In Phase 7 (Finance), these accumulated taxes are transferred to empire treasury.
        // For now, we'll add to wage pools as a temporary holding area.
        // TODO: In a future iteration, maintain separate city_tax_pools HashMap
        *self.city_wage_pools.entry(city_id).or_insert(0.0) += amount;
    }

    /// Get the current treasury balance for an empire.
    pub fn get_empire_treasury(&self, empire_id: i32) -> f64 {
        self.empire_treasuries
            .get(&empire_id)
            .copied()
            .unwrap_or(0.0)
    }

    /// Add tax revenue to an empire's treasury.
    pub fn add_to_empire_treasury(&mut self, empire_id: i32, amount: f64) {
        *self.empire_treasuries.entry(empire_id).or_insert(0.0) += amount;
    }

    /// Withdraw treasury funds (e.g., for empire relief spending).
    pub fn withdraw_from_empire_treasury(&mut self, empire_id: i32, amount: f64) -> f64 {
        let treasury = self.empire_treasuries.entry(empire_id).or_insert(0.0);
        let available = *treasury;
        if available >= amount {
            *treasury -= amount;
            amount
        } else {
            *treasury = 0.0;
            available
        }
    }

    /// Calculate the empire_id for a company (via sector → empire mapping).
    pub fn get_company_empire(&self, company_id: i32) -> Option<i32> {
        self.company_to_empire.get(&company_id).copied()
    }

    /// Calculate and calculate summary of the current simulation state.
    pub fn generate_summary(&self) -> TickSummary {
        let mut summary = TickSummary {
            tick: self.tick,
            active_orders: self.market_orders.len(),
            total_companies: self.companies.len(),
            ..Default::default()
        };

        // --- Companies & Finance ---
        let mut total_company_debt = 0.0;
        let mut company_count_for_ratio = 0;
        for c in self.companies.values() {
            summary.total_cash += c.cash;
            summary.total_debt += c.debt;

            // Track company type breakdown
            summary
                .company_breakdown
                .entry(c.company_type.clone())
                .and_modify(|count| *count += 1)
                .or_insert(1);

            // Accumulate for debt-to-cash ratio
            if c.cash + c.debt > 0.0 {
                total_company_debt += c.debt;
                company_count_for_ratio += 1;
            }
        }

        if company_count_for_ratio > 0 {
            summary.avg_debt_to_cash = total_company_debt / (company_count_for_ratio as f64);
        }

        // --- Population ---
        for city in self.cities.values() {
            summary.total_population += city.population;
        }

        // --- Inventory & Food ---
        for inv in self.inventories.values() {
            summary.total_inventory += inv.quantity;

            // Check if this is food
            if let Some(res) = self.resource_types.get(&inv.resource_type_id)
                && res.is_vital
            {
                summary.total_food_inventory += inv.quantity;
            }
        }

        // --- Plantations ---
        for facility in self.facilities.values() {
            if facility.facility_type == "plantation" {
                summary.active_plantations += 1;
            }
        }

        // --- Active Events ---
        summary.total_active_events = self.active_events.len();

        // --- Market Orders Breakdown ---
        for order in self.market_orders.values() {
            match order.order_type.as_str() {
                "buy" => summary.buy_orders += 1,
                "sell" => summary.sell_orders += 1,
                _ => {}
            }
        }

        // --- Prices ---
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

        // Total volume accumulated since the last flush
        summary.trade_volume = self.market_history_buffer.iter().map(|h| h.volume).sum();

        summary
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
    // New metrics for enhanced display
    pub total_companies: usize,
    pub company_breakdown: HashMap<String, usize>, // company_type -> count
    pub total_population: i64,
    pub total_food_inventory: i64,
    pub active_plantations: usize,
    pub avg_debt_to_cash: f64,
    pub total_active_events: usize,
    pub buy_orders: usize,
    pub sell_orders: usize,
}
