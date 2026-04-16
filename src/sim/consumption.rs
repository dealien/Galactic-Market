use tracing::debug;

use crate::sim::state::{MarketOrder, SimState};

/// Credits earned per citizen per tick (represents wages, taxes, etc.)
const INCOME_PER_CAPITA_PER_TICK: f64 = 0.01; // Increased from 0.005

/// Ingots demanded per 1,000 citizens per tick.
const DEMAND_PER_1K_POPULATION: i64 = 1;

/// Phase 5: Population consumption.
///
/// Each city's consumer company receives a per-capita income credit, then
/// posts buy orders for all Refined and Consumer goods.
pub fn run_consumption(state: &mut SimState, current_tick: u64) {
    // Snapshot last known prices for budgeting
    let last_prices = state.price_cache.clone();

    // Identify target resource types (Refined and Consumer Goods)
    let target_resource_ids: Vec<i32> = state
        .resource_types
        .values()
        .filter(|r| r.category == "Refined Material" || r.category == "Consumer Good")
        .map(|r| r.id)
        .collect();

    if target_resource_ids.is_empty() {
        return;
    }

    // Collect (city_id, population, consumer_company_id)
    let consumers: Vec<(i32, i64, i32)> = state
        .city_consumer_ids
        .iter()
        .filter_map(|(&city_id, &company_id)| {
            let population = state.cities.get(&city_id)?.population;
            Some((city_id, population, company_id))
        })
        .collect();

    for (city_id, population, company_id) in consumers {
        // 1. Credit per-tick income to the consumer treasury
        let income = population as f64 * INCOME_PER_CAPITA_PER_TICK;
        let mut available_cash = 0.0;

        if let Some(company) = state.companies.get_mut(&company_id) {
            company.cash += income;
            available_cash = company.cash;
        }

        // 2. Budgeting: Split 80% of available cash among target resources
        let total_budget = available_cash * 0.8;
        let budget_per_resource = total_budget / target_resource_ids.len() as f64;

        for &res_id in &target_resource_ids {
            let market_price = last_prices.get(&(city_id, res_id)).copied().unwrap_or(20.0);

            // Consumers are willing to pay slightly above market to ensure fulfillment
            let bid_price = (market_price * 1.1).min(500.0);

            // Demand scales with population
            let ideal_demand = (population / 1000).max(1) * DEMAND_PER_1K_POPULATION;

            // Can we afford the ideal demand?
            let affordable_qty = (budget_per_resource / bid_price) as i64;
            let final_qty = ideal_demand.min(affordable_qty);

            if final_qty > 0 {
                let order_id = state.next_order_id();
                state.market_orders.insert(
                    order_id,
                    MarketOrder {
                        id: order_id,
                        city_id,
                        company_id,
                        resource_type_id: res_id,
                        order_type: "buy".into(),
                        order_kind: "limit".into(),
                        price: bid_price,
                        quantity: final_qty,
                        created_tick: current_tick,
                    },
                );
            }
        }

        debug!(city_id, population, "Consumer demand updated for city");
    }
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, ResourceType, SimState};

    fn make_consumer_state(population: i64, cash: f64) -> SimState {
        let mut s = SimState::new();

        s.resource_types.insert(
            1,
            ResourceType {
                id: 1,
                name: "Test Ingot".into(),
                category: "Refined Material".into(),
            },
        );

        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Consumer City".into(),
                population,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
            },
        );

        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "Test Consumer".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        s.city_consumer_ids.insert(1, 1);
        s
    }

    #[test]
    fn consumer_receives_income_each_tick() {
        let mut state = make_consumer_state(1_000_000, 0.0);
        run_consumption(&mut state, 1);
        let expected_income = 1_000_000.0 * INCOME_PER_CAPITA_PER_TICK;
        assert!((state.companies[&1].cash - expected_income).abs() < 0.001);
    }

    #[test]
    fn consumer_posts_buy_orders() {
        let mut state = make_consumer_state(1_000_000, 10_000.0);
        run_consumption(&mut state, 1);
        let orders: Vec<_> = state.market_orders.values().collect();
        assert!(!orders.is_empty(), "Should have posted buy orders");
        assert_eq!(orders[0].order_type, "buy");
    }
}
