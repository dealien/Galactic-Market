import re

with open("src/sim/markets.rs", "r") as f:
    content = f.read()

# Add comments explaining the optimization as requested by code review!

new_content = content.replace("""    let mut markets: HashMap<(i32, i32), (Vec<OrderKey>, Vec<OrderKey>)> = HashMap::with_capacity(32);""", """    // Bolt optimization: Pre-allocate capacity for markets map to avoid re-allocating during tick loop.
    let mut markets: HashMap<(i32, i32), (Vec<OrderKey>, Vec<OrderKey>)> = HashMap::with_capacity(32);""")

new_content = new_content.replace("""        let entry = markets
            .entry((order.city_id, order.resource_type_id))
            .or_insert_with(|| (Vec::with_capacity(4), Vec::with_capacity(4)));""", """        // Bolt optimization: Pre-allocate vectors with capacity to avoid dynamic sizing overhead in tick loop
        let entry = markets
            .entry((order.city_id, order.resource_type_id))
            .or_insert_with(|| (Vec::with_capacity(4), Vec::with_capacity(4)));""")

new_content = new_content.replace("""        let last_ema_price = state
            .ema_prices
            .get(&(city_id, resource_type_id))
            .copied()
            .unwrap_or(10.0);""", """        // Bolt optimization: Hoist the loop-invariant EMA price lookup outside of the
        // while matching loop to avoid repeated O(1) hashmap lookups for every single trade.
        let last_ema_price = state
            .ema_prices
            .get(&(city_id, resource_type_id))
            .copied()
            .unwrap_or(10.0);""")

with open("src/sim/markets.rs", "w") as f:
    f.write(new_content)
