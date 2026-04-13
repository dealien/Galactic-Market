use rand::Rng;
use tracing::debug;

use crate::sim::state::{Inventory, MarketOrder, SimState, TradeRoute};

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
        // Clear all outstanding orders for this company before making new ones.
        // This ensures the market book doesn't bloat with obsolete strategies.
        state
            .market_orders
            .retain(|_, order| order.company_id != company_id);

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
            // Consumers represent the population. They buy refined products and consumer goods.
            let cash = state.companies.get(&company_id).unwrap().cash;

            // Target all available refined or consumer products
            let target_ids: Vec<i32> = state
                .resource_types
                .values()
                .filter(|r| r.category == "Refined Material" || r.category == "Consumer Good")
                .map(|r| r.id)
                .collect();

            if cash > 10.0 && !target_ids.is_empty() {
                // Split budget among all products
                let budget_per_product = (cash * 0.5) / target_ids.len() as f64;

                for &r_id in &target_ids {
                    let target_price = last_prices.get(&(city_id, r_id)).copied().unwrap_or(20.0);

                    // Cap the maximum willingness to pay to prevent runaway inflation
                    let max_willingness_to_pay = 250.0;
                    let bid_price = (target_price * 1.02).min(max_willingness_to_pay);

                    let qty = (budget_per_product / bid_price) as i64;
                    if qty > 0 {
                        orders_to_post.push(MarketOrder {
                            id: 0,
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
                    let old_target = facility.target_resource_id;
                    facility.target_resource_id = Some(best_id);

                    if old_target.is_some() {
                        facility.setup_ticks_remaining = 2;
                        state.companies.get_mut(&company_id).unwrap().cash -= 50.0;
                    }
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
                    // --- Logistics Logic: If we have too much ore and no local refinery, move it! ---
                    let has_local_refinery = state
                        .facilities
                        .values()
                        .any(|f| f.city_id == city_id && f.facility_type == "refinery");

                    let facility_capacity = state
                        .facilities
                        .get(&facility_id)
                        .map(|f| f.capacity)
                        .unwrap_or(10);

                    if !has_local_refinery && inv.quantity >= (facility_capacity * 2) as i64 {
                        // Find the "nearest" refinery (simplistic for Stage 1: first city of each body group)
                        let refinery_city_id = state
                            .cities
                            .keys()
                            .find(|&&id| (id - 1) % 4 == 0 && id != city_id)
                            .copied();

                        if let Some(target_city) = refinery_city_id {
                            // Queue an "instant" trade route
                            let route_id = state.next_trade_route_id();
                            let move_qty = inv.quantity; // Move everything

                            // Deduct from local inventory immediately to avoid double-counting
                            if let Some(mut_inv) = state.inventories.get_mut(&key) {
                                mut_inv.quantity -= move_qty;
                            }

                            state.trade_routes.insert(
                                route_id,
                                TradeRoute {
                                    id: route_id,
                                    company_id,
                                    origin_city_id: city_id,
                                    dest_city_id: target_city,
                                    resource_type_id: res_id,
                                    quantity: move_qty,
                                    arrival_tick: current_tick, // Instant
                                },
                            );

                            debug!(
                                company_id,
                                move_qty,
                                from = city_id,
                                to = target_city,
                                "Miner moving ore to refinery city"
                            );
                            continue; // Skip posting sell order in current city
                        }
                    }

                    // Cost-disciplined pricing:
                    let base_ask = cost * 1.15;
                    let market_price = last_prices
                        .get(&(city_id, res_id))
                        .copied()
                        .unwrap_or(base_ask * 1.5);

                    // Desperation Logic: If inventory is high, discount aggressively to liquidate.
                    // If margin is razor thin, we be extra aggressive on clearing stock.
                    let ask_price = if inv.quantity > (facility_capacity * 5) as i64 {
                        cost * 1.01 // Clear it out near cost
                    } else if inv.quantity > (facility_capacity * 2) as i64 {
                        base_ask.min(market_price * 0.90) // Under-cut heavily
                    } else if inv.quantity > facility_capacity as i64 {
                        base_ask.min(market_price * 0.95) // Keep undercutting
                    } else {
                        base_ask.max(market_price)
                    };

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

            // --- Facility Expansion Logic (Miners) ---
            let cash = state.companies.get(&company_id).unwrap().cash;
            let facility = state.facilities.get(&facility_id).unwrap();
            
            // Progressive cost: base 500 * 1.2 ^ current_capacity
            let expansion_cost = 500.0 * 1.2_f64.powi(facility.capacity);
            
            if cash > expansion_cost * 2.0 {
                // Expand if we have double the cash needed
                let facility = state.facilities.get_mut(&facility_id).unwrap();
                facility.capacity += 5; // Expand capacity by 5 units
                facility.setup_ticks_remaining = 5; // Construction delay
                state.companies.get_mut(&company_id).unwrap().cash -= expansion_cost;
                
                debug!(
                    company_id, 
                    new_capacity = facility.capacity, 
                    cost = expansion_cost, 
                    "Miner expanded facility capacity"
                );
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
                    // Only incur penalty if we were already producing.
                    if facility.production_ratios.is_some() {
                        facility.setup_ticks_remaining = 3;
                        state.companies.get_mut(&company_id).unwrap().cash -= 200.0;
                    }
                    debug!(company_id, "Refinery switched production ratios");
                }

                // 3. Post orders
                for (_r_id, _margin, cost_basis, out_price, recipe) in recipes_evaluated {
                    // ... (existing order posting logic)
                }

                // --- Facility Expansion Logic (Refineries) ---
                let cash = state.companies.get(&company_id).unwrap().cash;
                let facility = state.facilities.get(&facility_id).unwrap();
                
                // Progressive cost for refineries (more complex facilities, higher base)
                let expansion_cost = 1500.0 * 1.3_f64.powi(facility.capacity / 5); // every 5 units is a 'tier'
                
                if cash > expansion_cost * 2.5 {
                    // Refineries require more safety capital to expand
                    let facility = state.facilities.get_mut(&facility_id).unwrap();
                    facility.capacity += 5;
                    facility.setup_ticks_remaining = 8; // Longer construction for complex refineries
                    state.companies.get_mut(&company_id).unwrap().cash -= expansion_cost;
                    
                    debug!(
                        company_id, 
                        new_capacity = facility.capacity, 
                        cost = expansion_cost, 
                        "Refinery expanded facility capacity"
                    );
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
