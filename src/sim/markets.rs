use std::collections::HashMap;

use tracing::{debug, info};

use crate::sim::state::{MarketHistory, MarketOrder, SimState};

/// Phase 4: Simple market clearing.
///
/// For each city, match all sell orders against buy orders. Orders are sorted
/// by price and matched greedily. The clearing price is the price at which the
/// last successful match occurred. Cash and inventory are transferred in-memory.
/// A `MarketHistory` record is appended to the delta buffer for each resource
/// that traded this tick.
///
/// # Examples
/// ```
/// use galactic_market::sim::state::SimState;
/// use galactic_market::sim::markets::clear_orders;
/// let mut state = SimState::new();
/// clear_orders(&mut state, 1);
/// ```
pub fn clear_orders(state: &mut SimState, current_tick: u64) {
    // Group orders by (city_id, resource_type_id)
    let mut orders_by_market: HashMap<(i32, i32), Vec<MarketOrder>> = HashMap::new();
    for order in state.market_orders.values() {
        orders_by_market
            .entry((order.city_id, order.resource_type_id))
            .or_default()
            .push(order.clone());
    }

    if !orders_by_market.is_empty() {
        let markets: Vec<_> = orders_by_market.keys().take(10).collect();
        debug!(count = orders_by_market.len(), sample = ?markets, "Order book markets");
    }

    // Clear each market
    for ((city_id, resource_type_id), orders) in orders_by_market {
        let mut sells: Vec<MarketOrder> = orders
            .iter()
            .filter(|o| o.order_type == "sell")
            .cloned()
            .collect();
        let mut buys: Vec<MarketOrder> = orders
            .iter()
            .filter(|o| o.order_type == "buy")
            .cloned()
            .collect();

        // Sort: cheapest sells first, highest bids first
        sells.sort_by(|a, b| a.price.partial_cmp(&b.price).unwrap());
        buys.sort_by(|a, b| b.price.partial_cmp(&a.price).unwrap());

        let mut prices_this_tick: Vec<f64> = Vec::new();
        let mut volume: i64 = 0;

        let mut buy_idx = 0;
        let mut sell_idx = 0;

        if !sells.is_empty() && !buys.is_empty() {
             // Use tracing at DEBUG level for specific market states
             debug!(
                city_id, resource_type_id,
                best_ask = sells[0].price,
                ask_qty = sells[0].quantity,
                best_bid = buys[0].price,
                bid_qty = buys[0].quantity,
                "Market status check"
            );
        }

        while sell_idx < sells.len() && buy_idx < buys.len() {
            let sell_id = sells[sell_idx].id;
            let buy_id = buys[buy_idx].id;
            
            let sell_company_id = sells[sell_idx].company_id;
            let buy_company_id = buys[buy_idx].company_id;
            
            let sell_city_id = sells[sell_idx].city_id;
            let buy_city_id = buys[buy_idx].city_id;

            let sell_price = sells[sell_idx].price;
            let buy_price = buys[buy_idx].price;

            let sell_quantity = sells[sell_idx].quantity;
            let buy_quantity = buys[buy_idx].quantity;

            // No match if best bid is below best ask
            if buy_price < sell_price {
                break;
            }

            // Midpoint clearing price
            let clearing_price = (buy_price + sell_price) / 2.0;

            // Verify buyer actually has cash
            let actual_buyer_cash = state.companies.get(&buy_company_id).map(|c| c.cash).unwrap_or(0.0);
            let affordable_qty = if clearing_price > 0.0 { (actual_buyer_cash / clearing_price).floor() as i64 } else { i64::MAX };
            
            // Verify seller actually has inventory
            let sell_key = crate::sim::state::Inventory::key(sell_company_id, sell_city_id, resource_type_id);
            let actual_seller_inventory = state.inventories.get(&sell_key).map(|i| i.quantity).unwrap_or(0);

            let qty = sell_quantity.min(buy_quantity).min(affordable_qty).min(actual_seller_inventory);

            if qty <= 0 {
                // Determine fault and void the order
                if actual_seller_inventory <= 0 {
                    sells[sell_idx].quantity = 0;
                }
                if affordable_qty <= 0 {
                    buys[buy_idx].quantity = 0;
                }
                
                if sells[sell_idx].quantity <= 0 {
                    if let Some(global_sell) = state.market_orders.get_mut(&sell_id) { global_sell.quantity = 0; }
                    sell_idx += 1;
                }
                if buys[buy_idx].quantity <= 0 {
                    if let Some(global_buy) = state.market_orders.get_mut(&buy_id) { global_buy.quantity = 0; }
                    buy_idx += 1;
                }
                
                // Infinite loop breakout fallback
                if qty <= 0 && sells[sell_idx].quantity > 0 && buys[buy_idx].quantity > 0 {
                    sell_idx += 1;
                    buy_idx += 1;
                }
                continue;
            }

            let cash_transferred = clearing_price * qty as f64;

            // Transfer cash: buyer pays, seller receives
            if let Some(buyer) = state.companies.get_mut(&buy_company_id) {
                buyer.cash -= cash_transferred;
            }
            if let Some(seller) = state.companies.get_mut(&sell_company_id) {
                seller.cash += cash_transferred;
            }

            // Transfer inventory: seller loses, buyer gains
            let sell_key =
                crate::sim::state::Inventory::key(sell_company_id, sell_city_id, resource_type_id);
            if let Some(inv) = state.inventories.get_mut(&sell_key) {
                inv.quantity -= qty;
            }

            let buy_key =
                crate::sim::state::Inventory::key(buy_company_id, buy_city_id, resource_type_id);
            let buy_inv =
                state
                    .inventories
                    .entry(buy_key)
                    .or_insert(crate::sim::state::Inventory {
                        company_id: buy_company_id,
                        city_id: buy_city_id,
                        resource_type_id,
                        quantity: 0,
                    });
            buy_inv.quantity += qty;

            prices_this_tick.push(clearing_price);
            volume += qty;

            info!(
                city_id,
                resource_type_id, qty, clearing_price, "Trade matched"
            );

            // Advance exhausted orders
            sells[sell_idx].quantity -= qty;
            buys[buy_idx].quantity -= qty;

            if let Some(global_sell) = state.market_orders.get_mut(&sell_id) {
                global_sell.quantity -= qty;
            }
            if let Some(global_buy) = state.market_orders.get_mut(&buy_id) {
                global_buy.quantity -= qty;
            }

            if sells[sell_idx].quantity <= 0 {
                sell_idx += 1;
            }
            if buys[buy_idx].quantity <= 0 {
                buy_idx += 1;
            }
        }

        // Record market history if any trades occurred
        if !prices_this_tick.is_empty() {
            let open = *prices_this_tick.first().unwrap();
            let close = *prices_this_tick.last().unwrap();
            let high = prices_this_tick
                .iter()
                .cloned()
                .fold(f64::NEG_INFINITY, f64::max);
            let low = prices_this_tick
                .iter()
                .cloned()
                .fold(f64::INFINITY, f64::min);

            state.market_history_buffer.push(MarketHistory {
                city_id,
                resource_type_id,
                tick: current_tick,
                open,
                high,
                low,
                close,
                volume,
            });

            // Update persistent price cache
            state.price_cache.insert((city_id, resource_type_id), close);

            // Update EMA price cache for smoothed AI evaluation
            let alpha = 0.1;
            let current_ema = state
                .ema_prices
                .get(&(city_id, resource_type_id))
                .copied()
                .unwrap_or(close);
            let next_ema = alpha * close + (1.0 - alpha) * current_ema;
            state
                .ema_prices
                .insert((city_id, resource_type_id), next_ema);
        }
    }

    // Expire empty orders or very old orders (e.g. older than 100 ticks)
    state.market_orders.retain(|_, order| order.quantity > 0 && order.created_tick + 100 > current_tick);
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{Company, Inventory, MarketOrder, SimState};

    fn make_company(id: i32, cash: f64) -> Company {
        Company {
            id,
            name: format!("Company {}", id),
            company_type: "freelancer".into(),
            home_city_id: 1,
            cash,
            debt: 0.0,
            next_eval_tick: 1,
        }
    }

    #[test]
    fn clearing_transfers_cash_and_inventory() {
        let mut state = SimState::new();
        state.companies.insert(1, make_company(1, 0.0)); // seller
        state.companies.insert(2, make_company(2, 100.0)); // buyer

        // Seller has 10 ore
        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );

        // Sell 5 @ 8.0, Buy 5 @ 10.0 — should match at clearing price 9.0
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                city_id: 1,
                company_id: 1,
                resource_type_id: 1,
                order_type: "sell".into(),
                price: 8.0,
                quantity: 5,
                created_tick: 0,
            },
        );
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: 2,
                resource_type_id: 1,
                order_type: "buy".into(),
                price: 10.0,
                quantity: 5,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        let clearing = 9.0; // midpoint of 8 and 10
        assert!((state.companies[&1].cash - clearing * 5.0).abs() < 0.01); // seller received
        assert!((state.companies[&2].cash - (100.0 - clearing * 5.0)).abs() < 0.01); // buyer paid
        assert_eq!(state.inventories[&Inventory::key(1, 1, 1)].quantity, 5); // seller's ore
        assert_eq!(state.inventories[&Inventory::key(2, 1, 1)].quantity, 5); // buyer's ore
    }

    #[test]
    fn clearing_records_market_history() {
        let mut state = SimState::new();
        state.companies.insert(1, make_company(1, 0.0));
        state.companies.insert(2, make_company(2, 1000.0));
        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                city_id: 1,
                company_id: 1,
                resource_type_id: 1,
                order_type: "sell".into(),
                price: 5.0,
                quantity: 10,
                created_tick: 0,
            },
        );
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: 2,
                resource_type_id: 1,
                order_type: "buy".into(),
                price: 5.0,
                quantity: 10,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 42);

        assert!(!state.market_history_buffer.is_empty());
        let hist = &state.market_history_buffer[0];
        assert_eq!(hist.tick, 42);
        assert_eq!(hist.volume, 10);
    }

    #[test]
    fn no_match_when_bid_below_ask() {
        let mut state = SimState::new();
        state.companies.insert(1, make_company(1, 0.0));
        state.companies.insert(2, make_company(2, 100.0));
        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );
        state.market_orders.insert(
            1,
            MarketOrder {
                id: 1,
                city_id: 1,
                company_id: 1,
                resource_type_id: 1,
                order_type: "sell".into(),
                price: 10.0,
                quantity: 5,
                created_tick: 0,
            },
        );
        state.market_orders.insert(
            2,
            MarketOrder {
                id: 2,
                city_id: 1,
                company_id: 2,
                resource_type_id: 1,
                order_type: "buy".into(),
                price: 5.0,
                quantity: 5,
                created_tick: 0,
            },
        );

        clear_orders(&mut state, 1);

        // No trade: cash and inventory unchanged
        assert_eq!(state.companies[&2].cash, 100.0);
        assert!(state.market_history_buffer.is_empty());
    }
}
