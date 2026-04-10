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
        let (city_id, company_type) = {
            let company = state.companies.get_mut(&company_id).unwrap();
            let (min_interval, max_interval) = eval_interval_range(&company.company_type);
            let jitter = rng.gen_range(min_interval..=max_interval);
            company.next_eval_tick = current_tick + jitter;
            (company.home_city_id, company.company_type.clone())
        };

        let mut orders_to_post = Vec::new();

        // ─── Consumer AI ──────────────────────────────────────────────────────
        if company_type == "consumer" {
            // Consumers represent the population. They buy end products.
            let cash = state.companies.get(&company_id).unwrap().cash;
            if cash > 10.0 {
                let r_id = 2; // Iron Ingots
                let target_price = last_prices.get(&(city_id, r_id)).copied().unwrap_or(20.0);
                
                // Cap the maximum willingness to pay to prevent runaway inflation
                let max_willingness_to_pay = 150.0;
                let bid_price = (target_price * 1.02).min(max_willingness_to_pay);
                
                let qty = ((cash * 0.5) / bid_price) as i64;
                if qty > 0 {
                    orders_to_post.push(MarketOrder {
                        id: 0, // Assigned later
                        city_id,
                        company_id,
                        resource_type_id: r_id,
                        order_type: "buy".into(),
                        price: bid_price,
                        quantity: qty,
                        created_tick: current_tick,
                    });
                }
            }
        }

        // ─── Miner AI ─────────────────────────────────────────────────────────
        let miner_info = state
            .facilities
            .values()
            .find(|f| {
                f.company_id == company_id && f.city_id == city_id && f.facility_type == "mine"
            })
            .map(|f| f.id);

        if let Some(facility_id) = miner_info {
            let planet_id = state.cities.get(&city_id).map(|c| c.body_id).unwrap_or(0);
            let available_ores: Vec<_> = state
                .deposits
                .values()
                .filter(|d| d.body_id == planet_id && d.size_remaining > 0)
                .map(|d| (d.resource_type_id, d.extraction_cost_per_unit))
                .collect();

            // 1. Target selection based on EMA margins
            let mut best_ore_id = None;
            let mut best_margin = f64::NEG_INFINITY;

            for &(res_id, cost) in &available_ores {
                let ema = state
                    .ema_prices
                    .get(&(city_id, res_id))
                    .copied()
                    .unwrap_or(cost * 1.5);
                let margin = ema - cost;
                if margin > best_margin {
                    best_margin = margin;
                    best_ore_id = Some(res_id);
                }
            }

            // 2. Setup switch if needed
            if let Some(best_id) = best_ore_id {
                let facility = state.facilities.get_mut(&facility_id).unwrap();
                if facility.target_resource_id != Some(best_id) {
                    facility.target_resource_id = Some(best_id);
                    facility.setup_ticks_remaining = 3;
                    state.companies.get_mut(&company_id).unwrap().cash -= 100.0;
                    debug!(
                        company_id,
                        new_target = best_id,
                        "Miner switched target resource"
                    );
                }
            }

            // 3. Post sell orders for ALL extracted ores in inventory
            for &(res_id, cost) in &available_ores {
                let key = Inventory::key(company_id, city_id, res_id);
                if let Some(inv) = state.inventories.get(&key).cloned()
                    && inv.quantity > 0
                {
                    // Cost-disciplined pricing:
                    // Start with a healthy margin over extraction cost (e.g. 20%)
                    let base_ask = cost * 1.2;

                    // Look at the market clearing price
                    let market_price = last_prices
                        .get(&(city_id, res_id))
                        .copied()
                        .unwrap_or(base_ask);

                    // We want to sell, so we might drop our price slightly below market
                    // BUT never below our profitable base_ask.
                    // This creates a "gravity" effect towards cost-plus pricing.
                    let ask_price = base_ask.max(market_price * 0.98);

                    orders_to_post.push(MarketOrder {
                        id: 0,
                        city_id,
                        company_id,
                        resource_type_id: res_id,
                        order_type: "sell".into(),
                        price: ask_price,
                        quantity: inv.quantity,
                        created_tick: current_tick,
                    });
                }
            }
        }

        // ─── Refinery AI ──────────────────────────────────────────────────────
        let refinery_info = state
            .facilities
            .values()
            .find(|f| {
                f.company_id == company_id && f.city_id == city_id && f.facility_type == "refinery"
            })
            .map(|f| (f.id, f.capacity));

        if let Some((facility_id, capacity)) = refinery_info {
            let mut recipes_evaluated = Vec::new();
            let mut total_positive_margin = 0.0;
            let labor_margin = 1.5; // Cost of labor/power per recipe execution

            // 1. Evaluate profitability of all refinery recipes
            for recipe in state
                .recipes
                .values()
                .filter(|r| r.facility_type == "refinery")
            {
                // Approximate cost: sum(inputs cost)
                let mut cost_basis = 0.0;
                for input in &recipe.inputs {
                    let in_price = state
                        .ema_prices
                        .get(&(city_id, input.resource_type_id))
                        .copied()
                        .unwrap_or(2.5);
                    cost_basis += in_price * input.quantity as f64;
                }

                let out_price = state
                    .ema_prices
                    .get(&(city_id, recipe.output_resource_id))
                    .copied()
                    .unwrap_or(cost_basis * 1.5);
                let revenue = out_price * recipe.output_qty as f64;
                let margin = revenue - cost_basis;

                if margin > 0.0 {
                    recipes_evaluated.push((
                        recipe.id,
                        margin,
                        cost_basis,
                        out_price,
                        recipe.clone(),
                    ));
                    total_positive_margin += margin;
                }
            }

            // 2. Split capacity proportionally and switch if ratios changed significantly
            if total_positive_margin > 0.0 {
                let mut new_ratios = std::collections::HashMap::new();
                for (id, margin, _, _, _) in &recipes_evaluated {
                    let ratio = margin / total_positive_margin;
                    new_ratios.insert(id.to_string(), ratio);
                }

                let facility = state.facilities.get_mut(&facility_id).unwrap();
                let current_ratios = facility.production_ratios.clone().unwrap_or_default();

                // Compare difference. If changed by > 10% in any ratio, switch it and incur penalty
                let mut significant_change = false;
                for (id, &new_val) in &new_ratios {
                    let old_val = current_ratios.get(id).copied().unwrap_or(0.0);
                    if (new_val - old_val).abs() > 0.10 {
                        significant_change = true;
                        break;
                    }
                }

                if significant_change {
                    facility.production_ratios = Some(new_ratios.clone());
                    facility.setup_ticks_remaining = 5;
                    state.companies.get_mut(&company_id).unwrap().cash -= 500.0;
                    debug!(company_id, "Refinery switched production ratios");
                }

                // 3. Post orders
                for (_r_id, _margin, cost_basis, out_price, recipe) in recipes_evaluated {
                    // Sell all ingots of this type
                    let out_key = Inventory::key(company_id, city_id, recipe.output_resource_id);
                    if let Some(inv) = state.inventories.get(&out_key).cloned()
                        && inv.quantity > 0
                    {
                        // Price ingots at cost + 30% margin, gravity towards market price
                        let base_ask = cost_basis * 1.3;
                        let ask_price = base_ask.max(out_price * 0.98);

                        orders_to_post.push(MarketOrder {
                            id: 0,
                            city_id,
                            company_id,
                            resource_type_id: recipe.output_resource_id,
                            order_type: "sell".into(),
                            price: ask_price,
                            quantity: inv.quantity,
                            created_tick: current_tick,
                        });
                    }

                    // Buy inputs for a batch (e.g. 5 ticks of capacity * input qty)
                    let portion_cap = (capacity as f64
                        * (new_ratios.get(&recipe.id.to_string()).unwrap_or(&0.5)))
                        as i32;
                    if portion_cap > 0 {
                        for input in &recipe.inputs {
                            let buy_qty = (portion_cap * input.quantity * 5) as i64;

                            // EMA as base for bidding
                            let in_price = state
                                .ema_prices
                                .get(&(city_id, input.resource_type_id))
                                .copied()
                                .unwrap_or(2.5);

                            // Profit-disciplined bidding:
                            let max_affordable = (out_price * recipe.output_qty as f64
                                - labor_margin)
                                / (recipe.inputs.iter().map(|i| i.quantity).sum::<i32>() as f64);
                            
                            // Try to buy at a tiny discount to market, but be willing to bid up to market
                            let target_bid = in_price * 0.98;
                            let bid_price = target_bid.min(max_affordable);

                            if bid_price > 0.0 {
                                orders_to_post.push(MarketOrder {
                                    id: 0,
                                    city_id,
                                    company_id,
                                    resource_type_id: input.resource_type_id,
                                    order_type: "buy".into(),
                                    price: bid_price,
                                    quantity: buy_qty,
                                    created_tick: current_tick,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Apply generated orders
        for mut order in orders_to_post {
            let id = state.next_order_id();
            order.id = id;
            state.market_orders.insert(id, order);
        }
    }
}

/// Returns the last clearing price per (city_id, resource_type_id) from the state's persistent cache.
fn last_known_prices(state: &SimState) -> std::collections::HashMap<(i32, i32), f64> {
    state.price_cache.clone()
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, Deposit, Facility, Inventory, SimState};

    fn make_state_with_miner() -> SimState {
        let mut s = SimState::new();

        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Test City".into(),
                population: 0,
            },
        );

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

        s.facilities.insert(
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

        // Company has 50 Iron Ore ready to sell
        s.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 50,
            },
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
        assert!(
            company.next_eval_tick > 1,
            "next_eval_tick should be rescheduled"
        );
    }

    #[test]
    fn company_skips_when_not_due() {
        let mut state = make_state_with_miner();
        // Set next_eval far in the future
        state.companies.get_mut(&1).unwrap().next_eval_tick = 9999;
        run_decisions(&mut state, 1);

        assert!(
            state.market_orders.is_empty(),
            "No orders should be posted if not due"
        );
    }
}
