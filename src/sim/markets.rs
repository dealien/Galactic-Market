//! Market order matching and price discovery mechanisms.
//!
//! Handles clearing limit and market orders across all cities, manages port fee
//! collections, updates moving price averages (EMA), and tracks OHLCV price histories.

use std::collections::HashMap;

use tracing::{debug, warn};

use crate::sim::state::{Inventory, MarketHistory, SimState};

/// Phase 7: Sophisticated market clearing.
///
/// For each city and resource, match buy and sell orders.
/// Supports:
/// - **Market Orders:** Execute immediately at the best available price.
/// - **Limit Orders:** Execute only at or better than the specified price.
/// - **Priority:** Market orders clear first, then Limit orders (sorted by price).
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::markets::clear_orders;
///
/// let mut state = SimState::new();
/// clear_orders(&mut state, 1);
/// ```
pub fn clear_orders(state: &mut SimState, current_tick: u64) {
    let mut orders_by_market: HashMap<(i32, i32), Vec<i32>> = HashMap::new();

    for (&id, order) in &state.market_orders {
        orders_by_market
            .entry((order.city_id, order.resource_type_id))
            .or_default()
            .push(id);
    }

    for ((city_id, resource_type_id), order_ids) in orders_by_market {
        let mut buys = Vec::with_capacity(order_ids.len());
        let mut sells = Vec::with_capacity(order_ids.len());

        for id in order_ids {
            let order = &state.market_orders[&id];
            let is_market = order.order_kind == "market";
            if order.order_type == "buy" {
                buys.push((id, is_market, order.price, order.created_tick));
            } else {
                sells.push((id, is_market, order.price, order.created_tick));
            }
        }

        // Sort orders:
        // Market orders first, then Limit orders.
        // Buys: Market -> Highest Limit Price
        // Sells: Market -> Lowest Limit Price
        buys.sort_unstable_by(
            |&(a_id, a_is_market, a_price, a_tick), &(b_id, b_is_market, b_price, b_tick)| {
                if a_is_market != b_is_market {
                    if a_is_market {
                        return std::cmp::Ordering::Less;
                    }
                    return std::cmp::Ordering::Greater;
                }
                b_price
                    .partial_cmp(&a_price)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a_tick.cmp(&b_tick))
                    .then_with(|| a_id.cmp(&b_id))
            },
        );

        sells.sort_unstable_by(
            |&(a_id, a_is_market, a_price, a_tick), &(b_id, b_is_market, b_price, b_tick)| {
                if a_is_market != b_is_market {
                    if a_is_market {
                        return std::cmp::Ordering::Less;
                    }
                    return std::cmp::Ordering::Greater;
                }
                a_price
                    .partial_cmp(&b_price)
                    .unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a_tick.cmp(&b_tick))
                    .then_with(|| a_id.cmp(&b_id))
            },
        );

        let mut b_idx = 0;
        let mut s_idx = 0;

        let mut total_volume = 0;
        let mut sum_prices = 0.0;
        let mut high = f64::MIN;
        let mut low = f64::MAX;
        let mut open = None;
        let mut close = 0.0;

        while b_idx < buys.len() && s_idx < sells.len() {
            let (b_id, _, _, _) = buys[b_idx];
            let (s_id, _, _, _) = sells[s_idx];

<<<<<<< HEAD
            let (buy_qty, buy_price, buy_is_market, buy_company_id) = {
                let o = &state.market_orders[&b_id];
                (o.quantity, o.price, o.order_kind == "market", o.company_id)
            };
            let (sell_qty, sell_price, sell_is_market, sell_company_id) = {
                let o = &state.market_orders[&s_id];
                (o.quantity, o.price, o.order_kind == "market", o.company_id)
            };

            // Check price compatibility for Limit vs Limit
            if !buy_is_market && !sell_is_market && buy_price < sell_price {
=======
            let (buy_qty, buy_price, buy_is_limit, buy_company_id) = {
                let o = &state.market_orders[&b_id];
                (o.quantity, o.price, o.order_kind == "limit", o.company_id)
            };
            let (sell_qty, sell_price, sell_is_limit, sell_company_id) = {
                let o = &state.market_orders[&s_id];
                (o.quantity, o.price, o.order_kind == "limit", o.company_id)
            };

            // Check price compatibility for Limit vs Limit
            if buy_is_limit && sell_is_limit && buy_price < sell_price {
>>>>>>> origin/main
                break; // No more matches possible
            }

            // Determine clearing price
<<<<<<< HEAD
            let clearing_price = match (buy_is_market, sell_is_market) {
                (true, true) => {
=======
            let clearing_price = match (buy_is_limit, sell_is_limit) {
                (false, false) => {
>>>>>>> origin/main
                    // Two market orders: use last known EMA or fallback
                    state
                        .ema_prices
                        .get(&(city_id, resource_type_id))
                        .copied()
                        .unwrap_or(10.0)
                }
<<<<<<< HEAD
                (true, false) => sell_price,
                (false, true) => buy_price,
=======
                (false, true) => sell_price,
                (true, false) => buy_price,
>>>>>>> origin/main
                _ => (buy_price + sell_price) / 2.0, // Midpoint discovery for Limit-Limit
            };

            let actual_buyer_cash = if buy_company_id < 0 {
                // Sentinels (e.g. Empire Relief) are fully paid up front.
                // We treat their available cash as sufficient for the order.
                buy_qty as f64 * clearing_price
            } else {
                state
                    .companies
                    .get(&buy_company_id)
                    .map(|c| c.cash)
                    .unwrap_or(0.0)
            };
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

                // Issue #9: Calculate port fee on settlement
                let city = state.cities.get(&city_id);
                let port_fee = city
                    .map(|c| c.port_fee_per_unit * qty as f64)
                    .unwrap_or(0.0);

                // Transfer cash (seller receives cash, minus port fee)
                if let Some(seller) = state.companies.get_mut(&sell_company_id) {
                    seller.cash += cash_transferred - port_fee;
                    seller.last_trade_tick = current_tick;
                }
                if let Some(buyer) = state.companies.get_mut(&buy_company_id) {
                    buyer.cash -= cash_transferred;
                    buyer.last_trade_tick = current_tick;
                }

                // Issue #9: Transfer port fee to city tax pool
                if port_fee > 0.0 {
                    state.add_city_tax(city_id, port_fee);
                }

                // Transfer inventory
                if let Some(seller_inv) = state.inventories.get_mut(&seller_inv_key) {
                    seller_inv.quantity -= qty;
                }

                let target_buyer_company_id = if buy_company_id < 0 {
                    // Sentinels (e.g. Empire Relief) deposit the purchased goods directly
                    // into the city's consumer company inventory to feed the population.
                    state
                        .city_consumer_ids
                        .get(&city_id)
                        .copied()
                        .unwrap_or(buy_company_id)
                } else {
                    buy_company_id
                };

                let buyer_inv = state
                    .inventories
                    .entry(Inventory::key(
                        target_buyer_company_id,
                        city_id,
                        resource_type_id,
                    ))
                    .or_insert(Inventory {
                        company_id: target_buyer_company_id,
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
                    port_fee = port_fee,
                    "Match: {} bought from {} (port fee: {})",
                    buy_company_id,
                    sell_company_id,
                    port_fee
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
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
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

        // Port fee: 10 * 0.1 = 1.0, so seller gets 50 - 1 = 49
        // Buyer pays: 10 * 5.0 = 50
        assert_eq!(state.companies[&1].cash, 1049.0); // 1000 + 50 - 1 (port fee)
        assert_eq!(state.companies[&2].cash, 950.0); // 1000 - 50
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
        // Port fee: 10 * 0.1 = 1.0, so seller gets 50 - 1 = 49
        assert_eq!(state.companies[&1].cash, 1049.0); // 1000 + 50 - 1 (port fee)
        assert_eq!(state.companies[&2].cash, 950.0); // 1000 - 50
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
        // Port fee: 10 * 0.1 = 1.0, so seller gets 250 - 1 = 249
        assert_eq!(state.companies[&1].cash, 1249.0); // 1000 + 250 - 1 (port fee)
        assert_eq!(state.companies[&2].cash, 750.0); // 1000 - 250
    }

    #[test]
    fn test_clear_orders_negative_company_id_does_not_panic() {
        let mut state = setup_test_state();

        // Seller: limit sell 10 at 5.0
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

        // Buyer: Empire Relief (company_id -1), limit buy 10 at 5.0
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: -1, // Negative ID
                resource_type_id: 1,
                order_type: "buy".into(),
                order_kind: "limit".into(),
                price: 5.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        // Run clear_orders, this should not panic!
        clear_orders(&mut state, 1);

        // Seller should be paid
        assert_eq!(state.companies[&1].cash, 1049.0); // 1000 + 50 - 1 (port fee)
        // Buyer is negative ID, not in companies, so they are ignored in cash subtraction
    }

    #[test]
    fn test_void_order_lack_of_cash() {
        let mut state = setup_test_state();

        // Give company 2 zero cash so they can't afford the purchase
        state.companies.get_mut(&2).unwrap().cash = 0.0;

        // Seller: Limit Sell 10 @ 5.0
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                order_type: "sell".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 5.0,
                created_tick: 0,
            },
        );

        // Buyer: Limit Buy 10 @ 5.0
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                company_id: 2,
                city_id: 1,
                resource_type_id: 1,
                order_type: "buy".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 5.0,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        // The buy order should have been voided, so there should only be 1 order left (the sell order)
        assert_eq!(state.market_orders.len(), 1);

        // Ensure the remaining order is the sell order
        assert!(state.market_orders.contains_key(&1));

        // Seller's cash should remain unchanged since trade was voided
        assert_eq!(state.companies[&1].cash, 1000.0);

        // Buyer's cash should remain at 0.0
        assert_eq!(state.companies[&2].cash, 0.0);

        // Inventory of seller should remain unchanged
        assert_eq!(
            state
                .inventories
                .get(&Inventory::key(1, 1, 1))
                .unwrap()
                .quantity,
            100
        );
    }

    #[test]
    fn test_price_drift_up_on_high_demand() {
        let mut state = setup_test_state();
        let city_id = 1;
        let resource_type_id = 1;

        // Set initial EMA
        state.ema_prices.insert((city_id, resource_type_id), 20.0);

        // Insert a buy order for city 1, resource 1
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                company_id: 1,
                city_id,
                resource_type_id,
                order_type: "buy".to_string(),
                order_kind: "market".to_string(),
                quantity: 10,
                price: 25.0,
                created_tick: 0,
            },
        );

        // Run clear_orders
        clear_orders(&mut state, 1);

        // Price should drift up
        let ema = state
            .ema_prices
            .get(&(city_id, resource_type_id))
            .copied()
            .unwrap_or(20.0);
        assert!(
            ema > 20.0,
            "EMA price should drift up from 20.0 on high demand, got {}",
            ema
        );
    }

    #[test]
    fn test_price_drift_down_on_high_supply() {
        let mut state = setup_test_state();
        let city_id = 1;
        let resource_type_id = 1;

        // Set initial EMA
        state.ema_prices.insert((city_id, resource_type_id), 20.0);

        // Insert a sell order for city 1, resource 1
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                company_id: 1,
                city_id,
                resource_type_id,
                order_type: "sell".to_string(),
                order_kind: "market".to_string(),
                quantity: 10,
                price: 15.0,
                created_tick: 0,
            },
        );

        // Run clear_orders
        clear_orders(&mut state, 1);

        // Price should drift down
        let ema = state
            .ema_prices
            .get(&(city_id, resource_type_id))
            .copied()
            .unwrap_or(20.0);
        assert!(
            ema < 20.0,
            "EMA price should drift down from 20.0 on high supply, got {}",
            ema
        );
    }

    #[test]
    fn test_buy_order_sorting_highest_price_first() {
        let mut state = setup_test_state();

        // We need 4 companies total
        state.companies.insert(3, make_company(3, 1000.0));
        state.companies.insert(4, make_company(4, 1000.0));

        // Seller: Limit Sell 10 @ 2.0
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                order_type: "sell".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 2.0,
                created_tick: 0,
            },
        );

        // Buyer 1: Limit Buy 10 @ 3.0
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                company_id: 2,
                city_id: 1,
                resource_type_id: 1,
                order_type: "buy".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 3.0,
                created_tick: 0,
            },
        );

        // Buyer 2: Limit Buy 10 @ 5.0 (Should beat Buyer 1)
        state.market_orders.insert(
            3,
            MarketOrder {
                id: 3,
                company_id: 3,
                city_id: 1,
                resource_type_id: 1,
                order_type: "buy".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 5.0,
                created_tick: 0,
            },
        );

        // Buyer 3: Market Buy 10 (Should beat Buyer 2 and Buyer 1)
        state.market_orders.insert(
            4,
            MarketOrder {
                id: 4,
                company_id: 4,
                city_id: 1,
                resource_type_id: 1,
                order_type: "buy".to_string(),
                order_kind: "market".to_string(),
                quantity: 10,
                price: 0.0,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        /* Seller has only 10 items to sell.
         * It should go to Market Buy (Company 4) because market orders sort first.
         * Company 4 bought 10 at price 2.0 (since seller limit is 2.0 and market order takes seller limit). */
        assert_eq!(state.companies[&4].cash, 980.0); // 1000 - 20 (price)

        // Let's add more seller inventory and another clear to test the remaining limits
        state.market_orders.insert(
            5,
            MarketOrder {
                id: 5,
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                order_type: "sell".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 2.0,
                created_tick: 0,
            },
        );
        clear_orders(&mut state, 1);

        /* Now, Company 3 (Limit Buy @ 5.0) should beat Company 2 (Limit Buy @ 3.0)
         * Midpoint clearing price between limit 5.0 and limit 2.0 is 3.5.
         * Buy pays 3.5 * 10 = 35.0 */
        assert_eq!(state.companies[&3].cash, 965.0); // 1000 - 35

        // Company 2 should have bought nothing
        assert_eq!(state.companies[&2].cash, 1000.0);
    }

    #[test]
    fn test_sell_order_sorting_lowest_price_first() {
        let mut state = setup_test_state();

        // Need 4 companies
        state.companies.insert(3, make_company(3, 1000.0));
        state.companies.insert(4, make_company(4, 1000.0));

        // Add 10 items to companies 2, 3, and 4
        state.inventories.insert(
            Inventory::key(2, 1, 1),
            Inventory {
                company_id: 2,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );
        state.inventories.insert(
            Inventory::key(3, 1, 1),
            Inventory {
                company_id: 3,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );
        state.inventories.insert(
            Inventory::key(4, 1, 1),
            Inventory {
                company_id: 4,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );

        // Buyer: Limit Buy 10 @ 6.0
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                order_type: "buy".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 6.0,
                created_tick: 0,
            },
        );

        // Seller 1: Limit Sell 10 @ 5.0
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                company_id: 2,
                city_id: 1,
                resource_type_id: 1,
                order_type: "sell".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 5.0,
                created_tick: 0,
            },
        );

        // Seller 2: Limit Sell 10 @ 4.0 (Should beat Seller 1)
        state.market_orders.insert(
            3,
            MarketOrder {
                id: 3,
                company_id: 3,
                city_id: 1,
                resource_type_id: 1,
                order_type: "sell".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 4.0,
                created_tick: 0,
            },
        );

        // Seller 3: Market Sell 10 (Should beat Seller 2 and Seller 1)
        state.market_orders.insert(
            4,
            MarketOrder {
                id: 4,
                company_id: 4,
                city_id: 1,
                resource_type_id: 1,
                order_type: "sell".to_string(),
                order_kind: "market".to_string(),
                quantity: 10,
                price: 0.0,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        /* Buyer only buys 10 items total.
         * The Market Sell order (Company 4) should get precedence.
         * Buy is limit 6.0, market sells take the buyer's limit price.
         * Port fee is 10 * 0.1 = 1.0. Company 4 cash = 1000 + 60 - 1 = 1059 */
        assert_eq!(state.companies[&4].cash, 1059.0);

        // Now let's add another buyer to clear the remaining limit orders
        state.market_orders.insert(
            5,
            MarketOrder {
                id: 5,
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                order_type: "buy".to_string(),
                order_kind: "limit".to_string(),
                quantity: 10,
                price: 6.0,
                created_tick: 0,
            },
        );
        clear_orders(&mut state, 1);

        /* Seller 2 (Company 3, Limit Sell @ 4.0) should beat Seller 1 (Company 2, Limit Sell @ 5.0)
         * Midpoint clearing price between limit 6.0 and limit 4.0 is 5.0.
         * Sell gets 10 * 5.0 = 50 - 1 (fee) = 49.0 */
        assert_eq!(state.companies[&3].cash, 1049.0);

        // Seller 1 (Company 2) shouldn't have sold anything yet
        assert_eq!(state.companies[&2].cash, 1000.0);
    }
}
