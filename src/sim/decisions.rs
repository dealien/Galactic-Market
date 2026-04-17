use rand::Rng;
use tracing::debug;

use crate::sim::logistics::get_transport_info;
use crate::sim::state::{Facility, Inventory, MarketOrder, SimState, TradeRoute};

/// Re-evaluation interval ranges by company type (min, max ticks).
fn eval_interval_range(company_type: &str) -> (u64, u64) {
    match company_type {
        "freelancer" => (1, 5),
        "small_company" => (5, 20),
        "corporation" => (20, 60),
        "megacorp" => (60, 200),
        "merchant" => (1, 2),
        _ => (5, 20),
    }
}

/// Phase 6: Company AI decisions.
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
        // Copy relevant company data locally to avoid immutable borrow while mutating state
        let (status, home_city_id, last_trade_tick) = {
            let c = state.companies.get(&company_id).unwrap();
            (c.status.clone(), c.home_city_id, c.last_trade_tick)
        };

        // --- Liquidation AI: Post Fire-Sale Orders ---
        if status == "bankrupt" {
            let mut orders_to_post = Vec::new();

            let company_inventories: Vec<_> = state
                .inventories
                .values()
                .filter(|inv| inv.company_id == company_id && inv.quantity > 0)
                .cloned()
                .collect();

            for inv in company_inventories {
                let market_price = state
                    .ema_prices
                    .get(&(inv.city_id, inv.resource_type_id))
                    .copied()
                    .unwrap_or(10.0);

                let fire_sale_price = market_price * 0.5;

                orders_to_post.push(MarketOrder {
                    id: 0,
                    city_id: inv.city_id,
                    company_id,
                    resource_type_id: inv.resource_type_id,
                    order_type: "sell".into(),
                    order_kind: "limit".into(),
                    price: fire_sale_price,
                    quantity: inv.quantity,
                    created_tick: current_tick,
                });
            }

            for mut order in orders_to_post {
                order.id = state.next_order_id();
                state.market_orders.insert(order.id, order);
            }
            continue;
        }

        if status != "active" {
            continue;
        }

        // Clear all outstanding orders for this company before making new ones.
        state
            .market_orders
            .retain(|_, order| order.company_id != company_id);

        let company_type = {
            let company = state.companies.get_mut(&company_id).unwrap();

            // --- Promotion Logic ---
            if company.company_type == "freelancer" && company.cash >= 10000.0 {
                company.company_type = "small_company".into();
                debug!(company_id, "Freelancer promoted to Small Company!");
            } else if company.company_type == "small_company" && company.cash >= 100000.0 {
                company.company_type = "corporation".into();
                debug!(company_id, "Small Company promoted to Corporation!");
            }

            let (min_interval, max_interval) = eval_interval_range(&company.company_type);
            let jitter = rng.gen_range(min_interval..=max_interval);
            company.next_eval_tick = current_tick + jitter;
            company.company_type.clone()
        };

        let mut orders_to_post = Vec::new();

        // --- Merchant AI (Arbitrageur) ──────────────────────────────────────────
        if company_type == "merchant" {
            let company_cash = state.companies.get(&company_id).unwrap().cash;
            if company_cash > 1000.0 {
                let mut best_arbitrage = None;
                let mut best_profit_margin = 0.0;

                for &res_id in state.resource_types.keys() {
                    for &origin_city_id in state.cities.keys() {
                        let buy_price = state
                            .ema_prices
                            .get(&(origin_city_id, res_id))
                            .copied()
                            .unwrap_or(1000.0);
                        for &dest_city_id in state.cities.keys() {
                            if origin_city_id == dest_city_id {
                                continue;
                            }
                            let sell_price = state
                                .ema_prices
                                .get(&(dest_city_id, res_id))
                                .copied()
                                .unwrap_or(0.0);
                            let transport = get_transport_info(state, origin_city_id, dest_city_id);
                            let total_cost = buy_price + transport.cost_per_unit;
                            let profit = sell_price - total_cost;

                            if profit > best_profit_margin && profit > (buy_price * 0.001) {
                                best_profit_margin = profit;
                                best_arbitrage =
                                    Some((res_id, origin_city_id, dest_city_id, buy_price));
                            }
                        }
                    }
                }

                if let Some((res_id, origin, _dest, buy_price)) = best_arbitrage {
                    let max_affordable = (company_cash * 0.5 / buy_price) as i64;
                    let qty = 50.min(max_affordable);
                    if qty > 0 {
                        orders_to_post.push(MarketOrder {
                            id: 0,
                            city_id: origin,
                            company_id,
                            resource_type_id: res_id,
                            order_type: "buy".into(),
                            order_kind: "market".into(),
                            price: buy_price * 1.1,
                            quantity: qty,
                            created_tick: current_tick,
                        });
                        debug!(
                            company_id,
                            res_id,
                            from = origin,
                            margin = best_profit_margin,
                            "Merchant initiated arbitrage buy"
                        );
                    }
                }
            }

            let company_inventories: Vec<_> = state
                .inventories
                .values()
                .filter(|inv| inv.company_id == company_id && inv.quantity > 0)
                .cloned()
                .collect();

            for inv in company_inventories {
                let local_ema = state
                    .ema_prices
                    .get(&(inv.city_id, inv.resource_type_id))
                    .copied()
                    .unwrap_or(0.0);
                let mut best_dest = inv.city_id;
                let mut best_price_after_transport = local_ema;
                let mut best_transport_ticks = 0;

                for &dest_city_id in state.cities.keys() {
                    if dest_city_id == inv.city_id {
                        continue;
                    }
                    let dest_ema = state
                        .ema_prices
                        .get(&(dest_city_id, inv.resource_type_id))
                        .copied()
                        .unwrap_or(0.0);
                    let transport = get_transport_info(state, inv.city_id, dest_city_id);
                    if dest_ema - transport.cost_per_unit > best_price_after_transport {
                        best_price_after_transport = dest_ema - transport.cost_per_unit;
                        best_dest = dest_city_id;
                        best_transport_ticks = transport.ticks;
                    }
                }

                if best_dest != inv.city_id {
                    let transport = get_transport_info(state, inv.city_id, best_dest);
                    let total_ship_cost = transport.cost_per_unit * inv.quantity as f64;
                    let cash = state.companies.get(&company_id).unwrap().cash;
                    if cash >= total_ship_cost {
                        state.companies.get_mut(&company_id).unwrap().cash -= total_ship_cost;
                        if let Some(mut_inv) = state.inventories.get_mut(&Inventory::key(
                            company_id,
                            inv.city_id,
                            inv.resource_type_id,
                        )) {
                            mut_inv.quantity = 0;
                        }
                        let route_id = state.next_trade_route_id();
                        state.trade_routes.insert(
                            route_id,
                            TradeRoute {
                                id: route_id,
                                company_id,
                                origin_city_id: inv.city_id,
                                dest_city_id: best_dest,
                                resource_type_id: inv.resource_type_id,
                                quantity: inv.quantity,
                                arrival_tick: current_tick + best_transport_ticks,
                            },
                        );
                        debug!(
                            company_id,
                            qty = inv.quantity,
                            from = inv.city_id,
                            to = best_dest,
                            "Merchant shipping inventory"
                        );
                    }
                } else {
                    orders_to_post.push(MarketOrder {
                        id: 0,
                        city_id: inv.city_id,
                        company_id,
                        resource_type_id: inv.resource_type_id,
                        order_type: "sell".into(),
                        order_kind: "limit".into(),
                        price: local_ema * 0.98,
                        quantity: inv.quantity,
                        created_tick: current_tick,
                    });
                }
            }
        }

        // --- New Facility Scouting ---
        if company_type == "small_company" || company_type == "corporation" {
            let company_cash = state.companies.get(&company_id).unwrap().cash;
            let facility_count = state
                .facilities
                .values()
                .filter(|f| f.company_id == company_id)
                .count();
            let max_facilities = if company_type == "small_company" {
                3
            } else {
                10
            };

            if facility_count < max_facilities {
                // 1. Scouting for Mines
                let mut best_mine_target = None;
                let mut best_mine_profit = 0.0;
                let mine_cost = 5000.0;

                if company_cash > mine_cost * 3.0 {
                    for (&city_id_target, city) in &state.cities {
                        if state.facilities.values().any(|f| {
                            f.company_id == company_id
                                && f.city_id == city_id_target
                                && f.facility_type == "mine"
                        }) {
                            continue;
                        }
                        let planet_id = city.body_id;
                        let deposits: Vec<_> = state
                            .deposits
                            .values()
                            .filter(|d| d.body_id == planet_id && d.size_remaining > 5000)
                            .collect();

                        for d in deposits {
                            let ema = state
                                .ema_prices
                                .get(&(city_id_target, d.resource_type_id))
                                .copied()
                                .unwrap_or(d.extraction_cost_per_unit * 1.5);
                            let margin = ema - d.extraction_cost_per_unit;
                            if margin > best_mine_profit {
                                best_mine_profit = margin;
                                best_mine_target = Some(city_id_target);
                            }
                        }
                    }
                }

                #[allow(clippy::collapsible_if)]
                if let Some(target_city_id) = best_mine_target {
                    if best_mine_profit > 1.0 {
                        let facility_id = state.next_facility_id();
                        state.facilities.insert(
                            facility_id,
                            Facility {
                                id: facility_id,
                                city_id: target_city_id,
                                company_id,
                                facility_type: "mine".into(),
                                capacity: 10,
                                setup_ticks_remaining: 20,
                                target_resource_id: None,
                                production_ratios: None,
                            },
                        );
                        state.companies.get_mut(&company_id).unwrap().cash -= mine_cost;
                        debug!(company_id, target_city_id, "Constructing new Mine facility");
                    }
                }

                // 2. Scouting for Refineries
                let mut best_refinery_target = None;
                let mut best_refinery_profit = 0.0;
                let refinery_cost = 15000.0;

                if company_cash > refinery_cost * 3.0 {
                    for &city_id_target in state.cities.keys() {
                        if state.facilities.values().any(|f| {
                            f.company_id == company_id
                                && f.city_id == city_id_target
                                && f.facility_type == "refinery"
                        }) {
                            continue;
                        }
                        let mut total_margin = 0.0;
                        for recipe in state
                            .recipes
                            .values()
                            .filter(|r| r.facility_type == "refinery")
                        {
                            let out_price = state
                                .ema_prices
                                .get(&(city_id_target, recipe.output_resource_id))
                                .copied()
                                .unwrap_or(30.0);
                            let margin = out_price - 10.0;
                            if margin > 0.0 {
                                total_margin += margin;
                            }
                        }
                        if total_margin > best_refinery_profit {
                            best_refinery_profit = total_margin;
                            best_refinery_target = Some(city_id_target);
                        }
                    }
                }

                #[allow(clippy::collapsible_if)]
                if let Some(target_city_id) = best_refinery_target {
                    if best_refinery_profit > 10.0 {
                        let facility_id = state.next_facility_id();
                        state.facilities.insert(
                            facility_id,
                            Facility {
                                id: facility_id,
                                city_id: target_city_id,
                                company_id,
                                facility_type: "refinery".into(),
                                capacity: 5,
                                setup_ticks_remaining: 30,
                                target_resource_id: None,
                                production_ratios: None,
                            },
                        );
                        state.companies.get_mut(&company_id).unwrap().cash -= refinery_cost;
                        debug!(
                            company_id,
                            target_city_id, "Constructing new Refinery facility"
                        );
                    }
                }
            }
        }

        // ─── Consumer AI ──────────────────────────────────────────────────────
        if company_type == "consumer" {
            let cash = state.companies.get(&company_id).unwrap().cash;
            let target_ids: Vec<i32> = state
                .resource_types
                .values()
                .filter(|r| r.category == "Refined Material" || r.category == "Consumer Good")
                .map(|r| r.id)
                .collect();

            if cash > 10.0 && !target_ids.is_empty() {
                let budget_per_product = (cash * 0.5) / target_ids.len() as f64;
                for &r_id in &target_ids {
                    let target_price = last_prices
                        .get(&(home_city_id, r_id))
                        .copied()
                        .unwrap_or(20.0);
                    let bid_price = (target_price * 1.02).min(1000.0);
                    let qty = (budget_per_product / bid_price) as i64;
                    if qty > 0 {
                        orders_to_post.push(MarketOrder {
                            id: 0,
                            city_id: home_city_id,
                            company_id,
                            resource_type_id: r_id,
                            order_type: "buy".into(),
                            order_kind: "limit".into(),
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
                f.company_id == company_id && f.city_id == home_city_id && f.facility_type == "mine"
            })
            .map(|f| f.id);

        if let Some(facility_id) = miner_info {
            let planet_id = state
                .cities
                .get(&home_city_id)
                .map(|c| c.body_id)
                .unwrap_or(0);
            let available_ores: Vec<_> = state
                .deposits
                .values()
                .filter(|d| d.body_id == planet_id && d.size_remaining > 0)
                .map(|d| (d.resource_type_id, d.extraction_cost_per_unit))
                .collect();

            let mut best_ore_id = None;
            let mut best_margin = f64::NEG_INFINITY;
            let mut selected_ore_cost = 0.0;

            for &(res_id, cost) in &available_ores {
                let ema = state
                    .ema_prices
                    .get(&(home_city_id, res_id))
                    .copied()
                    .unwrap_or(cost * 1.5);
                let margin = ema - cost;
                if margin > best_margin {
                    best_margin = margin;
                    best_ore_id = Some(res_id);
                    selected_ore_cost = cost;
                }
            }

            let mut best_overall_margin = best_margin;
            if let Some(target_id) = best_ore_id {
                for &target_city_id in state.cities.keys() {
                    if target_city_id == home_city_id {
                        continue;
                    }
                    let transport = get_transport_info(state, home_city_id, target_city_id);
                    let dest_ema = state
                        .ema_prices
                        .get(&(target_city_id, target_id))
                        .copied()
                        .unwrap_or(selected_ore_cost * 1.5);
                    let ship_margin = dest_ema - selected_ore_cost - transport.cost_per_unit;
                    if ship_margin > best_overall_margin {
                        best_overall_margin = ship_margin;
                    }
                }
            }

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

            for &(res_id, cost) in &available_ores {
                let key = Inventory::key(company_id, home_city_id, res_id);
                let inv_opt = state.inventories.get(&key).cloned();
                if let Some(inv) = inv_opt {
                    #[allow(clippy::collapsible_if)]
                    if inv.quantity > 0 {
                        let has_local_refinery = state
                            .facilities
                            .values()
                            .any(|f| f.city_id == home_city_id && f.facility_type == "refinery");
                        let facility_capacity = state
                            .facilities
                            .get(&facility_id)
                            .map(|f| f.capacity)
                            .unwrap_or(10);
                        let mut best_target_city = None;
                        let mut best_target_profit = 0.0;
                        let mut best_target_info = None;
                        let local_price = state
                            .ema_prices
                            .get(&(home_city_id, res_id))
                            .copied()
                            .unwrap_or(cost * 1.1);

                        for &target_city_id in state.cities.keys() {
                            if target_city_id == home_city_id {
                                continue;
                            }
                            let transport = get_transport_info(state, home_city_id, target_city_id);
                            let dest_price = state
                                .ema_prices
                                .get(&(target_city_id, res_id))
                                .copied()
                                .unwrap_or(cost * 1.5);
                            let margin = dest_price - local_price - transport.cost_per_unit;
                            if margin > best_target_profit {
                                best_target_profit = margin;
                                best_target_city = Some(target_city_id);
                                best_target_info = Some(transport);
                            }
                        }

                        let should_ship = if best_target_city.is_some() {
                            best_target_profit > (cost * 0.05)
                                || (!has_local_refinery
                                    && inv.quantity >= (facility_capacity * 2) as i64)
                        } else {
                            false
                        };

                        if should_ship {
                            let target_city = best_target_city.unwrap();
                            let transport = best_target_info.unwrap();
                            let move_qty = inv.quantity;
                            let total_cost = transport.cost_per_unit * move_qty as f64;
                            let company_cash = state.companies.get(&company_id).unwrap().cash;

                            if company_cash >= total_cost {
                                state.companies.get_mut(&company_id).unwrap().cash -= total_cost;
                                if let Some(mut_inv) = state.inventories.get_mut(&key) {
                                    mut_inv.quantity -= move_qty;
                                }
                                let route_id = state.next_trade_route_id();
                                state.trade_routes.insert(
                                    route_id,
                                    TradeRoute {
                                        id: route_id,
                                        company_id,
                                        origin_city_id: home_city_id,
                                        dest_city_id: target_city,
                                        resource_type_id: res_id,
                                        quantity: move_qty,
                                        arrival_tick: current_tick + transport.ticks,
                                    },
                                );
                                debug!(
                                    company_id,
                                    move_qty,
                                    from = home_city_id,
                                    to = target_city,
                                    "Miner shipped ore"
                                );
                                continue;
                            }
                        }

                        let base_ask = cost * 1.15;
                        let market_price = last_prices
                            .get(&(home_city_id, res_id))
                            .copied()
                            .unwrap_or(base_ask * 1.5);
                        let ticks_since_trade = current_tick.saturating_sub(last_trade_tick);
                        let ask_price = if inv.quantity > (facility_capacity * 10) as i64
                            || ticks_since_trade > 100
                        {
                            cost * 0.95 // Liquidation!
                        } else if inv.quantity > (facility_capacity * 5) as i64
                            || ticks_since_trade > 50
                        {
                            cost * 1.01
                        } else if inv.quantity > (facility_capacity * 2) as i64
                            || ticks_since_trade > 20
                        {
                            base_ask.min(market_price * 0.85)
                        } else {
                            base_ask.max(market_price * 0.98)
                        };

                        orders_to_post.push(MarketOrder {
                            id: 0,
                            city_id: home_city_id,
                            company_id,
                            resource_type_id: res_id,
                            order_type: "sell".into(),
                            order_kind: "limit".into(),
                            price: ask_price,
                            quantity: inv.quantity,
                            created_tick: current_tick,
                        });
                    }
                }
            }

            let company_cash = state.companies.get(&company_id).unwrap().cash;
            let facility = state.facilities.get(&facility_id).unwrap();
            let expansion_cost = 500.0 * 1.2_f64.powi(facility.capacity);
            let expected_additional_profit_per_tick = best_overall_margin * 5.0;
            let roi_ticks = expansion_cost / expected_additional_profit_per_tick.max(0.01);

            if company_cash > expansion_cost * 2.5
                && best_overall_margin > (selected_ore_cost * 0.10)
                && roi_ticks < 50.0
            {
                let facility = state.facilities.get_mut(&facility_id).unwrap();
                facility.capacity += 5;
                facility.setup_ticks_remaining = 5;
                state.companies.get_mut(&company_id).unwrap().cash -= expansion_cost;
                debug!(
                    company_id,
                    new_capacity = facility.capacity,
                    cost = expansion_cost,
                    "Miner expanded facility (Smarter Decision)"
                );
            }
        }

        // ─── Refinery AI ──────────────────────────────────────────────────────
        let refineries: Vec<(i32, i32)> = state
            .facilities
            .values()
            .filter(|f| f.company_id == company_id && f.facility_type == "refinery")
            .map(|f| (f.id, f.city_id))
            .collect();

        for (facility_id, refinery_city_id) in refineries {
            let capacity = state.facilities.get(&facility_id).unwrap().capacity;
            let mut recipes_evaluated = Vec::new();
            let mut total_positive_margin = 0.0;
            let labor_margin = 1.5;

            for recipe in state
                .recipes
                .values()
                .filter(|r| r.facility_type == "refinery")
            {
                let mut cost_basis = 0.0;
                for input in &recipe.inputs {
                    let in_price = state
                        .ema_prices
                        .get(&(refinery_city_id, input.resource_type_id))
                        .copied()
                        .unwrap_or(2.5);
                    cost_basis += in_price * input.quantity as f64;
                }
                let out_price = state
                    .ema_prices
                    .get(&(refinery_city_id, recipe.output_resource_id))
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

            if total_positive_margin > 0.0 {
                let mut new_ratios = std::collections::HashMap::new();
                for (id, margin, _, _, _) in &recipes_evaluated {
                    new_ratios.insert(id.to_string(), margin / total_positive_margin);
                }

                let facility = state.facilities.get_mut(&facility_id).unwrap();
                let current_ratios = facility.production_ratios.clone().unwrap_or_default();
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
                    facility.setup_ticks_remaining = 3;
                    state.companies.get_mut(&company_id).unwrap().cash -= 200.0;
                    debug!(company_id, "Refinery switched production ratios");
                }

                for (_r_id, _margin, cost_basis, out_price, recipe) in recipes_evaluated {
                    let out_key =
                        Inventory::key(company_id, refinery_city_id, recipe.output_resource_id);
                    let inv_opt = state.inventories.get(&out_key).cloned();
                    if let Some(inv) = inv_opt {
                        #[allow(clippy::collapsible_if)]
                        if inv.quantity > 0 {
                            let mut best_target_city = None;
                            let mut best_target_profit = 0.0;
                            let mut best_target_info = None;
                            let local_price = state
                                .ema_prices
                                .get(&(refinery_city_id, recipe.output_resource_id))
                                .copied()
                                .unwrap_or(out_price);

                            for &target_city_id in state.cities.keys() {
                                if target_city_id == refinery_city_id {
                                    continue;
                                }
                                let transport =
                                    get_transport_info(state, refinery_city_id, target_city_id);
                                let dest_price = state
                                    .ema_prices
                                    .get(&(target_city_id, recipe.output_resource_id))
                                    .copied()
                                    .unwrap_or(out_price * 1.2);
                                let margin = dest_price - local_price - transport.cost_per_unit;
                                if margin > best_target_profit {
                                    best_target_profit = margin;
                                    best_target_city = Some(target_city_id);
                                    best_target_info = Some(transport);
                                }
                            }

                            let mut shipped = false;
                            #[allow(clippy::collapsible_if)]
                            if let Some(target_city) = best_target_city {
                                if best_target_profit > (out_price * 0.10) {
                                    let transport = best_target_info.unwrap();
                                    let move_qty = inv.quantity;
                                    let total_cost = transport.cost_per_unit * move_qty as f64;
                                    let company_cash =
                                        state.companies.get(&company_id).unwrap().cash;

                                    if company_cash >= total_cost {
                                        state.companies.get_mut(&company_id).unwrap().cash -=
                                            total_cost;
                                        if let Some(mut_inv) = state.inventories.get_mut(&out_key) {
                                            mut_inv.quantity -= move_qty;
                                        }
                                        let route_id = state.next_trade_route_id();
                                        state.trade_routes.insert(
                                            route_id,
                                            TradeRoute {
                                                id: route_id,
                                                company_id,
                                                origin_city_id: refinery_city_id,
                                                dest_city_id: target_city,
                                                resource_type_id: recipe.output_resource_id,
                                                quantity: move_qty,
                                                arrival_tick: current_tick + transport.ticks,
                                            },
                                        );
                                        debug!(
                                            company_id,
                                            move_qty,
                                            from = refinery_city_id,
                                            to = target_city,
                                            "Refinery shipped refined goods"
                                        );
                                        shipped = true;
                                    }
                                }
                            }

                            if shipped {
                                continue;
                            }

                            let base_ask = cost_basis * 1.3;
                            let ask_price = if inv.quantity > (capacity * recipe.output_qty) as i64
                            {
                                cost_basis * 1.05
                            } else {
                                base_ask.min(out_price * 0.98)
                            };

                            orders_to_post.push(MarketOrder {
                                id: 0,
                                city_id: refinery_city_id,
                                company_id,
                                resource_type_id: recipe.output_resource_id,
                                order_type: "sell".into(),
                                order_kind: "limit".into(),
                                price: ask_price,
                                quantity: inv.quantity,
                                created_tick: current_tick,
                            });
                        }
                    }

                    let portion_cap = (capacity as f64
                        * (new_ratios.get(&recipe.id.to_string()).unwrap_or(&0.5)))
                        as i32;
                    if portion_cap > 0 {
                        for input in &recipe.inputs {
                            let buy_qty = (portion_cap * input.quantity * 5) as i64;
                            let in_price = state
                                .ema_prices
                                .get(&(refinery_city_id, input.resource_type_id))
                                .copied()
                                .unwrap_or(2.5);
                            let mut max_affordable = (out_price * recipe.output_qty as f64
                                - labor_margin)
                                / (recipe.inputs.iter().map(|i| i.quantity).sum::<i32>() as f64);
                            let in_key = Inventory::key(
                                company_id,
                                refinery_city_id,
                                input.resource_type_id,
                            );
                            let raw_inv_qty = state
                                .inventories
                                .get(&in_key)
                                .map(|i| i.quantity)
                                .unwrap_or(0);

                            let ticks_since_trade = current_tick.saturating_sub(last_trade_tick);
                            if raw_inv_qty == 0 && ticks_since_trade > 20 {
                                max_affordable *= 1.5;
                            }

                            let target_bid = if raw_inv_qty == 0 || ticks_since_trade > 100 {
                                in_price * 2.50
                            } else if ticks_since_trade > 50 {
                                in_price * 1.50
                            } else if ticks_since_trade > 20 {
                                in_price * 1.20
                            } else {
                                in_price * 0.98
                            };

                            let bid_price = target_bid.min(max_affordable);
                            if bid_price > 0.0 {
                                orders_to_post.push(MarketOrder {
                                    id: 0,
                                    city_id: refinery_city_id,
                                    company_id,
                                    resource_type_id: input.resource_type_id,
                                    order_type: "buy".into(),
                                    order_kind: "limit".into(),
                                    price: bid_price,
                                    quantity: buy_qty,
                                    created_tick: current_tick,
                                });
                            }
                        }
                    }
                }

                let company_cash = state.companies.get(&company_id).unwrap().cash;
                let facility = state.facilities.get(&facility_id).unwrap();
                let expansion_cost = 1500.0 * 1.3_f64.powi(facility.capacity / 5);
                let expected_additional_profit =
                    (total_positive_margin / facility.capacity as f64) * 5.0;
                let roi_ticks = expansion_cost / expected_additional_profit.max(0.01);

                if company_cash > expansion_cost * 3.0
                    && expected_additional_profit > 50.0
                    && roi_ticks < 60.0
                {
                    let facility = state.facilities.get_mut(&facility_id).unwrap();
                    facility.capacity += 5;
                    facility.setup_ticks_remaining = 8;
                    state.companies.get_mut(&company_id).unwrap().cash -= expansion_cost;
                    debug!(
                        company_id,
                        new_capacity = facility.capacity,
                        cost = expansion_cost,
                        "Refinery expanded facility (Smarter Decision)"
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
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
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
                status: "active".into(),
                last_trade_tick: 0,
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
        assert!(state.market_orders.values().any(|o| o.order_type == "sell"));
    }

    #[test]
    fn company_reschedules_next_eval() {
        let mut state = make_state_with_miner();
        run_decisions(&mut state, 1);
        let company = &state.companies[&1];
        assert!(company.next_eval_tick > 1);
    }

    #[test]
    fn company_skips_when_not_due() {
        let mut state = make_state_with_miner();
        state.companies.get_mut(&1).unwrap().next_eval_tick = 9999;
        run_decisions(&mut state, 1);
        assert!(state.market_orders.is_empty());
    }
}
