use std::collections::HashMap;

use tracing::debug;

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

        while sell_idx < sells.len() && buy_idx < buys.len() {
            let sell = &sells[sell_idx];
            let buy = &buys[buy_idx];

            // No match if best bid is below best ask
            if buy.price < sell.price {
                break;
            }

            // Midpoint clearing price
            let clearing_price = (buy.price + sell.price) / 2.0;
            let qty = sell.quantity.min(buy.quantity);

            let cash_transferred = clearing_price * qty as f64;

            // Transfer cash: buyer pays, seller receives
            if let Some(buyer) = state.companies.get_mut(&buy.company_id) {
                buyer.cash -= cash_transferred;
            }
            if let Some(seller) = state.companies.get_mut(&sell.company_id) {
                seller.cash += cash_transferred;
            }

            // Transfer inventory: seller loses, buyer gains
            let sell_key = crate::sim::state::Inventory::key(sell.company_id, sell.city_id, resource_type_id);
            if let Some(inv) = state.inventories.get_mut(&sell_key) {
                inv.quantity -= qty;
            }

            let buy_key = crate::sim::state::Inventory::key(buy.company_id, buy.city_id, resource_type_id);
            let buy_inv = state
                .inventories
                .entry(buy_key)
                .or_insert(crate::sim::state::Inventory {
                    company_id: buy.company_id,
                    city_id: buy.city_id,
                    resource_type_id,
                    quantity: 0,
                });
            buy_inv.quantity += qty;

            prices_this_tick.push(clearing_price);
            volume += qty;

            debug!(
                city_id,
                resource_type_id,
                qty,
                clearing_price,
                "Market trade executed"
            );

            // Advance exhausted orders
            sells[sell_idx].quantity -= qty;
            buys[buy_idx].quantity -= qty;
            if sells[sell_idx].quantity == 0 {
                sell_idx += 1;
            }
            if buys[buy_idx].quantity == 0 {
                buy_idx += 1;
            }
        }

        // Record market history if any trades occurred
        if !prices_this_tick.is_empty() {
            let open = *prices_this_tick.first().unwrap();
            let close = *prices_this_tick.last().unwrap();
            let high = prices_this_tick.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let low = prices_this_tick.iter().cloned().fold(f64::INFINITY, f64::min);

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
        }
    }

    // Clear all orders after matching (simple clearing — orders don't persist)
    state.market_orders.clear();
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
        state.companies.insert(1, make_company(1, 0.0));   // seller
        state.companies.insert(2, make_company(2, 100.0));  // buyer

        // Seller has 10 ore
        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory { company_id: 1, city_id: 1, resource_type_id: 1, quantity: 10 },
        );

        // Sell 5 @ 8.0, Buy 5 @ 10.0 — should match at clearing price 9.0
        state.market_orders.insert(1, MarketOrder {
            id: 1, city_id: 1, company_id: 1,
            resource_type_id: 1, order_type: "sell".into(),
            price: 8.0, quantity: 5, created_tick: 0,
        });
        state.market_orders.insert(2, MarketOrder {
            id: 2, city_id: 1, company_id: 2,
            resource_type_id: 1, order_type: "buy".into(),
            price: 10.0, quantity: 5, created_tick: 0,
        });

        clear_orders(&mut state, 1);

        let clearing = 9.0; // midpoint of 8 and 10
        assert!((state.companies[&1].cash - clearing * 5.0).abs() < 0.01);   // seller received
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
            Inventory { company_id: 1, city_id: 1, resource_type_id: 1, quantity: 10 },
        );
        state.market_orders.insert(1, MarketOrder {
            id: 1, city_id: 1, company_id: 1,
            resource_type_id: 1, order_type: "sell".into(),
            price: 5.0, quantity: 10, created_tick: 0,
        });
        state.market_orders.insert(2, MarketOrder {
            id: 2, city_id: 1, company_id: 2,
            resource_type_id: 1, order_type: "buy".into(),
            price: 5.0, quantity: 10, created_tick: 0,
        });

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
            Inventory { company_id: 1, city_id: 1, resource_type_id: 1, quantity: 10 },
        );
        state.market_orders.insert(1, MarketOrder {
            id: 1, city_id: 1, company_id: 1,
            resource_type_id: 1, order_type: "sell".into(),
            price: 10.0, quantity: 5, created_tick: 0,
        });
        state.market_orders.insert(2, MarketOrder {
            id: 2, city_id: 1, company_id: 2,
            resource_type_id: 1, order_type: "buy".into(),
            price: 5.0, quantity: 5, created_tick: 0,
        });

        clear_orders(&mut state, 1);

        // No trade: cash and inventory unchanged
        assert_eq!(state.companies[&2].cash, 100.0);
        assert!(state.market_history_buffer.is_empty());
    }
}
