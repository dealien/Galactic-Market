import re

with open('src/sim/decisions.rs', 'r') as f:
    content = f.read()

pattern = r'''    for company_id in due \{
        // Copy relevant company data locally to avoid immutable borrow while mutating state
        let \(status, home_city_id, last_trade_tick\) = \{
            let c = state\.companies\.get\(&company_id\)\.unwrap\(\);
            \(c\.status\.clone\(\), c\.home_city_id, c\.last_trade_tick\)
        \};

        // --- Liquidation AI: Post Fire-Sale Orders ---
        if status == "bankrupt" \{'''

replace = '''    for company_id in due {
        // Copy relevant company data locally to avoid immutable borrow while mutating state
        let (is_bankrupt, is_active, home_city_id, last_trade_tick) = {
            let c = state.companies.get(&company_id).unwrap();
            (c.status == "bankrupt", c.status == "active", c.home_city_id, c.last_trade_tick)
        };

        // --- Liquidation AI: Post Fire-Sale Orders ---
        if is_bankrupt {'''

content = re.sub(pattern, replace, content)

content = content.replace('if status != "active" {', 'if !is_active {')

with open('src/sim/decisions.rs', 'w') as f:
    f.write(content)
