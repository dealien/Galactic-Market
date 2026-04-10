use tracing::debug;

use crate::sim::state::{MarketOrder, SimState};

/// Iron Ingot resource type ID (hardcoded for Stage 1; will be data-driven in Stage 2).
const IRON_INGOT_RESOURCE_ID: i32 = 2;

/// Credits earned per citizen per tick (represents wages, taxes, etc.)
const INCOME_PER_CAPITA_PER_TICK: f64 = 0.001;

/// Ingots demanded per 10,000 citizens per tick.
const DEMAND_PER_10K_POPULATION: i64 = 1;

/// Maximum price consumers are willing to pay for an iron ingot.
/// This is deliberately above extraction + refining cost to ensure trades clear.
const CONSUMER_WILLINGNESS_TO_PAY: f64 = 20.0;

/// Phase 3: Population consumption.
///
/// Each city's consumer company receives a per-capita income credit, then
/// posts a buy order for Iron Ingots sized by population. This creates the
/// demand side of the economic cycle:
///
/// ```text
/// Miners → extract ore → sell to market
/// Refiners → buy ore → smelt ingots → sell to market
/// Consumers → post buy orders → purchase ingots → cash flows to producers
/// ```
///
/// Consumer cash accumulates between ticks if no ingots are available,
/// creating a demand pressure that rises until the market can supply it.
///
/// # Examples
/// ```
/// use galactic_market::sim::state::SimState;
/// use galactic_market::sim::consumption::run_consumption;
/// let mut state = SimState::new();
/// run_consumption(&mut state, 1);
/// ```
pub fn run_consumption(state: &mut SimState, current_tick: u64) {
    // Collect (city_id, population, consumer_company_id) for cities with a consumer
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
        if let Some(company) = state.companies.get_mut(&company_id) {
            company.cash += income;
        }

        // 2. Calculate demand quantity based on population
        let demand_qty = (population / 10_000).max(1) * DEMAND_PER_10K_POPULATION;

        // 3. Only post a buy order if the consumer has enough cash to cover
        let required_cash = demand_qty as f64 * CONSUMER_WILLINGNESS_TO_PAY;
        let available_cash = state
            .companies
            .get(&company_id)
            .map(|c| c.cash)
            .unwrap_or(0.0);

        if available_cash < required_cash {
            // Can only afford a partial order
            let affordable_qty = (available_cash / CONSUMER_WILLINGNESS_TO_PAY) as i64;
            if affordable_qty == 0 {
                continue;
            }
        }

        let order_id = state.next_order_id();
        state.market_orders.insert(
            order_id,
            MarketOrder {
                id: order_id,
                city_id,
                company_id,
                resource_type_id: IRON_INGOT_RESOURCE_ID,
                order_type: "buy".into(),
                price: CONSUMER_WILLINGNESS_TO_PAY,
                quantity: demand_qty,
                created_tick: current_tick,
            },
        );

        debug!(
            city_id,
            population,
            demand_qty,
            "Consumer buy order posted"
        );
    }
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, SimState};

    fn make_consumer_state(population: i64, cash: f64) -> SimState {
        let mut s = SimState::new();

        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Consumer City".into(),
                population,
            },
        );

        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "City 1 Consumers".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash,
                debt: 0.0,
                next_eval_tick: 1,
            },
        );

        s.city_consumer_ids.insert(1, 1); // city 1 → company 1
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
    fn consumer_posts_buy_order_for_ingots() {
        let mut state = make_consumer_state(1_000_000, 10_000.0);
        run_consumption(&mut state, 1);
        let orders: Vec<_> = state.market_orders.values().collect();
        assert!(!orders.is_empty(), "Should have posted at least one buy order");
        assert_eq!(orders[0].resource_type_id, IRON_INGOT_RESOURCE_ID);
        assert_eq!(orders[0].order_type, "buy");
        assert_eq!(orders[0].price, CONSUMER_WILLINGNESS_TO_PAY);
    }

    #[test]
    fn consumer_demand_scales_with_population() {
        let mut small = make_consumer_state(100_000, 100_000.0);
        let mut large = make_consumer_state(10_000_000, 100_000.0);

        run_consumption(&mut small, 1);
        run_consumption(&mut large, 1);

        let small_qty = small.market_orders.values().next().unwrap().quantity;
        let large_qty = large.market_orders.values().next().unwrap().quantity;

        assert!(large_qty > small_qty, "Larger city should demand more ingots");
    }

    #[test]
    fn consumer_skips_if_insufficient_cash() {
        // Only 0.5 credits — can't afford even 1 ingot at 20.0
        let mut state = make_consumer_state(1_000_000, 0.5);
        run_consumption(&mut state, 1);

        // After income is added (1000 credits), it should now post an order
        // because income bumps cash above threshold
        assert!(
            !state.market_orders.is_empty(),
            "After income credit, consumer should post order"
        );
    }
}
