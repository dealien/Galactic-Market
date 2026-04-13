use galactic_market::sim::SimState;
use galactic_market::sim::state::{
    City, Company, Deposit, Facility, Inventory, MarketOrder, Recipe, RecipeInput,
};

fn main() {
    divan::main();
}

fn make_extraction_state(num_companies: usize) -> SimState {
    let mut state = SimState::new();

    for i in 1..=(num_companies as i32) {
        state.cities.insert(
            i,
            City {
                id: i,
                body_id: i,
                name: format!("City {i}"),
                population: 0,
            },
        );

        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Miner {i}"),
                company_type: "freelancer".into(),
                home_city_id: i,
                cash: 100_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
            },
        );

        state.deposits.insert(
            i,
            Deposit {
                id: i,
                body_id: i,
                resource_type_id: 1,
                size_total: 1_000_000,
                size_remaining: 1_000_000,
                extraction_cost_per_unit: 2.0,
            },
        );

        state.facilities.insert(
            i,
            Facility {
                id: i,
                city_id: i,
                company_id: i,
                facility_type: "mine".into(),
                capacity: 10,
                setup_ticks_remaining: 0,
                target_resource_id: None,
                production_ratios: None,
            },
        );
    }

    state
}

fn make_market_state(num_orders: usize) -> SimState {
    let mut state = SimState::new();
    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "Market City".into(),
            population: 0,
        },
    );

    // One seller with lots of ore
    state.companies.insert(
        1,
        Company {
            id: 1,
            name: "Seller".into(),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash: 0.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
        },
    );
    state.inventories.insert(
        Inventory::key(1, 1, 1),
        Inventory {
            company_id: 1,
            city_id: 1,
            resource_type_id: 1,
            quantity: num_orders as i64 * 10,
        },
    );

    for i in 2..=(num_orders as i32 + 1) {
        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Buyer {i}"),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 100_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
            },
        );
        state.market_orders.insert(
            i * 2,
            MarketOrder {
                id: i * 2,
                city_id: 1,
                company_id: 1,
                resource_type_id: 1,
                order_type: "sell".into(),
                price: 8.0,
                quantity: 10,
                created_tick: 0,
            },
        );
        state.market_orders.insert(
            i * 2 + 1,
            MarketOrder {
                id: i * 2 + 1,
                city_id: 1,
                company_id: i,
                resource_type_id: 1,
                order_type: "buy".into(),
                price: 10.0,
                quantity: 10,
                created_tick: 0,
            },
        );
    }

    state
}

fn make_production_state(num_refineries: usize) -> SimState {
    let mut state = SimState::new();

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
        },
    );

    for i in 1..=(num_refineries as i32) {
        state.cities.insert(
            i,
            City {
                id: i,
                body_id: i,
                name: format!("City {i}"),
                population: 0,
            },
        );
        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Refiner {i}"),
                company_type: "freelancer".into(),
                home_city_id: i,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
            },
        );
        state.inventories.insert(
            Inventory::key(i, i, 1),
            Inventory {
                company_id: i,
                city_id: i,
                resource_type_id: 1,
                quantity: 300,
            },
        );
        state.facilities.insert(
            i,
            Facility {
                id: i,
                city_id: i,
                company_id: i,
                facility_type: "refinery".into(),
                capacity: 5,
                setup_ticks_remaining: 0,
                target_resource_id: None,
                production_ratios: None,
            },
        );
    }

    state
}

fn make_decisions_state(num_companies: usize) -> SimState {
    let mut state = SimState::new();

    for i in 1..=(num_companies as i32) {
        state.cities.insert(
            i,
            City {
                id: i,
                body_id: i,
                name: format!("City {i}"),
                population: 0,
            },
        );
        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Company {i}"),
                company_type: "freelancer".into(),
                home_city_id: i,
                cash: 500.0,
                debt: 0.0,
                // Mix of due and not-due companies
                next_eval_tick: if i % 2 == 0 { 1 } else { 9999 },
                status: "active".into(),
            },
        );
        state.deposits.insert(
            i,
            Deposit {
                id: i,
                body_id: i,
                resource_type_id: 1,
                size_total: 1_000,
                size_remaining: 1_000,
                extraction_cost_per_unit: 2.0,
            },
        );
        state.inventories.insert(
            Inventory::key(i, i, 1),
            Inventory {
                company_id: i,
                city_id: i,
                resource_type_id: 1,
                quantity: 100,
            },
        );
    }

    state
}

// ─── Benchmarks ────────────────────────────────────────────────────────────────

#[divan::bench(args = [32, 128, 512])]
fn bench_extraction_phase(bencher: divan::Bencher, num_companies: usize) {
    bencher
        .with_inputs(|| make_extraction_state(num_companies))
        .bench_local_refs(|state| {
            galactic_market::sim::resources::run_extraction(state);
        });
}

#[divan::bench(args = [32, 128, 512])]
fn bench_market_clearing(bencher: divan::Bencher, num_orders: usize) {
    bencher
        .with_inputs(|| make_market_state(num_orders))
        .bench_local_refs(|state| {
            galactic_market::sim::markets::clear_orders(state, 1);
        });
}

#[divan::bench(args = [32, 128, 512])]
fn bench_production_phase(bencher: divan::Bencher, num_refineries: usize) {
    bencher
        .with_inputs(|| make_production_state(num_refineries))
        .bench_local_refs(|state| {
            galactic_market::sim::production::run_production(state);
        });
}

#[divan::bench(args = [32, 128, 512])]
fn bench_decisions_phase(bencher: divan::Bencher, num_companies: usize) {
    bencher
        .with_inputs(|| make_decisions_state(num_companies))
        .bench_local_refs(|state| {
            galactic_market::sim::decisions::run_decisions(state, 1);
        });
}
