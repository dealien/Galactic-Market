use galactic_market::sim::SimState;
use galactic_market::sim::state::{
    ActiveEvent, CelestialBody, City, Company, Empire, ResourceType, Sector, StarSystem,
};

fn setup_empire_state() -> SimState {
    let mut state = SimState::new();

    // 1. Empire
    state.empires.insert(
        1,
        Empire {
            id: 1,
            name: "Test Empire".into(),
            government_type: "Republic".into(),
            tax_rate_base: 0.05,
        },
    );

    // 2. Sector
    state.sectors.insert(
        1,
        Sector {
            id: 1,
            empire_id: 1,
            name: "Test Sector".into(),
        },
    );

    // 3. System
    state.star_systems.insert(
        1,
        StarSystem {
            id: 1,
            sector_id: 1,
            name: "Test System".into(),
        },
    );

    // 4. Body
    state.celestial_bodies.insert(
        1,
        CelestialBody {
            id: 1,
            system_id: 1,
            name: "Test Planet".into(),
        },
    );

    // 5. City
    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "Test City".into(),
            population: 1000,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );

    // 6. Central Bank
    state.companies.insert(
        100,
        Company {
            id: 100,
            name: "Central Bank".into(),
            company_type: "central_bank".into(),
            home_city_id: 1,
            cash: 1000.0, // Low cash
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    // 7. Commercial Bank (Required for lending)
    state.companies.insert(
        200,
        Company {
            id: 200,
            name: "Sector Bank".into(),
            company_type: "commercial_bank".into(),
            home_city_id: 1,
            cash: 100000.0, // Significant but not infinite
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    // 8. Resource Types
    state.resource_types.insert(
        1,
        ResourceType {
            id: 1,
            name: "Iron Ore".into(),
            category: "Raw Material".into(),
        },
    );
    state.resource_types.insert(
        5,
        ResourceType {
            id: 5,
            name: "Food Rations".into(),
            category: "Consumer Good".into(),
        },
    );
    state.resource_types.insert(
        6,
        ResourceType {
            id: 6,
            name: "Water".into(),
            category: "Consumer Good".into(),
        },
    );

    state
}

#[test]
fn test_famine_relief_buy_orders() {
    let mut state = setup_empire_state();

    // Set next_eval_tick to current tick (1) to ensure the AI processes this tick
    state.companies.get_mut(&100).unwrap().next_eval_tick = 1;
    state.companies.get_mut(&100).unwrap().cash = 1000000.0; // High cash for relief

    // Add a famine event in city 1
    state.active_events.insert(
        1,
        ActiveEvent {
            id: 1,
            event_type: "famine".into(),
            target_id: Some(1),
            severity: 1.0,
            start_tick: 1,
            end_tick: 10,
            flavor_text: Some("Famine!".into()),
        },
    );

    // Prime the rate
    state.prime_rates.insert(1, 0.05);

    // Run decisions phase
    galactic_market::sim::decisions::run_decisions(&mut state, 1);

    // Check if any relief orders were posted by company 100 (Central Bank)
    let relief_orders: Vec<_> = state
        .market_orders
        .values()
        .filter(|o| o.company_id == 100 && o.order_kind == "market")
        .collect();

    assert!(
        !relief_orders.is_empty(),
        "Central Bank should have posted relief buy orders"
    );
    assert!(
        relief_orders.iter().any(|o| o.resource_type_id == 5),
        "Should have food relief order"
    );
}

#[test]
fn test_central_bank_monetary_policy() {
    let mut state = setup_empire_state();

    // Simulate high empire debt vs low cash
    // We have ~100k cash in the bank, so we need >40k debt
    state.companies.insert(
        1,
        Company {
            id: 1,
            name: "Debtor Co".into(),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash: 1000.0,
            debt: 50000.0, // High debt (> 0.4 * 101k)
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    state.companies.get_mut(&100).unwrap().next_eval_tick = 1;
    state.prime_rates.insert(1, 0.05);

    // Run decisions
    galactic_market::sim::decisions::run_decisions(&mut state, 1);

    let next_rate = state.prime_rates[&1];
    assert!(
        next_rate > 0.05,
        "Central Bank should have increased rates due to high debt (current: {})",
        next_rate
    );
}

#[test]
fn test_merchant_takes_loan_for_arbitrage() {
    let mut state = setup_empire_state();

    // Setup another city
    state.cities.insert(
        2,
        City {
            id: 2,
            body_id: 1,
            name: "City 2".into(),
            population: 1000,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );

    // Merchant with very low cash (10)
    state.companies.insert(
        300,
        Company {
            id: 300,
            name: "Merchant Co".into(),
            company_type: "merchant".into(),
            home_city_id: 1,
            cash: 10.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    // Arbitrage opportunity: City 1 (Price 10) -> City 2 (Price 100)
    state.ema_prices.insert((1, 1), 10.0);
    state.ema_prices.insert((2, 1), 100.0);

    // Prime rate
    state.prime_rates.insert(1, 0.05);

    // Run decisions
    galactic_market::sim::decisions::run_decisions(&mut state, 1);

    let merchant = &state.companies[&300];
    assert!(
        merchant.debt > 0.0,
        "Merchant should have taken a loan to capitalize on arbitrage (debt: {})",
        merchant.debt
    );
}

#[test]
fn test_consumer_borrows_during_liquidity_crisis() {
    let mut state = setup_empire_state();

    // Consumer with almost no cash
    state.companies.insert(
        400,
        Company {
            id: 400,
            name: "Consumer Co".into(),
            company_type: "consumer".into(),
            home_city_id: 1,
            cash: 10.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    // Prime rate
    state.prime_rates.insert(1, 0.05);

    // Run decisions
    galactic_market::sim::decisions::run_decisions(&mut state, 1);

    let consumer = &state.companies[&400];
    assert!(
        consumer.debt > 0.0,
        "Consumer should have taken a liquidity loan"
    );
    assert!(
        consumer.cash >= 5000.0,
        "Consumer should have received liquidity injection"
    );
}
