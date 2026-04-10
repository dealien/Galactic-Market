use rand::Rng;
use tracing::debug;

use crate::sim::state::{Inventory, MarketOrder, SimState};

/// Re-evaluation interval ranges by company type (min, max ticks).
fn eval_interval_range(company_type: &str) -> (u64, u64) {
    match company_type {
        "freelancer" => (1, 5),
        "small_company" => (5, 20),
        "corporation" => (20, 60),
        "megacorp" => (60, 200),
        _ => (5, 20),
    }
}

/// Phase 6: Company AI decisions.
///
/// Each company checks whether the current tick is their scheduled re-evaluation
/// tick. If so, they decide whether to post buy or sell orders based on market
/// prices and their current inventory. After deciding, `next_eval_tick` is
/// updated with a freshly jittered interval.
///
/// **Miners** sell Iron Ore if the estimated clearing price is above extraction cost.
/// **Refineries** buy Iron Ore if the ingot margin is profitable.
///
/// # Examples
/// ```
/// use galactic_market::sim::state::SimState;
/// use galactic_market::sim::decisions::run_decisions;
/// let mut state = SimState::new();
/// run_decisions(&mut state, 1);
/// ```
pub fn run_decisions(state: &mut SimState, current_tick: u64) {
    let mut rng = rand::thread_rng();

    // Snapshot the last known clearing prices for decision-making
    let last_prices = last_known_prices(state);

    // Collect company IDs that are due for re-evaluation
    let due: Vec<i32> = state
        .companies
        .iter()
        .filter(|(_, c)| c.next_eval_tick <= current_tick)
        .map(|(id, _)| *id)
        .collect();

    for company_id in due {
        let company = match state.companies.get_mut(&company_id) {
            Some(c) => c,
            None => continue,
        };

        // Schedule next evaluation
        let (min_interval, max_interval) = eval_interval_range(&company.company_type.clone());
        let jitter = rng.gen_range(min_interval..=max_interval);
        company.next_eval_tick = current_tick + jitter;

        let city_id = company.home_city_id;

        // --- Miner AI ---
        // If we have Iron Ore in inventory, post a sell order above extraction cost
        let ore_resource_id = 1; // Iron Ore
        let ore_key = Inventory::key(company_id, city_id, ore_resource_id);

        if let Some(inv) = state.inventories.get(&ore_key).cloned()
            && inv.quantity > 0
        {
            // Target sell price: extraction cost + 20% margin
            let extraction_cost = extraction_cost_for(state, city_id);
            let ask_price = extraction_cost * 1.2;

            // Check if the market is offering better; if so aim slightly higher
            let market_price = last_prices
                .get(&(city_id, ore_resource_id))
                .copied()
                .unwrap_or(ask_price);
            let ask_price = ask_price.max(market_price * 0.95); // don't undercut too hard

            let order_id = state.next_order_id();
            state.market_orders.insert(
                order_id,
                MarketOrder {
                    id: order_id,
                    city_id,
                    company_id,
                    resource_type_id: ore_resource_id,
                    order_type: "sell".into(),
                    price: ask_price,
                    quantity: inv.quantity,
                    created_tick: current_tick,
                },
            );

            debug!(company_id, city_id, qty = inv.quantity, price = ask_price, "Sell order posted");
        }


        // --- Refinery AI ---
        // Any company that owns a refinery should: sell ingots it has produced,
        // and buy ore if the ingot margin makes it profitable.
        let ingot_resource_id = 2; // Iron Ingot
        let has_refinery = state.facilities.values().any(|f| {
            f.company_id == company_id && f.city_id == city_id && f.facility_type == "refinery"
        });

        if has_refinery {
            let ore_price = last_prices
                .get(&(city_id, ore_resource_id))
                .copied()
                .unwrap_or(2.5); // default just above extraction cost

            let ingot_price = last_prices
                .get(&(city_id, ingot_resource_id))
                .copied()
                .unwrap_or(10.0); // default consumer-level price

            // Sell ingots if we have any in inventory
            let ingot_key = Inventory::key(company_id, city_id, ingot_resource_id);
            if let Some(ingot_inv) = state.inventories.get(&ingot_key).cloned()
                && ingot_inv.quantity > 0
            {
                // Price ingots at ore_cost × 3 (recipe ratio) + 30% margin
                let cost_basis = ore_price * 3.0;
                let ask_price = cost_basis * 1.3;
                // But if market clearing price is higher, ride it
                let ask_price = ask_price.max(ingot_price * 0.95);

                let order_id = state.next_order_id();
                state.market_orders.insert(
                    order_id,
                    MarketOrder {
                        id: order_id,
                        city_id,
                        company_id,
                        resource_type_id: ingot_resource_id,
                        order_type: "sell".into(),
                        price: ask_price,
                        quantity: ingot_inv.quantity,
                        created_tick: current_tick,
                    },
                );
                debug!(company_id, city_id, qty = ingot_inv.quantity, price = ask_price, "Ingot sell order posted");
            }

            // Buy ore if the ingot margin is profitable (1 ingot = 3 ore + labor)
            let labor_margin = 1.5;
            if ingot_price > ore_price * 3.0 + labor_margin {
                let order_id = state.next_order_id();
                state.market_orders.insert(
                    order_id,
                    MarketOrder {
                        id: order_id,
                        city_id,
                        company_id,
                        resource_type_id: ore_resource_id,
                        order_type: "buy".into(),
                        price: ore_price * 1.1, // pay up to 10% over last price for ore
                        quantity: 30,            // buy in batches
                        created_tick: current_tick,
                    },
                );
            }
        }
    }
}

/// Returns the last clearing price per (city_id, resource_type_id) from the state's persistent cache.
fn last_known_prices(state: &SimState) -> std::collections::HashMap<(i32, i32), f64> {
    state.price_cache.clone()
}

/// Returns a representative extraction cost for the deposit linked to a city's planet.
fn extraction_cost_for(state: &SimState, city_id: i32) -> f64 {
    let body_id = state.cities.get(&city_id).map(|c| c.body_id).unwrap_or(0);
    state
        .deposits
        .values()
        .find(|d| d.body_id == body_id)
        .map(|d| d.extraction_cost_per_unit)
        .unwrap_or(2.0)
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, Deposit, Inventory, SimState};

    fn make_state_with_miner() -> SimState {
        let mut s = SimState::new();

        s.cities.insert(1, City { id: 1, body_id: 1, name: "Test City".into(), population: 0 });

        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "Mining Co".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 200.0,
                debt: 0.0,
                next_eval_tick: 1,
            },
        );

        s.deposits.insert(
            1,
            Deposit {
                id: 1,
                body_id: 1,
                resource_type_id: 1,
                size_total: 1000,
                size_remaining: 1000,
                extraction_cost_per_unit: 2.0,
            },
        );

        // Company has 50 Iron Ore ready to sell
        s.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory { company_id: 1, city_id: 1, resource_type_id: 1, quantity: 50 },
        );

        s
    }

    #[test]
    fn miner_posts_sell_order_when_inventory_available() {
        let mut state = make_state_with_miner();
        run_decisions(&mut state, 1);

        // At least one sell order should have been posted
        assert!(state.market_orders.values().any(|o| o.order_type == "sell"));
    }

    #[test]
    fn company_reschedules_next_eval() {
        let mut state = make_state_with_miner();
        run_decisions(&mut state, 1);

        let company = &state.companies[&1];
        assert!(company.next_eval_tick > 1, "next_eval_tick should be rescheduled");
    }

    #[test]
    fn company_skips_when_not_due() {
        let mut state = make_state_with_miner();
        // Set next_eval far in the future
        state.companies.get_mut(&1).unwrap().next_eval_tick = 9999;
        run_decisions(&mut state, 1);

        assert!(state.market_orders.is_empty(), "No orders should be posted if not due");
    }
}
