use galactic_market::sim::state::{
    City, Company, Deposit, Facility, Inventory, MarketOrder, Recipe, RecipeInput,
};
use galactic_market::sim::SimState;

/// Build a minimal SimState with one miner + deposit + refinery for integration tests.
fn full_economy_state() -> SimState {
    let mut state = SimState::new();

    state.cities.insert(1, City { id: 1, body_id: 1, name: "Test City".into() });

    state.companies.insert(
        1,
        Company {
            id: 1,
            name: "Miner Co".into(),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash: 10_000.0,
            debt: 0.0,
            next_eval_tick: 1,
        },
    );

    state.deposits.insert(
        1,
        Deposit {
            id: 1,
            body_id: 1,
            resource_type_id: 1,
            size_total: 10_000,
            size_remaining: 10_000,
            extraction_cost_per_unit: 2.0,
        },
    );

    state.facilities.insert(
        1,
        Facility {
            id: 1,
            city_id: 1,
            company_id: 1,
            facility_type: "mine".into(),
            capacity: 10,
        },
    );

    state.facilities.insert(
        2,
        Facility {
            id: 2,
            city_id: 1,
            company_id: 1,
            facility_type: "refinery".into(),
            capacity: 5,
        },
    );

    state.recipes.insert(
        1,
        Recipe {
            id: 1,
            name: "Iron Ingot Smelting".into(),
            output_resource_id: 2,
            output_qty: 1,
            facility_type: "refinery".into(),
            inputs: vec![RecipeInput { resource_type_id: 1, quantity: 3 }],
        },
    );

    state
}

#[test]
fn test_extraction_depletes_deposit() {
    let mut state = full_economy_state();

    let initial_remaining = state.deposits[&1].size_remaining;

    // Run 10 extraction ticks manually
    for _ in 0..10 {
        galactic_market::sim::resources::run_extraction(&mut state);
    }

    assert!(
        state.deposits[&1].size_remaining < initial_remaining,
        "Deposit should be depleted after extraction ticks"
    );
}

#[test]
fn test_refinery_consumes_ore_and_produces_ingots() {
    let mut state = full_economy_state();

    // Extract ore first
    for _ in 0..10 {
        galactic_market::sim::resources::run_extraction(&mut state);
    }

    // Run production
    for _ in 0..3 {
        galactic_market::sim::production::run_production(&mut state);
    }

    let ingot_key = Inventory::key(1, 1, 2);
    assert!(
        state.inventories.get(&ingot_key).map(|i| i.quantity).unwrap_or(0) > 0,
        "Iron Ingots should have been produced"
    );

    let ore_key = Inventory::key(1, 1, 1);
    let ore_remaining = state.inventories.get(&ore_key).map(|i| i.quantity).unwrap_or(0);
    assert!(ore_remaining < 100, "Iron Ore should have been consumed");
}

#[test]
fn test_market_clearing_balances() {
    let mut state = SimState::new();
    state.cities.insert(1, City { id: 1, body_id: 1, name: "C".into() });

    state.companies.insert(
        1,
        Company {
            id: 1, name: "Seller".into(), company_type: "freelancer".into(),
            home_city_id: 1, cash: 0.0, debt: 0.0, next_eval_tick: 1,
        },
    );
    state.companies.insert(
        2,
        Company {
            id: 2, name: "Buyer".into(), company_type: "freelancer".into(),
            home_city_id: 1, cash: 500.0, debt: 0.0, next_eval_tick: 1,
        },
    );

    let total_before = state.companies[&1].cash + state.companies[&2].cash;

    // 10 ore for the seller
    state.inventories.insert(
        Inventory::key(1, 1, 1),
        Inventory { company_id: 1, city_id: 1, resource_type_id: 1, quantity: 10 },
    );

    state.market_orders.insert(
        1,
        MarketOrder {
            id: 1, city_id: 1, company_id: 1,
            resource_type_id: 1, order_type: "sell".into(),
            price: 8.0, quantity: 10, created_tick: 0,
        },
    );
    state.market_orders.insert(
        2,
        MarketOrder {
            id: 2, city_id: 1, company_id: 2,
            resource_type_id: 1, order_type: "buy".into(),
            price: 10.0, quantity: 10, created_tick: 0,
        },
    );

    galactic_market::sim::markets::clear_orders(&mut state, 1);

    let total_after = state.companies[&1].cash + state.companies[&2].cash;
    assert!(
        (total_before - total_after).abs() < 0.001,
        "Total cash must be conserved through market clearing"
    );
}
