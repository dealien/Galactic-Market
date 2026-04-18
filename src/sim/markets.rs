use std::collections::HashMap;

use tracing::{debug, warn};

use crate::sim::state::{Inventory, MarketHistory, SimState};

/// Phase 4: Sophisticated market clearing.
///
/// For each city and resource, match buy and sell orders.
/// Supports:
/// - **Market Orders:** Execute immediately at the best available price.
/// - **Limit Orders:** Execute only at or better than the specified price.
/// - **Priority:** Market orders clear first, then Limit orders (sorted by price).
pub fn clear_orders(state: &mut SimState, current_tick: u64) {
    let mut orders_by_market: HashMap<(i32, i32), Vec<i32>> = HashMap::new();

    for (&id, order) in &state.market_orders {
        orders_by_market
            .entry((order.city_id, order.resource_type_id))
            .or_default()
            .push(id);
    }

    for ((city_id, resource_type_id), order_ids) in orders_by_market {
        let mut buys = Vec::new();
        let mut sells = Vec::new();

        for id in order_ids {
            let order = &state.market_orders[&id];
            if order.order_type == "buy" {
                buys.push(id);
            } else {
                sells.push(id);
            }
        }

        // Sort orders:
        // Market orders first, then Limit orders.
        // Buys: Market -> Highest Limit Price
        // Sells: Market -> Lowest Limit Price
        buys.sort_by(|&a, &b| {
            let oa = &state.market_orders[&a];
            let ob = &state.market_orders[&b];
            if oa.order_kind != ob.order_kind {
                if oa.order_kind == "market" {
                    return std::cmp::Ordering::Less;
                }
                return std::cmp::Ordering::Greater;
            }
            ob.price
                .partial_cmp(&oa.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        sells.sort_by(|&a, &b| {
            let oa = &state.market_orders[&a];
            let ob = &state.market_orders[&b];
            if oa.order_kind != ob.order_kind {
                if oa.order_kind == "market" {
                    return std::cmp::Ordering::Less;
                }
                return std::cmp::Ordering::Greater;
            }
            oa.price
                .partial_cmp(&ob.price)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut b_idx = 0;
        let mut s_idx = 0;

        let mut total_volume = 0;
        let mut sum_prices = 0.0;
        let mut high = f64::MIN;
        let mut low = f64::MAX;
        let mut open = None;
        let mut close = 0.0;

        while b_idx < buys.len() && s_idx < sells.len() {
            let b_id = buys[b_idx];
            let s_id = sells[s_idx];

            let (buy_qty, buy_price, buy_kind, buy_company_id) = {
                let o = &state.market_orders[&b_id];
                (o.quantity, o.price, o.order_kind.clone(), o.company_id)
            };
            let (sell_qty, sell_price, sell_kind, sell_company_id) = {
                let o = &state.market_orders[&s_id];
                (o.quantity, o.price, o.order_kind.clone(), o.company_id)
            };

            // Check price compatibility for Limit vs Limit
            if buy_kind == "limit" && sell_kind == "limit" && buy_price < sell_price {
                break; // No more matches possible
            }

            // Determine clearing price
            let clearing_price = match (buy_kind.as_str(), sell_kind.as_str()) {
                ("market", "market") => {
                    // Two market orders: use last known EMA or fallback
                    state
                        .ema_prices
                        .get(&(city_id, resource_type_id))
                        .copied()
                        .unwrap_or(10.0)
                }
                ("market", "limit") => sell_price,
                ("limit", "market") => buy_price,
                _ => (buy_price + sell_price) / 2.0, // Midpoint discovery for Limit-Limit
            };

            let actual_buyer_cash = state.companies[&buy_company_id].cash;
            let affordable_by_buyer = if clearing_price > 0.0 {
                (actual_buyer_cash / clearing_price) as i64
            } else {
                buy_qty // Free items!
            };

            // Invariant: Orders in this loop belong to `city_id`.
            let seller_inv_key = Inventory::key(sell_company_id, city_id, resource_type_id);
            let actual_seller_inventory = state
                .inventories
                .get(&seller_inv_key)
                .map(|inv| inv.quantity)
                .unwrap_or(0);

            let qty = buy_qty
                .min(sell_qty)
                .min(affordable_by_buyer)
                .min(actual_seller_inventory);

            if qty > 0 {
                let cash_transferred = qty as f64 * clearing_price;

                // Transfer cash
                if let Some(buyer) = state.companies.get_mut(&buy_company_id) {
                    buyer.cash -= cash_transferred;
                    buyer.last_trade_tick = current_tick;
                }
                if let Some(seller) = state.companies.get_mut(&sell_company_id) {
                    seller.cash += cash_transferred;
                    seller.last_trade_tick = current_tick;
                }

                // Transfer inventory
                if let Some(seller_inv) = state.inventories.get_mut(&seller_inv_key) {
                    seller_inv.quantity -= qty;
                }

                let buyer_inv = state
                    .inventories
                    .entry(Inventory::key(buy_company_id, city_id, resource_type_id))
                    .or_insert(Inventory {
                        company_id: buy_company_id,
                        city_id,
                        resource_type_id,
                        quantity: 0,
                    });
                buyer_inv.quantity += qty;

                // Update remaining order quantities
                state.market_orders.get_mut(&b_id).unwrap().quantity -= qty;
                state.market_orders.get_mut(&s_id).unwrap().quantity -= qty;

                // Statistics
                total_volume += qty;
                sum_prices += clearing_price * qty as f64;
                if open.is_none() {
                    open = Some(clearing_price);
                }
                close = clearing_price;
                if clearing_price > high {
                    high = clearing_price;
                }
                if clearing_price < low {
                    low = clearing_price;
                }

                debug!(
                    city_id,
                    res_id = resource_type_id,
                    qty,
                    price = clearing_price,
                    "Match: {} bought from {}",
                    buy_company_id,
                    sell_company_id
                );
            } else {
                // Determine fault and void order. Each arm `continue`s so the
                // "fully filled" pointer-advance block below is only reached on
                // the successful trade path (qty > 0).
                if affordable_by_buyer == 0 && actual_buyer_cash < clearing_price {
                    debug!(buy_company_id, "Voiding buy order due to lack of cash");
                    state.market_orders.remove(&b_id);
                    b_idx += 1;
                    continue;
                } else if actual_seller_inventory == 0 {
                    debug!(
                        sell_company_id,
                        "Voiding sell order due to lack of inventory"
                    );
                    state.market_orders.remove(&s_id);
                    s_idx += 1;
                    continue;
                } else {
                    // Logic safety catch: skip this buy order if it's stuck
                    warn!(
                        city_id,
                        res_id = resource_type_id,
                        "Zero quantity match catch-all; skipping buyer"
                    );
                    b_idx += 1;
                    continue;
                }
            }

            // Advance pointers if orders fully filled after a successful trade.
            // Only reached when qty > 0 (the void branches above all `continue`).
            if state
                .market_orders
                .get(&b_id)
                .map(|o| o.quantity)
                .unwrap_or(0)
                == 0
            {
                state.market_orders.remove(&b_id);
                b_idx += 1;
            }
            if state
                .market_orders
                .get(&s_id)
                .map(|o| o.quantity)
                .unwrap_or(0)
                == 0
            {
                state.market_orders.remove(&s_id);
                s_idx += 1;
            }
        }

        // Record history if trades occurred
        if total_volume > 0 {
            let avg = sum_prices / total_volume as f64;
            state.market_history_buffer.push(MarketHistory {
                city_id,
                resource_type_id,
                tick: current_tick,
                open: open.unwrap_or(avg),
                high,
                low,
                close,
                volume: total_volume,
            });

            state.price_cache.insert((city_id, resource_type_id), close);

            // EMA alpha 0.2 chosen for Stage 3 to allow faster convergence
            // in a geography-distributed economy where arbitrageurs are active.
            let alpha = 0.2;
            let current_ema = state
                .ema_prices
                .get(&(city_id, resource_type_id))
                .copied()
                .unwrap_or(close);
            let next_ema = alpha * close + (1.0 - alpha) * current_ema;
            state
                .ema_prices
                .insert((city_id, resource_type_id), next_ema);
        } else {
            // --- Price Discovery Drift (Stage 1.5 Patch) ---
            // If no trades occurred, drift the EMA based on unsatisfied sentiment.
            // This breaks deadlocks where prices are too far apart for merchants to bridge cities.
            let current_ema = state
                .ema_prices
                .get(&(city_id, resource_type_id))
                .copied()
                .unwrap_or(20.0);

            let has_buys = !buys.is_empty();
            let has_sells = !sells.is_empty();

            let drift_alpha = 0.01; // Slow drift
            if has_buys && !has_sells {
                // High demand, no supply -> price should go up
                state.ema_prices.insert(
                    (city_id, resource_type_id),
                    current_ema * (1.0 + drift_alpha),
                );
            } else if has_sells && !has_buys {
                // High supply, no demand -> price should go down
                state.ema_prices.insert(
                    (city_id, resource_type_id),
                    current_ema * (1.0 - drift_alpha),
                );
            }
        }
    }

    // Clean up empty orders
    state.market_orders.retain(|_, o| o.quantity > 0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, Inventory, MarketOrder, SimState};

    fn make_company(id: i32, cash: f64) -> Company {
        Company {
            id,
            name: format!("Company {}", id),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".into(),
            last_trade_tick: 0,
        }
    }

    fn setup_test_state() -> SimState {
        let mut state = SimState::new();
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
        state.companies.insert(1, make_company(1, 1000.0));
        state.companies.insert(2, make_company(2, 1000.0));
        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 100,
            },
        );
        state
    }

    #[test]
    fn market_order_matches_limit_order() {
        let mut state = setup_test_state();

        // Seller: Limit Sell 10 @ 5.0
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
                quantity: 10,
                created_tick: 0,
            },
        );

        // Buyer: Market Buy 10
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: 2,
                resource_type_id: 1,
                order_type: "buy".into(),
                order_kind: "market".into(),
                price: 0.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        assert_eq!(state.companies[&1].cash, 1050.0); // 1000 + (10 * 5.0)
        assert_eq!(state.companies[&2].cash, 950.0);
    }

    #[test]
    fn limit_order_midpoint_clearing() {
        let mut state = setup_test_state();

        // Seller: Limit Sell 10 @ 4.0
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                city_id: 1,
                company_id: 1,
                resource_type_id: 1,
                order_type: "sell".into(),
                order_kind: "limit".into(),
                price: 4.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        // Buyer: Limit Buy 10 @ 6.0
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: 2,
                resource_type_id: 1,
                order_type: "buy".into(),
                order_kind: "limit".into(),
                price: 6.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        // Price should be 5.0 (midpoint)
        assert_eq!(state.companies[&1].cash, 1050.0);
        assert_eq!(state.companies[&2].cash, 950.0);
    }

    #[test]
    fn market_order_to_market_order_uses_ema() {
        let mut state = setup_test_state();
        state.ema_prices.insert((1, 1), 25.0);

        // Seller: Market Sell 10
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                city_id: 1,
                company_id: 1,
                resource_type_id: 1,
                order_type: "sell".into(),
                order_kind: "market".into(),
                price: 0.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        // Buyer: Market Buy 10
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: 2,
                resource_type_id: 1,
                order_type: "buy".into(),
                order_kind: "market".into(),
                price: 0.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        // Uses EMA price of 25.0
        assert_eq!(state.companies[&1].cash, 1250.0);
        assert_eq!(state.companies[&2].cash, 750.0);
    }
}
