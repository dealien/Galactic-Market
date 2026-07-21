use galactic_market::sim::SimState;
use galactic_market::sim::state::{
    City, Company, Deposit, Facility, Inventory, MarketOrder, Recipe, RecipeInput,
};

/// Build a minimal SimState with one miner + deposit + refinery for integration tests.
fn full_economy_state() -> SimState {
    let mut state = SimState::new();

    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "Test City".into(),
            population: 10000,
            infrastructure_lvl: 5,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
            tax_collected_this_tick: 0.0,
            population_growth_rate: 0.0,
        },
    );

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
            status: "active".into(),
            last_trade_tick: 0,
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
            setup_ticks_remaining: 0,
            target_resource_id: Some(1),
            production_ratios: None,
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
            setup_ticks_remaining: 0,
            target_resource_id: None,
            production_ratios: Some(std::collections::HashMap::from([("1".to_string(), 1.0)])),
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
            inputs: vec![RecipeInput {
                resource_type_id: 1,
                quantity: 3,
            }],
            labor_cost_per_run: 1.5,
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
        state
            .inventories
            .get(&ingot_key)
            .map(|i| i.quantity)
            .unwrap_or(0)
            > 0,
        "Iron Ingots should have been produced"
    );

    let ore_key = Inventory::key(1, 1, 1);
    let ore_remaining = state
        .inventories
        .get(&ore_key)
        .map(|i| i.quantity)
        .unwrap_or(0);
    assert!(ore_remaining < 100, "Iron Ore should have been consumed");
}

#[test]
fn test_market_clearing_balances() {
    let mut state = SimState::new();
    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "C".into(),
            population: 10000,
            infrastructure_lvl: 5,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
            tax_collected_this_tick: 0.0,
            population_growth_rate: 0.0,
        },
    );

    state.companies.insert(
        1,
        Company {
            id: 1,
            name: "Miner".into(),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash: 100.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );
    state.companies.insert(
        2,
        Company {
            id: 2,
            name: "Refiner".into(),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash: 100.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        },
    );

    let total_before = state.companies[&1].cash + state.companies[&2].cash;

    // 10 ore for the seller
    state.inventories.insert(
        Inventory::key(1, 1, 1),
        Inventory {
            company_id: 1,
            city_id: 1,
            resource_type_id: 1,
            quantity: 10,
        },
    );

    state.market_orders.insert(
        1,
        MarketOrder {
            id: 1,
            city_id: 1,
            company_id: 1,
            resource_type_id: 1,
            order_type: "sell".into(),
            order_kind: "limit".into(),
            price: 5.0,
            quantity: 50,
            created_tick: 1,
        },
    );
    state.market_orders.insert(
        2,
        MarketOrder {
            id: 2,
            city_id: 1,
            company_id: 2,
            resource_type_id: 2,
            order_type: "buy".into(),
            order_kind: "limit".into(),
            price: 5.0,
            quantity: 50,
            created_tick: 1,
        },
    );

    galactic_market::sim::markets::clear_orders(&mut state, 1);

    let total_after = state.companies[&1].cash + state.companies[&2].cash;
    assert!(
        (total_before - total_after).abs() < 0.001,
        "Total cash must be conserved through market clearing"
    );
}

#[tokio::test]
async fn test_db_flush_persists_closed_loop_economy_fields() -> Result<(), anyhow::Error> {
    let _ = dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    galactic_market::db::utils::clear_database(&pool).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    galactic_market::db::seed::run_seed(&pool).await?;

    let mut state = galactic_market::db::load::load(&pool).await?;

    state.add_to_wage_pool(1, 123.45);
    state.add_city_tax(1, 56.78);

    if let Some(city) = state.cities.get_mut(&1) {
        city.population_growth_rate = 0.042;
    }

    state.add_to_empire_treasury(1, 999.99);

    if let Some(emp) = state.empires.get_mut(&1) {
        emp.tax_rate = 0.08;
    }

    state.flush_with_pulse(&pool).await?;

    let reloaded_state = galactic_market::db::load::load(&pool).await?;

    assert!((reloaded_state.get_wage_pool(1) - 180.23).abs() < 0.001); // add_city_tax added to wage_pool in memory

    let city = reloaded_state.cities.get(&1).unwrap();
    assert_eq!(city.tax_collected_this_tick, 56.78);
    assert_eq!(city.population_growth_rate, 0.042);

    assert_eq!(reloaded_state.get_empire_treasury(1), 999.99);

    let emp = reloaded_state.empires.get(&1).unwrap();
    assert_eq!(emp.tax_rate, 0.08);

    Ok(())
}
