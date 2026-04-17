use galactic_market::sim::SimState;
use galactic_market::sim::state::{
    ActiveEvent, CelestialBody, City, Company, Empire, Sector, StarSystem,
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
            cash: 1000000.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    state
}

#[test]
fn test_famine_relief_buy_orders() {
    let mut state = setup_empire_state();

    // Set next_eval_tick to current tick (1) to ensure the AI processes this tick
    state.companies.get_mut(&100).unwrap().next_eval_tick = 1;

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
    assert!(
        relief_orders.iter().any(|o| o.resource_type_id == 6),
        "Should have water relief order"
    );
}

#[test]
fn test_central_bank_monetary_policy() {
    let mut state = setup_empire_state();

    // Override cash to be low for this test to trigger rate increase
    state.companies.get_mut(&100).unwrap().cash = 1000.0;
    state.companies.get_mut(&100).unwrap().next_eval_tick = 1;

    // Simulate high empire debt vs low cash
    // Add a company with high debt
    state.companies.insert(
        1,
        Company {
            id: 1,
            name: "Debtor Co".into(),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash: 1000.0,
            debt: 10000.0, // High debt (10:1 ratio)
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

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
