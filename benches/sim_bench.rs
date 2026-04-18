use galactic_market::sim::SimState;
use galactic_market::sim::state::{
    CelestialBody, City, Company, Deposit, Facility, Inventory, Loan, MarketOrder, Recipe,
    RecipeInput, ResourceType, Sector, StarSystem, TradeRoute,
};

fn main() {
    divan::main();
}

/// Helper to setup a valid universe hierarchy for benchmarks that require pathfinding.
fn setup_benchmark_hierarchy(state: &mut SimState, num_cities: usize) {
    // Sector
    state.sectors.insert(
        1,
        Sector {
            id: 1,
            empire_id: 1,
            name: "Sector 1".into(),
        },
    );

    // Systems (1 per 8 cities)
    let num_systems = (num_cities / 8).max(1);
    for i in 1..=num_systems {
        state.star_systems.insert(
            i as i32,
            StarSystem {
                id: i as i32,
                sector_id: 1,
                name: format!("Sys {i}"),
            },
        );
    }

    // Bodies (1 per 4 cities)
    let num_bodies = (num_cities / 4).max(1);
    for i in 1..=num_bodies {
        let sys_id = ((i - 1) / 2 + 1) as i32;
        state.celestial_bodies.insert(
            i as i32,
            CelestialBody {
                id: i as i32,
                system_id: sys_id,
                name: format!("Body {i}"),
                fertility: 1.5,
            },
        );
    }
}

fn make_extraction_state(num_companies: usize) -> SimState {
    let mut state = SimState::new();
    setup_benchmark_hierarchy(&mut state, num_companies);

    for i in 1..=(num_companies as i32) {
        let body_id = (i - 1) / 4 + 1;
        state.cities.insert(
            i,
            City {
                id: i,
                body_id,
                name: format!("City {i}"),
                population: 0,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
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
                last_trade_tick: 0,
            },
        );

        state.deposits.insert(
            i,
            Deposit {
                id: i,
                body_id,
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
    setup_benchmark_hierarchy(&mut state, 1);

    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "Market City".into(),
            population: 0,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
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
            last_trade_tick: 0,
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
                last_trade_tick: 0,
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
                order_kind: "limit".into(),
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
                order_kind: "limit".into(),
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
    setup_benchmark_hierarchy(&mut state, num_refineries);

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
        let body_id = (i - 1) / 4 + 1;
        state.cities.insert(
            i,
            City {
                id: i,
                body_id,
                name: format!("City {i}"),
                population: 0,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
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
                last_trade_tick: 0,
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
    setup_benchmark_hierarchy(&mut state, num_companies);

    for i in 1..=(num_companies as i32) {
        let body_id = (i - 1) / 4 + 1;
        state.cities.insert(
            i,
            City {
                id: i,
                body_id,
                name: format!("City {i}"),
                population: 0,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
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
                last_trade_tick: 0,
            },
        );
        state.deposits.insert(
            i,
            Deposit {
                id: i,
                body_id,
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

fn make_finance_state(num_companies: usize) -> SimState {
    let mut state = SimState::new();
    for i in 1..=(num_companies as i32) {
        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Co {i}"),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 100.0,
                debt: 1000.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.add_loan(Loan {
            id: i,
            company_id: i,
            lender_company_id: None,
            principal: 1000.0,
            interest_rate: 0.05,
            balance: 1000.0,
        });
    }
    state
}

fn make_logistics_state(num_routes: usize) -> SimState {
    let mut state = SimState::new();
    for i in 1..=(num_routes as i32) {
        state.trade_routes.insert(
            i,
            TradeRoute {
                id: i,
                company_id: 1,
                origin_city_id: 1,
                dest_city_id: 2,
                resource_type_id: 1,
                quantity: 100,
                arrival_tick: 1, // Ready to deliver
            },
        );
    }
    state
}

fn make_spatial_state() -> SimState {
    let mut state = SimState::new();
    state.sectors.insert(
        1,
        Sector {
            id: 1,
            empire_id: 1,
            name: "S1".into(),
        },
    );
    state.star_systems.insert(
        1,
        StarSystem {
            id: 1,
            sector_id: 1,
            name: "Sys1".into(),
        },
    );
    state.celestial_bodies.insert(
        1,
        CelestialBody {
            id: 1,
            system_id: 1,
            name: "B1".into(),
            fertility: 1.5,
        },
    );
    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "C1".into(),
            population: 0,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );
    state.cities.insert(
        2,
        City {
            id: 2,
            body_id: 1,
            name: "C2".into(),
            population: 0,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );
    state
}

fn make_merchant_state(num_merchants: usize) -> SimState {
    let mut state = SimState::new();
    setup_benchmark_hierarchy(&mut state, 32);

    // 32 cities
    for i in 1..=32 {
        state.cities.insert(
            i,
            City {
                id: i,
                body_id: (i - 1) / 4 + 1,
                name: format!("City {i}"),
                population: 1_000_000,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 10000,
            },
        );
    }

    // 7 resources
    for i in 1..=7 {
        state.resource_types.insert(
            i,
            ResourceType {
                id: i,
                name: format!("Res {i}"),
                category: "Refined Material".into(),
                is_vital: false,
            },
        );
    }

    // N merchants
    for i in 1..=(num_merchants as i32) {
        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Merchant {i}"),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 1_000_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
    }

    state
}

fn make_advanced_market_state(num_orders: usize) -> SimState {
    let mut state = SimState::new();
    setup_benchmark_hierarchy(&mut state, 1);

    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "C1".into(),
            population: 0,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );

    // Mixer of Market and Limit orders
    for i in 1..=(num_orders as i32) {
        state.companies.insert(
            i,
            Company {
                id: i,
                name: format!("Co {i}"),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 1_000_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.inventories.insert(
            Inventory::key(i, 1, 1),
            Inventory {
                company_id: i,
                city_id: 1,
                resource_type_id: 1,
                quantity: 1000,
            },
        );

        let kind = if i % 2 == 0 { "market" } else { "limit" };
        state.market_orders.insert(
            i * 2,
            MarketOrder {
                id: i * 2,
                city_id: 1,
                company_id: i,
                resource_type_id: 1,
                order_type: "sell".into(),
                order_kind: kind.into(),
                price: 10.0,
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
                order_kind: kind.into(),
                price: 10.0,
                quantity: 10,
                created_tick: 0,
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

#[divan::bench(args = [32, 128, 512])]
fn bench_finance_phase(bencher: divan::Bencher, num_companies: usize) {
    bencher
        .with_inputs(|| make_finance_state(num_companies))
        .bench_local_refs(|state| {
            galactic_market::sim::finance::run_finance(state);
        });
}

#[divan::bench(args = [32, 128, 512])]
fn bench_logistics_phase(bencher: divan::Bencher, num_routes: usize) {
    bencher
        .with_inputs(|| make_logistics_state(num_routes))
        .bench_local_refs(|state| {
            galactic_market::sim::logistics::run_logistics(state, 1);
        });
}

#[divan::bench]
fn bench_spatial_lookup(bencher: divan::Bencher) {
    bencher
        .with_inputs(make_spatial_state)
        .bench_local_refs(|state| {
            let _ = galactic_market::sim::logistics::get_transport_info(state, 1, 2);
        });
}

#[divan::bench(args = [1, 4, 16])]
fn bench_merchant_arbitrage_scan(bencher: divan::Bencher, num_merchants: usize) {
    bencher
        .with_inputs(|| make_merchant_state(num_merchants))
        .bench_local_refs(|state| {
            galactic_market::sim::decisions::run_decisions(state, 1);
        });
}

#[divan::bench(args = [32, 128, 512])]
fn bench_advanced_market_clearing(bencher: divan::Bencher, num_orders: usize) {
    bencher
        .with_inputs(|| make_advanced_market_state(num_orders))
        .bench_local_refs(|state| {
            galactic_market::sim::markets::clear_orders(state, 1);
        });
}
