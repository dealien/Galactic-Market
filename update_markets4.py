import re

with open('src/sim/markets.rs', 'r') as f:
    content = f.read()

content = content.replace('''            let (buy_qty, buy_price, buy_kind, buy_company_id) = {
                let o = &state.market_orders[&b_id];
                (o.quantity, o.price, o.order_kind.clone(), o.company_id)
            };
            let (sell_qty, sell_price, sell_kind, sell_company_id) = {
                let o = &state.market_orders[&s_id];
                (o.quantity, o.price, o.order_kind.clone(), o.company_id)
            };''', '''            let (buy_qty, buy_price, buy_is_limit, buy_company_id) = {
                let o = &state.market_orders[&b_id];
                (o.quantity, o.price, o.order_kind == "limit", o.company_id)
            };
            let (sell_qty, sell_price, sell_is_limit, sell_company_id) = {
                let o = &state.market_orders[&s_id];
                (o.quantity, o.price, o.order_kind == "limit", o.company_id)
            };''')

content = content.replace('''            // Check price compatibility for Limit vs Limit
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
            };''', '''            // Check price compatibility for Limit vs Limit
            if buy_is_limit && sell_is_limit && buy_price < sell_price {
                break; // No more matches possible
            }

            // Determine clearing price
            let clearing_price = match (buy_is_limit, sell_is_limit) {
                (false, false) => {
                    // Two market orders: use last known EMA or fallback
                    state
                        .ema_prices
                        .get(&(city_id, resource_type_id))
                        .copied()
                        .unwrap_or(10.0)
                }
                (false, true) => sell_price,
                (true, false) => buy_price,
                _ => (buy_price + sell_price) / 2.0, // Midpoint discovery for Limit-Limit
            };''')

with open('src/sim/markets.rs', 'w') as f:
    f.write(content)
