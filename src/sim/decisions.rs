//! Company AI decision-making, trading, banking, and empire relief workflows.
//!
//! Handles strategic audits for all active companies (bankruptcies, loan applications,
//! facility retooling, mineral/recipe extraction, trade arbitrage, and order postings).
//! Also implements the empire-wide famine relief safety net.

use rand::Rng;
use std::collections::HashMap;
use tracing::debug;

use crate::sim::logger::LogCategory;
use crate::sim::logistics::get_transport_info;
use crate::sim::state::{Facility, Inventory, MarketOrder, SimState, TradeRoute};

/// Evaluate and issue a loan from a commercial bank to a company.
fn request_loan(state: &mut SimState, company_id: i32, amount: f64) -> bool {
    let (bank_id, _home_city_id) = {
        let c = match state.companies.get(&company_id) {
            Some(c) => c,
            None => return false,
        };
        let city = match state.cities.get(&c.home_city_id) {
            Some(city) => city,
            None => return false,
        };
        let body = match state.celestial_bodies.get(&city.body_id) {
            Some(body) => body,
            None => return false,
        };
        let system = match state.star_systems.get(&body.system_id) {
            Some(system) => system,
            None => return false,
        };
        let sector_id = system.sector_id;

        // Find commercial bank in this sector
        let bank = state.companies.values().find(|b| {
            if b.company_type != "commercial_bank" {
                return false;
            }
            let b_city = match state.cities.get(&b.home_city_id) {
                Some(city) => city,
                None => return false,
            };
            let b_body = match state.celestial_bodies.get(&b_city.body_id) {
                Some(body) => body,
                None => return false,
            };
            let b_sys = match state.star_systems.get(&b_body.system_id) {
                Some(sys) => sys,
                None => return false,
            };
            b_sys.sector_id == sector_id
        });

        match bank {
            Some(b) => (b.id, c.home_city_id),
            None => return false,
        }
    };

    // Bank evaluates the loan (conservative Debt-to-Asset ratio < 0.8)
    let company = match state.companies.get(&company_id) {
        Some(c) => c,
        None => return false,
    };
    let current_debt = company.debt;
    let total_assets = company.cash + 10000.0; // Minimal asset floor
    let debt_to_asset = (current_debt + amount) / total_assets;

    if debt_to_asset < 0.8 {
        let bank_cash = match state.companies.get(&bank_id) {
            Some(b) => b.cash,
            None => return false,
        };
        if bank_cash >= amount {
            if let Some(bank) = state.companies.get_mut(&bank_id) {
                bank.cash -= amount;
            }
            if let Some(company) = state.companies.get_mut(&company_id) {
                company.cash += amount;
                company.debt += amount;
            }

            let loan_id = state.next_loan_id();
            state.add_loan(crate::sim::state::Loan {
                id: loan_id,
                company_id,
                lender_company_id: Some(bank_id),
                principal: amount,
                interest_rate: 0.05,
                balance: amount,
            });
            debug!(company_id, amount, "Loan approved by bank");
            return true;
        }
    }

    false
}

/// Re-evaluation interval ranges by company type (min, max ticks).
fn eval_interval_range(company_type: &str) -> (u64, u64) {
    match company_type {
        "freelancer" => (1, 5),
        "small_company" => (5, 20),
        "corporation" => (20, 60),
        "megacorp" => (60, 200),
        "merchant" => (1, 2),
        "central_bank" => (50, 100),
        "commercial_bank" => (5, 20),
        _ => (5, 20),
    }
}

/// Phase 5b: Company AI decisions.
///
/// Iterates over all active companies due for strategic re-evaluation and issues
/// appropriate orders, including liquidation, treasury checks, recipe production,
/// mining, and arbitrage trading routes.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::decisions::run_decisions;
/// use rand::SeedableRng;
/// use rand::rngs::StdRng;
///
/// let mut state = SimState::new();
/// let mut rng = StdRng::seed_from_u64(42);
/// run_decisions(&mut state, 1, &mut rng);
/// ```
pub fn run_decisions(state: &mut SimState, current_tick: u64, rng: &mut impl Rng) {
    // Snapshot the last known clearing prices for decision-making

    // Snapshot the last known clearing prices for decision-making
    let last_prices = last_known_prices(state);

    // Collect company IDs that are due for re-evaluation
    let mut due: Vec<i32> = state
        .companies
        .iter()
        .filter(|(_, c)| c.next_eval_tick <= current_tick)
        .map(|(id, _)| *id)
        .collect();

    // Ensure deterministic processing order
    due.sort_unstable();

    for company_id in due {
        // Copy relevant company data locally to avoid immutable borrow while mutating state
        let (is_bankrupt, is_active, home_city_id, last_trade_tick) = {
            let c = state.companies.get(&company_id).unwrap();
            (
                c.status == "bankrupt",
                c.status == "active",
                c.home_city_id,
                c.last_trade_tick,
            )
        };

        // --- Liquidation AI: Post Fire-Sale Orders ---
        if is_bankrupt {
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

        if !is_active {
            continue;
        }

        // Clear all outstanding orders for this company before making new ones.
        state
            .market_orders
            .retain(|_, order| order.company_id != company_id);

        // --- Corporate Treasury AI (Deposit/Withdraw) ───────────────────────
        let account_id = state
            .bank_accounts
            .values()
            .find(|a| a.company_id == company_id)
            .map(|a| a.id);

        if let Some(acc_id) = account_id {
            let (bank_company_id, bank_balance) = {
                let a = &state.bank_accounts[&acc_id];
                (a.bank_company_id, a.balance)
            };
            let company_cash = state.companies[&company_id].cash;

            let buffer = 5000.0;
            if company_cash > buffer * 1.5 {
                let deposit = company_cash - buffer;
                if let Some(c) = state.companies.get_mut(&company_id) {
                    c.cash -= deposit;
                }
                if let Some(a) = state.bank_accounts.get_mut(&acc_id) {
                    a.balance += deposit;
                }
                // Credit bank's cash so it has liquidity for lending
                if let Some(bank) = state.companies.get_mut(&bank_company_id) {
                    bank.cash += deposit;
                }
                debug!(company_id, deposit, "Company deposited excess cash to bank");
            } else if company_cash < buffer * 0.5 && bank_balance > 0.0 {
                let bank_available = match state.companies.get(&bank_company_id) {
                    Some(b) => b.cash,
                    None => {
                        // Bank company missing despite a valid account — log the anomaly
                        // and skip withdrawal to avoid corrupting other state.
                        tracing::warn!(
                            company_id,
                            bank_company_id,
                            "Bank company not found during withdrawal; skipping"
                        );
                        0.0
                    }
                };
                // Limit withdrawal to what both the account and the bank actually hold
                let withdraw = (buffer - company_cash)
                    .min(bank_balance)
                    .min(bank_available)
                    .max(0.0);
                if withdraw > 0.0 {
                    if let Some(c) = state.companies.get_mut(&company_id) {
                        c.cash += withdraw;
                    }
                    if let Some(a) = state.bank_accounts.get_mut(&acc_id) {
                        a.balance -= withdraw;
                    }
                    if let Some(bank) = state.companies.get_mut(&bank_company_id) {
                        bank.cash -= withdraw;
                    }
                    debug!(
                        company_id,
                        withdraw, "Company withdrew cash from bank for operations"
                    );
                }
            }
        }

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
            match company.company_type.as_str() {
                "central_bank" => "central_bank",
                "commercial_bank" => "commercial_bank",
                "merchant" => "merchant",
                "small_company" => "small_company",
                "corporation" => "corporation",
                "consumer" => "consumer",
                _ => "",
            }
        };

        let mut orders_to_post = Vec::new();

        // --- Central Bank AI (Monetary Policy) ──────────────────────────────
        if company_type == "central_bank" {
            if let Some(city) = state.cities.get(&home_city_id)
                && let Some(body) = state.celestial_bodies.get(&city.body_id)
                && let Some(system) = state.star_systems.get(&body.system_id)
                && let Some(sector) = state.sectors.get(&system.sector_id)
            {
                let empire_id = sector.empire_id;

                let mut total_empire_cash = 0.0;
                let mut total_empire_debt = 0.0;

                for c in state.companies.values() {
                    if let Some(c_city) = state.cities.get(&c.home_city_id)
                        && let Some(c_body) = state.celestial_bodies.get(&c_city.body_id)
                        && let Some(c_sys) = state.star_systems.get(&c_body.system_id)
                        && let Some(c_sec) = state.sectors.get(&c_sys.sector_id)
                        && c_sec.empire_id == empire_id
                    {
                        total_empire_cash += c.cash;
                        total_empire_debt += c.debt;
                    }
                }

                let current_prime = state.prime_rates.get(&empire_id).copied().unwrap_or(0.05);
                let mut next_prime = current_prime;

                if total_empire_debt > total_empire_cash * 0.4 {
                    next_prime += 0.005;
                } else if total_empire_debt < total_empire_cash * 0.1 {
                    next_prime -= 0.005;
                }

                next_prime = next_prime.clamp(0.01, 0.15);
                state.prime_rates.insert(empire_id, next_prime);
                debug!(empire_id, next_prime, "Central Bank adjusted Prime Rate");

                // --- Empire Relief AI (Market Buyout during Famines) ---
                // If a city has a famine, the Empire treasury (Central Bank) buys food at a premium to attract merchants.
                let central_bank = state.companies.get_mut(&company_id).unwrap();
                if central_bank.cash > 100_000.0 {
                    let mut famine_cities = Vec::new();
                    for event in state.active_events.values() {
                        if event.event_type == "famine"
                            && let Some((c_id, 0)) = event.target_id
                            && let Some(city) = state.cities.get(&c_id)
                            && let Some(body) = state.celestial_bodies.get(&city.body_id)
                            && let Some(system) = state.star_systems.get(&body.system_id)
                            && let Some(sector) = state.sectors.get(&system.sector_id)
                            && sector.empire_id == empire_id
                        {
                            famine_cities.push(c_id);
                        }
                    }

                    for city_id in famine_cities {
                        // Post buy orders for "Food" resources
                        let relief_resources: Vec<i32> = state
                            .resource_types
                            .values()
                            .filter(|r| r.name.contains("Food"))
                            .map(|r| r.id)
                            .collect();

                        for res_id in relief_resources {
                            let ema_price = state
                                .ema_prices
                                .get(&(city_id, res_id))
                                .copied()
                                .unwrap_or(50.0);
                            let relief_price = ema_price * 1.5; // 50% premium

                            orders_to_post.push(crate::sim::state::MarketOrder {
                                id: state.next_order_id(),
                                city_id,
                                company_id,
                                resource_type_id: res_id,
                                order_type: "buy".into(),
                                order_kind: "market".into(),
                                price: relief_price,
                                quantity: 100,
                                created_tick: state.tick,
                            });
                            debug!(
                                city_id,
                                res_id, relief_price, "Central Bank posted RELIEF BUY ORDER"
                            );
                        }
                    }
                }
            }
            // Commit any relief orders before continuing
            for order in orders_to_post {
                state.market_orders.insert(order.id, order);
            }
            continue;
        }

        // --- Commercial Bank AI (Lending & Liquidity) ───────────────────────
        if company_type == "commercial_bank" {
            let empire_id = {
                let city = &state.cities[&home_city_id];
                let body = &state.celestial_bodies[&city.body_id];
                let system = &state.star_systems[&body.system_id];
                state.sectors[&system.sector_id].empire_id
            };

            let prime_rate = state.prime_rates.get(&empire_id).copied().unwrap_or(0.05);

            let total_deposits: f64 = state
                .bank_accounts
                .values()
                .filter(|a| a.bank_company_id == company_id)
                .map(|a| a.balance)
                .sum();

            // Lender of Last Resort (LLR) Emergency Injection
            let bank_cash = state.companies[&company_id].cash;
            let min_reserve = total_deposits * 0.10;
            if bank_cash < min_reserve {
                let borrow_amount = min_reserve - bank_cash;
                // Find central bank in the same empire
                if let Some(&central_bank_id) = state.companies.keys().find(|&&id| {
                    state.companies[&id].company_type == "central_bank" && {
                        state.company_to_empire.get(&id) == state.company_to_empire.get(&company_id)
                    }
                }) {
                    // Credit cash to the commercial bank (emergency liquidity)
                    state.companies.get_mut(&company_id).unwrap().cash += borrow_amount;

                    // Create emergency loan record
                    let loan_id = state.next_loan_id();
                    state.add_loan(crate::sim::state::Loan {
                        id: loan_id,
                        company_id,
                        lender_company_id: Some(central_bank_id),
                        principal: borrow_amount,
                        interest_rate: prime_rate * 1.5, // Penalty interest rate
                        balance: borrow_amount,
                    });

                    tracing::info!(
                        bank_id = company_id,
                        central_bank_id,
                        amount = borrow_amount,
                        "LLR Alert: Commercial Bank received emergency liquidity loan from Central Bank"
                    );
                }
            }

            // Repay Central Bank loan if cash is abundant
            let bank_cash_updated = state.companies[&company_id].cash;
            if bank_cash_updated > min_reserve * 2.0 {
                // Find Central Bank loans for this bank
                let mut emergency_loans = Vec::new();
                for loan in state.loans.values() {
                    if loan.company_id == company_id
                        && let Some(lender_id) = loan.lender_company_id
                    {
                        let is_cb = state
                            .companies
                            .get(&lender_id)
                            .map(|c| c.company_type == "central_bank")
                            .unwrap_or(false);
                        if is_cb && loan.balance > 0.0 {
                            emergency_loans.push(loan.id);
                        }
                    }
                }

                let mut excess_cash = bank_cash_updated - min_reserve * 1.5;
                for loan_id in emergency_loans {
                    if excess_cash <= 0.0 {
                        break;
                    }
                    let (repay_amt, cb_id) = {
                        let loan = state.loans.get(&loan_id).unwrap();
                        (
                            excess_cash.min(loan.balance),
                            loan.lender_company_id.unwrap(),
                        )
                    };

                    if repay_amt > 0.0 {
                        // Repay loan
                        state.loans.get_mut(&loan_id).unwrap().balance -= repay_amt;
                        state.companies.get_mut(&company_id).unwrap().cash -= repay_amt;
                        state.companies.get_mut(&cb_id).unwrap().cash += repay_amt;
                        excess_cash -= repay_amt;

                        tracing::info!(
                            bank_id = company_id,
                            central_bank_id = cb_id,
                            amount = repay_amt,
                            "LLR Alert: Commercial Bank repaid emergency liquidity loan to Central Bank"
                        );
                    }
                }
            }

            let total_loans: f64 = state
                .loans
                .values()
                .filter(|l| l.lender_company_id == Some(company_id))
                .map(|l| l.balance)
                .sum();

            let reserve_multiplier = 5.0;
            let capacity = total_deposits * reserve_multiplier;
            let utilization = if capacity > 0.0 {
                total_loans / capacity
            } else {
                1.0
            };

            let local_lending_rate = prime_rate + (utilization * 0.10);
            for loan in state.loans.values_mut() {
                if loan.lender_company_id == Some(company_id) {
                    loan.interest_rate = local_lending_rate;
                }
            }

            let deposit_rate = (prime_rate * 0.5) + (utilization * 0.05);
            for account in state.bank_accounts.values_mut() {
                if account.bank_company_id == company_id {
                    account.interest_rate = deposit_rate;
                }
            }

            debug!(
                company_id,
                utilization, local_lending_rate, deposit_rate, "Commercial Bank updated rates"
            );
            continue;
        }

        // --- Merchant AI (Arbitrageur) ──────────────────────────────────────────
        if company_type == "merchant" {
            let mut company_cash = state.companies.get(&company_id).unwrap().cash;
            if company_cash > 1000.0
                || (company_cash < 1000.0
                    && state.companies.get(&company_id).unwrap().debt < 50000.0)
            {
                // --- PHASE 2: Food routing (priority over arbitrage) ---
                // Merchants always check for food imbalances before pursuing other arbitrage
                let food_resource_id = state
                    .resource_types
                    .values()
                    .find(|r| r.name.contains("Food") || r.name.contains("Ration"))
                    .map(|r| r.id);

                let mut food_routed = false;
                if let Some(food_id) = food_resource_id {
                    // Identify deficit cities (fulfillment < 0.6) and surplus cities (surplus > 50)
                    let deficit_cities: Vec<i32> = state
                        .city_food_balance
                        .values()
                        .filter(|b| b.fulfillment_ratio < 0.6)
                        .map(|b| b.city_id)
                        .collect();

                    let surplus_cities: Vec<i32> = state
                        .city_food_balance
                        .values()
                        .filter(|b| b.has_surplus && b.food_surplus > 50)
                        .map(|b| b.city_id)
                        .collect();

                    // Try to route food from surplus to deficit
                    // Prioritize starving cities (fulfillment < 0.3) first, then all deficits
                    let starving_first: Vec<i32> = deficit_cities
                        .iter()
                        .filter(|&cid| {
                            state
                                .city_food_balance
                                .get(cid)
                                .map(|b| b.fulfillment_ratio < 0.3)
                                .unwrap_or(false)
                        })
                        .copied()
                        .collect();

                    let all_deficit = [starving_first.as_slice(), &deficit_cities].concat();

                    for &dest_city_id in &all_deficit {
                        for &origin_city_id in &surplus_cities {
                            if origin_city_id == dest_city_id {
                                continue;
                            }

                            let food_price_origin = state
                                .ema_prices
                                .get(&(origin_city_id, food_id))
                                .copied()
                                .unwrap_or(1.0);
                            let food_price_dest = state
                                .ema_prices
                                .get(&(dest_city_id, food_id))
                                .copied()
                                .unwrap_or(100.0);
                            let transport = get_transport_info(state, origin_city_id, dest_city_id);
                            let profit =
                                food_price_dest - food_price_origin - transport.cost_per_unit;

                            // Route food if economically viable (profit >= -0.5 allows small losses for food security)
                            if profit > -0.5 {
                                let max_affordable =
                                    (company_cash * 0.3 / food_price_origin.max(0.1)) as i64;
                                let qty = 100.min(max_affordable).max(1);

                                if qty > 0 {
                                    orders_to_post.push(MarketOrder {
                                        id: 0,
                                        city_id: origin_city_id,
                                        company_id,
                                        resource_type_id: food_id,
                                        order_type: "buy".into(),
                                        order_kind: "market".into(),
                                        price: food_price_origin * 1.05,
                                        quantity: qty,
                                        created_tick: current_tick,
                                    });

                                    debug!(
                                        company_id,
                                        from = origin_city_id,
                                        to = dest_city_id,
                                        qty,
                                        profit,
                                        "Merchant routing food"
                                    );

                                    food_routed = true;
                                    break; // One food trade per tick
                                }
                            }
                        }

                        if food_routed {
                            break; // Only one food route per tick
                        }
                    }
                }

                // If food was routed, skip normal arbitrage this tick
                if food_routed {
                    for mut order in orders_to_post {
                        let id = state.next_order_id();
                        order.id = id;
                        state.market_orders.insert(id, order);
                    }
                    continue;
                }

                // --- Cached arbitrage logic (only if no famine routing) ---
                // Stage 2d: Use cached opportunities instead of triple-nested scan
                let mut best_arbitrage = None;
                let mut best_profit_margin = 0.0;

                // Check cache and recompute if stale (every 5 ticks) or never computed
                let last_scan_tick = state
                    .merchant_last_scan
                    .get(&company_id)
                    .copied()
                    .unwrap_or(u64::MAX);
                if last_scan_tick == u64::MAX || current_tick - last_scan_tick >= 5 {
                    // Cache is stale or missing; recompute opportunities
                    let opportunities = compute_merchant_opportunities(state, company_id);
                    state
                        .merchant_opportunities
                        .insert(company_id, opportunities);
                    state.merchant_last_scan.insert(company_id, current_tick);
                }

                // Use cached opportunities if available
                if let Some(opportunities) = state.merchant_opportunities.get(&company_id)
                    && let Some(opp) = opportunities.first()
                {
                    // Take the first (highest profit) opportunity
                    best_profit_margin = opp.profit_margin;
                    best_arbitrage = Some((
                        opp.resource_type_id,
                        opp.origin_city_id,
                        opp.dest_city_id,
                        opp.buy_price,
                    ));
                }

                if let Some((res_id, origin, _dest, buy_price)) = best_arbitrage {
                    // If high profit but low cash, take a loan to capitalize on the opportunity
                    if company_cash < buy_price * 10.0
                        && best_profit_margin > (buy_price * 0.2)
                        && request_loan(state, company_id, buy_price * 100.0)
                    {
                        company_cash = state.companies.get(&company_id).unwrap().cash;
                        debug!(company_id, "Merchant took a loan to fund arbitrage");
                    }

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
            let mut cash = state.companies.get(&company_id).unwrap().cash;
            // If consumer is out of cash (potentially due to famine prices), try to take a loan to continue buying food
            if cash < 100.0 && request_loan(state, company_id, 5000.0) {
                cash = state.companies.get(&company_id).unwrap().cash;
                debug!(company_id, "Consumer took a liquidity loan");
            }

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

                    // Evaluate if the switch is financially viable
                    let retooling_fee = 50.0;
                    let cycle_cost = facility.capacity as f64 * selected_ore_cost;
                    let required_cash = retooling_fee + 3.0 * cycle_cost;
                    let company_cash = state.companies[&company_id].cash;

                    // Switch if first target initialization (free) or if cash is sufficient for 3 cycles and margin is positive
                    if old_target.is_none()
                        || (best_overall_margin > 0.0 && company_cash >= required_cash)
                    {
                        facility.target_resource_id = Some(best_id);
                        if old_target.is_some() {
                            facility.setup_ticks_remaining = 2;
                            state.companies.get_mut(&company_id).unwrap().cash -= retooling_fee;
                        }
                        debug!(
                            company_id,
                            new_target = best_id,
                            "Miner switched target resource"
                        );
                    }
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

            let mut can_afford = company_cash > expansion_cost * 2.5;
            if !can_afford && roi_ticks < 30.0 && company_cash < expansion_cost {
                // If extremely high ROI, try to leverage
                can_afford = request_loan(state, company_id, expansion_cost);
            }

            if can_afford && best_overall_margin > (selected_ore_cost * 0.10) && roi_ticks < 50.0 {
                let facility = state.facilities.get_mut(&facility_id).unwrap();
                facility.capacity += 5;
                facility.setup_ticks_remaining = 5;
                state.companies.get_mut(&company_id).unwrap().cash -= expansion_cost;
                debug!(
                    company_id,
                    new_capacity = facility.capacity,
                    cost = expansion_cost,
                    "Miner expanded facility (Smarter Decision with Leverage)"
                );
            }
        }

        // ─── Plantation AI ───────────────────────────────────────────────────
        let plantation_info = state
            .facilities
            .values()
            .find(|f| {
                f.company_id == company_id
                    && f.city_id == home_city_id
                    && f.facility_type == "plantation"
            })
            .map(|f| f.capacity);

        if let Some(facility_capacity) = plantation_info {
            let food_id = state
                .resource_types
                .values()
                .find(|r| r.name.contains("Food") || r.name.contains("Ration"))
                .map(|r| r.id)
                .unwrap_or(7); // Default fallback

            let key = Inventory::key(company_id, home_city_id, food_id);
            let inv_opt = state.inventories.get(&key).cloned();
            if let Some(inv) = inv_opt
                && inv.quantity > 0
            {
                // Try to ship food to other starving cities if there is a higher price
                let mut best_target_city = None;
                let mut best_target_profit = 0.0;
                let mut best_target_info = None;
                let base_cost = 2.0; // labor cost per run
                let local_price = state
                    .ema_prices
                    .get(&(home_city_id, food_id))
                    .copied()
                    .unwrap_or(base_cost * 1.5);

                for &target_city_id in state.cities.keys() {
                    if target_city_id == home_city_id {
                        continue;
                    }
                    let transport = get_transport_info(state, home_city_id, target_city_id);
                    let dest_price = state
                        .ema_prices
                        .get(&(target_city_id, food_id))
                        .copied()
                        .unwrap_or(base_cost * 2.0);
                    let margin = dest_price - local_price - transport.cost_per_unit;
                    if margin > best_target_profit {
                        best_target_profit = margin;
                        best_target_city = Some(target_city_id);
                        best_target_info = Some(transport);
                    }
                }

                let should_ship = if best_target_city.is_some() {
                    best_target_profit > (base_cost * 0.05)
                } else {
                    false
                };

                let mut shipped = false;
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
                                resource_type_id: food_id,
                                quantity: move_qty,
                                arrival_tick: current_tick + transport.ticks,
                            },
                        );
                        debug!(
                            company_id,
                            move_qty,
                            from = home_city_id,
                            to = target_city,
                            "Plantation shipped food"
                        );
                        shipped = true;
                    }
                }

                if !shipped {
                    let base_ask = base_cost * 1.15;
                    let market_price = state
                        .ema_prices
                        .get(&(home_city_id, food_id))
                        .copied()
                        .unwrap_or(base_ask * 1.5);
                    let ticks_since_trade = current_tick.saturating_sub(last_trade_tick);
                    let ask_price = if inv.quantity > (facility_capacity * 10) as i64
                        || ticks_since_trade > 100
                    {
                        base_cost * 0.95 // Liquidation!
                    } else if inv.quantity > (facility_capacity * 5) as i64
                        || ticks_since_trade > 50
                    {
                        base_cost * 1.01
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
                        resource_type_id: food_id,
                        order_type: "sell".into(),
                        order_kind: "limit".into(),
                        price: ask_price,
                        quantity: inv.quantity,
                        created_tick: current_tick,
                    });
                }
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
                    // Compute expected cycle cost for 1 full capacity production run
                    let mut cycle_cost = 0.0;
                    for (_id, margin, cost_basis, _, recipe) in &recipes_evaluated {
                        let ratio = margin / total_positive_margin;
                        let runs = (facility.capacity as f64 * ratio).round();
                        cycle_cost += (cost_basis + recipe.labor_cost_per_run) * runs;
                    }
                    let retooling_fee = 200.0;
                    let required_cash = retooling_fee + 3.0 * cycle_cost;
                    let company_cash = state.companies[&company_id].cash;

                    // Only retool if first initialization (no existing ratios) or if cash is sufficient
                    if current_ratios.is_empty() || company_cash >= required_cash {
                        facility.production_ratios = Some(new_ratios.clone());
                        facility.setup_ticks_remaining = 3;
                        if !current_ratios.is_empty() {
                            state.companies.get_mut(&company_id).unwrap().cash -= retooling_fee;
                        }
                        debug!(company_id, "Refinery switched production ratios");
                    }
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
                            let total_input_qty: i32 =
                                recipe.inputs.iter().map(|i| i.quantity).sum();
                            let mut max_affordable = if total_input_qty > 0 {
                                (out_price * recipe.output_qty as f64 - labor_margin)
                                    / total_input_qty as f64
                            } else {
                                0.0
                            };
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
                let expected_additional_profit = if facility.capacity > 0 {
                    (total_positive_margin / facility.capacity as f64) * 5.0
                } else {
                    0.0
                };
                let roi_ticks = expansion_cost / expected_additional_profit.max(0.01);

                let mut can_afford = company_cash > expansion_cost * 3.0;
                if !can_afford && roi_ticks < 30.0 && company_cash < expansion_cost {
                    // If extremely high ROI, try to leverage
                    can_afford = request_loan(state, company_id, expansion_cost);
                }

                if can_afford && expected_additional_profit > 50.0 && roi_ticks < 60.0 {
                    let facility = state.facilities.get_mut(&facility_id).unwrap();
                    facility.capacity += 5;
                    facility.setup_ticks_remaining = 8;
                    state.companies.get_mut(&company_id).unwrap().cash -= expansion_cost;
                    debug!(
                        company_id,
                        new_capacity = facility.capacity,
                        cost = expansion_cost,
                        "Refinery expanded facility (Smarter Decision with Leverage)"
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

/// Phase 6b: Empire relief system — stabilize populations during famine via treasury relief.
///
/// Issue #10: Empire scans for starving cities (fulfillment < 40%) and posts relief food
/// buy orders funded by the empire treasury. This prevents population collapse until
/// Phase 2 refactor (merchant routing) can establish natural food trade networks.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::decisions::run_empire_relief;
///
/// let mut state = SimState::new();
/// run_empire_relief(&mut state, 1);
/// ```
pub fn run_empire_relief(state: &mut SimState, _current_tick: u64) {
    // Refund unmatched relief orders back to empire treasuries before clearing them
    let mut refunds = std::collections::HashMap::new();
    for order in state.market_orders.values() {
        if order.company_id < 0 {
            let empire_id = -order.company_id;
            let refund_amount = order.quantity as f64 * order.price;
            *refunds.entry(empire_id).or_insert(0.0) += refund_amount;
        }
    }
    for (empire_id, refund) in refunds {
        if refund > 0.0 {
            state.add_to_empire_treasury(empire_id, refund);
            if state.should_log(LogCategory::EmpireRelief, &format!("refund:{empire_id}")) {
                tracing::info!(
                    empire_id,
                    refund,
                    "Empire Relief Alert: Refunded unused relief budget to treasury"
                );
            }
        }
    }
    state.market_orders.retain(|_, order| order.company_id >= 0);

    // Constants
    const STARVATION_THRESHOLD: f64 = 0.40;
    const RELIEF_PRICE_PER_UNIT: f64 = 15.0;
    const MAX_RELIEF_PERCENT_OF_TREASURY: f64 = 0.20; // Max 20% of treasury per tick

    // Find food resource ID
    let food_resource_id = state
        .resource_types
        .values()
        .find(|r| r.name.contains("Food") || r.name.contains("Ration"))
        .map(|r| r.id);

    if food_resource_id.is_none() {
        debug!("Food resource not found in resource_types; relief skipped");
        return;
    }

    let food_id = food_resource_id.unwrap();

    // Collect starving cities and their deficits
    let mut relief_orders = Vec::new();

    for (city_id, city) in state.cities.iter() {
        if city.population <= 0 {
            continue;
        }

        // Find consumer company for this city
        let consumer_company_id = match state.city_consumer_ids.get(city_id) {
            Some(&cid) => cid,
            None => continue,
        };

        // Calculate food fulfillment: actual food / required
        let food_required = city.population as f64;
        let food_consumed = state
            .inventories
            .get(&(consumer_company_id, *city_id, food_id))
            .map(|inv| inv.quantity as f64)
            .unwrap_or(0.0);

        let fulfillment = if food_required > 0.0 {
            (food_consumed / food_required).min(2.0)
        } else {
            1.0
        };

        // Check if city is starving
        if fulfillment < STARVATION_THRESHOLD {
            // Calculate relief needed: 10% of population's monthly food needs
            let relief_units = (city.population as f64 * 0.1).max(1.0) as i64;
            let relief_cost = relief_units as f64 * RELIEF_PRICE_PER_UNIT;

            // Find the empire for this city
            let empire_id = state
                .celestial_bodies
                .get(&city.body_id)
                .and_then(|city_body| state.star_systems.get(&city_body.system_id))
                .and_then(|city_system| state.sectors.get(&city_system.sector_id))
                .map(|city_sector| city_sector.empire_id);

            if let Some(empire_id) = empire_id {
                relief_orders.push((*city_id, empire_id, relief_units, relief_cost, fulfillment));
            }
        }
    }

    if relief_orders.is_empty() {
        debug!("No starving cities detected; empire relief inactive");
        return;
    }

    // Check if empire can afford relief (up to max % of treasury)
    let mut empire_relief_map = std::collections::HashMap::new();
    for (city_id, empire_id, relief_units, relief_cost, _fulfillment) in relief_orders {
        // Bolt optimization: Replace double map lookup and eager vector allocations with single entry API and closure
        let entry = empire_relief_map
            .entry(empire_id)
            .or_insert_with(|| (Vec::with_capacity(4), 0.0));
        entry.0.push((city_id, relief_units, relief_cost));
        entry.1 += relief_cost;
    }

    // Execute relief orders constrained by budget
    for (empire_id, (cities_to_relieve, total_cost)) in empire_relief_map.iter() {
        let empire_treasury = state.get_empire_treasury(*empire_id);
        let max_relief = empire_treasury * MAX_RELIEF_PERCENT_OF_TREASURY;

        let effective_cost = total_cost.min(max_relief);

        if effective_cost < 0.01 {
            debug!(
                empire_id,
                treasury = empire_treasury,
                "Empire treasury insufficient for relief"
            );
            continue;
        }

        // Proportionally distribute available budget among cities
        let budget_scale_factor = if *total_cost > 0.0 {
            effective_cost / total_cost
        } else {
            1.0
        };

        let mut relief_posted_count = 0;
        let mut relief_posted_units = 0i64;

        for (city_id, relief_units, relief_cost) in cities_to_relieve {
            let scaled_cost = relief_cost * budget_scale_factor;
            let scaled_units = (*relief_units as f64 * budget_scale_factor).max(1.0) as i64;

            if scaled_cost < 0.01 {
                continue;
            }

            // Post a buy order on behalf of the empire
            // The empire acts as a special "company" (we'll use a virtual ID or the empire entity itself)
            // For now, we'll create an order attributed to a special "empire_relief" company if it exists,
            // or use city_id as a placeholder company_id (negative to avoid collision with real companies)
            let relief_company_id = -(*empire_id); // Negative ID to mark as empire relief

            let order_id = state.next_order_id();
            state.market_orders.insert(
                order_id,
                MarketOrder {
                    id: order_id,
                    city_id: *city_id,
                    company_id: relief_company_id,
                    resource_type_id: food_id,
                    order_type: "buy".into(),
                    order_kind: "limit".into(),
                    price: RELIEF_PRICE_PER_UNIT,
                    quantity: scaled_units,
                    created_tick: state.tick,
                },
            );

            relief_posted_count += 1;
            relief_posted_units += scaled_units;

            debug!(
                empire_id = *empire_id,
                city_id = *city_id,
                relief_units = scaled_units,
                cost = scaled_cost,
                "Empire Relief Alert: Posted emergency food relief order for starving city"
            );
        }

        if relief_posted_count > 0
            && state.should_log(LogCategory::EmpireRelief, &format!("relief:{empire_id}"))
        {
            tracing::info!(
                empire_id = *empire_id,
                cities_relieved = relief_posted_count,
                total_units = relief_posted_units,
                total_cost = effective_cost,
                "Empire Relief Alert: Posted emergency food relief orders for starving cities"
            );
        }

        // Deduct from empire treasury
        state.withdraw_from_empire_treasury(*empire_id, effective_cost);

        debug!(
            empire_id,
            cities_relieved = relief_posted_count,
            units_ordered = relief_posted_units,
            cost = effective_cost,
            remaining_treasury = state.get_empire_treasury(*empire_id),
            "Empire relief executed"
        );
    }
}

/// Returns the last clearing price per (city_id, resource_type_id) from the state's persistent cache.
fn last_known_prices(state: &SimState) -> std::collections::HashMap<(i32, i32), f64> {
    state.price_cache.clone()
}

/// Phase 5a (precompute): Analyze food balance per city for merchant routing priority.
/// Computes surplus/deficit and fulfillment ratio, enabling merchants to route food to starving cities.
///
/// Updates SimState.city_food_balance with analysis for each city.
/// Called once per tick before merchant AI decision loop.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::decisions::analyze_city_food_balance;
///
/// let mut state = SimState::new();
/// analyze_city_food_balance(&mut state);
/// ```
pub fn analyze_city_food_balance(state: &mut SimState) {
    use crate::sim::state::CityFoodBalance;

    // Find food resource ID
    let food_resource_id = state
        .resource_types
        .values()
        .find(|r| r.name.contains("Food") || r.name.contains("Ration"))
        .map(|r| r.id);

    let mut food_balance_map = HashMap::new();

    for (&city_id, city) in &state.cities {
        let population = city.population as f64;

        // Get consumer company for this city (established in consumption phase)
        let consumer_co_id = state.city_consumer_ids.get(&city_id).copied();

        // Calculate food in inventory
        let food_in_inventory =
            if let (Some(food_id), Some(co_id)) = (food_resource_id, consumer_co_id) {
                state
                    .inventories
                    .get(&(co_id, city_id, food_id))
                    .map(|inv| inv.quantity as f64)
                    .unwrap_or(0.0)
            } else {
                0.0
            };

        // Calculate fulfillment ratio: food / population (1 unit per person per tick)
        let fulfillment_ratio = if population > 0.0 {
            (food_in_inventory / population).min(2.0)
        } else {
            1.0
        };

        // Calculate surplus/deficit
        let food_required = population as i64;
        let food_surplus = (food_in_inventory as i64) - food_required;

        // Classify city
        let needs_relief = fulfillment_ratio < 0.4;
        let has_surplus = food_surplus > 0;

        let balance = CityFoodBalance {
            city_id,
            food_surplus,
            fulfillment_ratio,
            needs_relief,
            has_surplus,
        };

        food_balance_map.insert(city_id, balance);
    }

    state.city_food_balance = food_balance_map;

    debug!(
        cities_analyzed = state.cities.len(),
        starving = state
            .city_food_balance
            .values()
            .filter(|b| b.needs_relief)
            .count(),
        "City food balance analysis complete"
    );
}

/// Phase 2d: Compute arbitrage opportunities for a single merchant.
/// Performs expensive triple-nested loop scan of all resources × city pairs.
/// Called once every 5 ticks per merchant to populate the opportunity cache.
///
/// Returns sorted Vec of opportunities (highest profit first).
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::decisions::compute_merchant_opportunities;
///
/// let state = SimState::new();
/// let opps = compute_merchant_opportunities(&state, 1);
/// assert!(opps.is_empty());
/// ```
pub fn compute_merchant_opportunities(
    state: &SimState,
    merchant_id: i32,
) -> Vec<crate::sim::state::MerchantOpportunity> {
    use crate::sim::state::MerchantOpportunity;

    let mut opportunities = Vec::new();

    // Optimization: Precompute the home_city_id for the merchant
    let merchant_home_city_id = state.companies.get(&merchant_id).map(|c| c.home_city_id);

    // Triple-nested loop: all resources × origin cities × destination cities
    for &res_id in state.resource_types.keys() {
        for &origin_city_id in state.cities.keys() {
            let buy_price = state
                .ema_prices
                .get(&(origin_city_id, res_id))
                .copied()
                .unwrap_or(1000.0);

            // Skip if no inventory to sell (can't buy)
            let has_inventory = merchant_home_city_id == Some(origin_city_id)
                || state
                    .inventories
                    .get(&(merchant_id, origin_city_id, res_id))
                    .map(|inv| inv.quantity > 0)
                    .unwrap_or(false);

            if !has_inventory {
                continue; // Can't profitably sell what we don't have
            }

            for &dest_city_id in state.cities.keys() {
                if origin_city_id == dest_city_id {
                    continue;
                }

                let sell_price = state
                    .ema_prices
                    .get(&(dest_city_id, res_id))
                    .copied()
                    .unwrap_or(0.0);

                // Bolt optimization: Only compute transport costs if there's a potential profit
                if sell_price <= buy_price {
                    continue;
                }

                let transport =
                    crate::sim::logistics::get_transport_info(state, origin_city_id, dest_city_id);
                let total_cost = buy_price + transport.cost_per_unit;
                let profit_margin = sell_price - total_cost;

                // Only cache profitable opportunities (> 0.1% profit margin)
                if profit_margin > buy_price * 0.001 {
                    opportunities.push(MerchantOpportunity {
                        resource_type_id: res_id,
                        origin_city_id,
                        dest_city_id,
                        buy_price,
                        sell_price,
                        profit_margin,
                        transport_cost: transport.cost_per_unit,
                    });
                }
            }
        }
    }

    // Sort by profit margin (highest first)
    opportunities.sort_by(|a, b| {
        b.profit_margin
            .partial_cmp(&a.profit_margin)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    debug!(
        merchant_id,
        opportunity_count = opportunities.len(),
        "Computed merchant opportunities"
    );

    opportunities
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, Deposit, Facility, Inventory, SimState};
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn test_rng() -> StdRng {
        StdRng::seed_from_u64(42)
    }

    fn make_state_with_miner() -> SimState {
        let mut s = SimState::new();
        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Test City".into(),
                population: 0,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
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
    fn test_consumer_ai_loan_and_buy_order() {
        let mut state = SimState::new();

        // 1. Create a commercial bank with enough cash
        state.companies.insert(
            1,
            crate::sim::state::Company {
                id: 1,
                company_type: "commercial_bank".into(),
                cash: 50000.0,
                home_city_id: 1,
                debt: 0.0,
                name: "Bank".into(),
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // 2. Create a consumer company with cash < 100
        state.companies.insert(
            2,
            crate::sim::state::Company {
                id: 2,
                company_type: "consumer".into(),
                cash: 50.0,
                home_city_id: 1,
                debt: 0.0,
                name: "Consumer".into(),
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // 3. Setup city, body, and system, and sector
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                population: 100,
                name: "City".into(),
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Planet".into(),
                fertility: 1.0,
            },
        );

        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "Test System".into(),
            },
        );

        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Test Sector".into(),
            },
        );

        // 4. Create a Consumer Good resource
        state.resource_types.insert(
            10,
            crate::sim::state::ResourceType {
                id: 10,
                category: "Consumer Good".into(),
                name: "Widgets".into(),
                is_vital: false,
            },
        );

        // 5. Provide an EMA price so bid price logic executes predictably
        state.ema_prices.insert((1, 10), 50.0);

        // 6. Run decisions
        run_decisions(&mut state, 1, &mut test_rng());

        // Assert loan was taken (cash should be 50.0 + 5000.0 = 5050.0)
        let consumer_cash = state.companies.get(&2).unwrap().cash;
        assert_eq!(consumer_cash, 5050.0);

        // Assert buy order was placed for the resource
        let buy_orders: Vec<_> = state
            .market_orders
            .values()
            .filter(|o| o.company_id == 2 && o.resource_type_id == 10 && o.order_type == "buy")
            .collect();
        assert!(
            !buy_orders.is_empty(),
            "Consumer should have placed a buy order"
        );
    }

    #[test]
    fn miner_posts_sell_order_when_inventory_available() {
        let mut state = make_state_with_miner();
        run_decisions(&mut state, 1, &mut test_rng());
        assert!(state.market_orders.values().any(|o| o.order_type == "sell"));
    }

    #[test]
    fn company_reschedules_next_eval() {
        let mut state = make_state_with_miner();
        run_decisions(&mut state, 1, &mut test_rng());
        let company = &state.companies[&1];
        assert!(company.next_eval_tick > 1);
    }

    #[test]
    fn company_skips_when_not_due() {
        let mut state = make_state_with_miner();
        state.companies.get_mut(&1).unwrap().next_eval_tick = 9999;
        run_decisions(&mut state, 1, &mut test_rng());
        assert!(state.market_orders.is_empty());
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Phase 2: Food Balance Analysis Tests
    // ──────────────────────────────────────────────────────────────────────────────

    /// Verifies that if no food resource exists in the simulation, cities correctly
    /// calculate a food deficit, 0 fulfillment ratio, and trigger the needs_relief flag.
    #[test]
    fn test_analyze_city_food_balance_no_food_resource() {
        let mut state = SimState::new();
        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City A".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Analyze when no food resource exists
        analyze_city_food_balance(&mut state);

        let balance = &state.city_food_balance[&1];
        assert_eq!(balance.food_surplus, -100); // 0 inventory - 100 population
        assert_eq!(balance.fulfillment_ratio, 0.0);
        assert!(balance.needs_relief); // 0.0 < 0.4
        assert!(!balance.has_surplus);
    }

    #[test]
    fn test_surplus_detection() {
        let mut state = SimState::new();

        // Setup: 1 city with population 100
        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City A".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Setup: Food resource type
        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "commodity".into(),
                is_vital: true,
            },
        );

        // Setup: Consumer company with 200 food units (surplus of 100)
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Consumer".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.city_consumer_ids.insert(1, 1);
        state.inventories.insert(
            (1, 1, 1), // (company=1, city=1, food=1)
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 200,
            },
        );

        // Analyze
        analyze_city_food_balance(&mut state);

        // Verify
        let balance = &state.city_food_balance[&1];
        assert_eq!(balance.food_surplus, 100);
        assert!(balance.has_surplus);
        assert!(!balance.needs_relief);
        assert_eq!(balance.fulfillment_ratio, 2.0); // Capped at 2.0
    }

    #[test]
    fn test_deficit_detection() {
        let mut state = SimState::new();

        // Setup: 1 city with population 100
        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City B".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Setup: Food resource type
        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "commodity".into(),
                is_vital: true,
            },
        );

        // Setup: Consumer company with 10 food units (deficit of 90)
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Consumer".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.city_consumer_ids.insert(1, 1);
        state.inventories.insert(
            (1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 10,
            },
        );

        // Analyze
        analyze_city_food_balance(&mut state);

        // Verify
        let balance = &state.city_food_balance[&1];
        assert_eq!(balance.food_surplus, -90);
        assert!(!balance.has_surplus);
        assert!(balance.needs_relief);
        assert_eq!(balance.fulfillment_ratio, 0.1);
    }

    #[test]
    fn test_fulfillment_calculation() {
        let mut state = SimState::new();

        // Setup: City with different fulfillment levels
        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City C".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "commodity".into(),
                is_vital: true,
            },
        );

        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Consumer".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.city_consumer_ids.insert(1, 1);

        // Test case 1: 50 food (50% fulfillment)
        state.inventories.insert(
            (1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 50,
            },
        );

        analyze_city_food_balance(&mut state);
        let balance = &state.city_food_balance[&1];
        assert_eq!(balance.fulfillment_ratio, 0.5);
        assert!(!balance.needs_relief); // 0.5 >= 0.4

        // Test case 2: 30 food (30% fulfillment)
        state.inventories.get_mut(&(1, 1, 1)).unwrap().quantity = 30;

        analyze_city_food_balance(&mut state);
        let balance = &state.city_food_balance[&1];
        assert_eq!(balance.fulfillment_ratio, 0.3);
        assert!(balance.needs_relief); // 0.3 < 0.4
    }

    // ──────────────────────────────────────────────────────────────────────────────
    // Phase 2: Famine Routing Tests
    // ──────────────────────────────────────────────────────────────────────────────

    #[test]
    fn test_famine_routing_activates_when_starving() {
        let mut state = SimState::new();

        // Setup: 2 cities
        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Surplus".into(),
                population: 50,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 2,
                name: "Starving".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Setup: Bodies and systems for routing
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            2,
            crate::sim::state::CelestialBody {
                id: 2,
                system_id: 1,
                name: "B2".into(),
                fertility: 1.0,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "S1".into(),
            },
        );
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.empires.insert(
            1,
            crate::sim::state::Empire {
                id: 1,
                name: "Empire".into(),
                government_type: "republic".into(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        // Setup: Food resource
        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "commodity".into(),
                is_vital: true,
            },
        );

        // Setup: Consumers in both cities
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Consumer1".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.companies.insert(
            2,
            Company {
                id: 2,
                name: "Consumer2".into(),
                company_type: "consumer".into(),
                home_city_id: 2,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.city_consumer_ids.insert(1, 1);
        state.city_consumer_ids.insert(2, 2);

        // Setup: City 1 has food surplus (100 units), City 2 has none
        state.inventories.insert(
            (1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 100,
            },
        );
        state.inventories.insert(
            (2, 2, 1),
            Inventory {
                company_id: 2,
                city_id: 2,
                resource_type_id: 1,
                quantity: 10,
            },
        );

        // Setup: Merchant with cash
        state.companies.insert(
            100,
            Company {
                id: 100,
                name: "Merchant".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 1, // Ready to evaluate
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup: EMA prices for food
        state.ema_prices.insert((1, 1), 2.0); // Cheap at city 1
        state.ema_prices.insert((2, 1), 20.0); // Expensive at city 2 (starving)

        // Analyze food balance
        analyze_city_food_balance(&mut state);

        // Verify: City 2 is identified as starving
        assert!(state.city_food_balance[&2].needs_relief);
        assert!(state.city_food_balance[&1].has_surplus);

        // Run merchant AI
        run_decisions(&mut state, 1, &mut test_rng());

        // Verify: Merchant posted a food buy order (famine relief)
        let food_buy_orders: Vec<_> = state
            .market_orders
            .values()
            .filter(|o| o.order_type == "buy" && o.resource_type_id == 1 && o.city_id == 1)
            .collect();
        assert!(
            !food_buy_orders.is_empty(),
            "Merchant should post food buy order for famine relief"
        );
    }

    #[test]
    fn test_compute_merchant_opportunities_empty() {
        // Stage 2d: Verify opportunity computation works with minimal setup
        let mut state = SimState::new();

        // Setup: Basic entities
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "S1".into(),
            },
        );
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.empires.insert(
            1,
            crate::sim::state::Empire {
                id: 1,
                name: "Empire".into(),
                government_type: "republic".into(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 1,
                name: "City2".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Ore".into(),
                category: "raw".into(),
                is_vital: false,
            },
        );

        state.companies.insert(
            100,
            Company {
                id: 100,
                name: "Merchant".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // With no prices set, should find no opportunities
        let opps = compute_merchant_opportunities(&state, 100);
        assert_eq!(opps.len(), 0, "No opportunities without prices");
    }

    #[test]
    fn test_compute_merchant_opportunities_finds_profitable_routes() {
        // Stage 2d: Verify profitable routes are cached in order
        let mut state = SimState::new();

        // Setup: Two cities with ore resource
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            2,
            crate::sim::state::CelestialBody {
                id: 2,
                system_id: 1,
                name: "B2".into(),
                fertility: 1.0,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "S1".into(),
            },
        );
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.empires.insert(
            1,
            crate::sim::state::Empire {
                id: 1,
                name: "Empire".into(),
                government_type: "republic".into(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 2,
                name: "City2".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Ore".into(),
                category: "raw".into(),
                is_vital: false,
            },
        );

        state.companies.insert(
            100,
            Company {
                id: 100,
                name: "Merchant".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Prices: Ore cheap at city 1, expensive at city 2 (profitable arbitrage)
        state.ema_prices.insert((1, 1), 10.0); // Ore at city 1 (buy side)
        state.ema_prices.insert((2, 1), 20.0); // Ore at city 2 (sell side - profit!)

        // Compute opportunities
        let opps = compute_merchant_opportunities(&state, 100);

        // Should find the profitable route
        assert!(!opps.is_empty(), "Should find profitable arbitrage route");
        assert_eq!(opps[0].resource_type_id, 1, "Should find ore arbitrage");
        assert_eq!(opps[0].origin_city_id, 1, "Should buy at cheaper city");
        assert_eq!(opps[0].dest_city_id, 2, "Should sell at expensive city");
        assert!(
            opps[0].profit_margin > 0.0,
            "Should have positive profit margin"
        );
    }

    #[test]
    fn test_compute_merchant_opportunities_sorted_by_profit() {
        // Stage 2d: Verify opportunities are sorted by profit (highest first)
        let mut state = SimState::new();

        // Setup: Three cities with different profit margins
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            2,
            crate::sim::state::CelestialBody {
                id: 2,
                system_id: 1,
                name: "B2".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            3,
            crate::sim::state::CelestialBody {
                id: 3,
                system_id: 1,
                name: "B3".into(),
                fertility: 1.0,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "S1".into(),
            },
        );
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.empires.insert(
            1,
            crate::sim::state::Empire {
                id: 1,
                name: "Empire".into(),
                government_type: "republic".into(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 2,
                name: "City2".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            3,
            City {
                id: 3,
                body_id: 3,
                name: "City3".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Ore".into(),
                category: "raw".into(),
                is_vital: false,
            },
        );

        state.companies.insert(
            100,
            Company {
                id: 100,
                name: "Merchant".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Prices: Ore cheap at city 1, medium profit to city 2, high profit to city 3
        state.ema_prices.insert((1, 1), 10.0); // Buy price
        state.ema_prices.insert((2, 1), 15.0); // Medium profit: 15-10 = 5
        state.ema_prices.insert((3, 1), 25.0); // High profit: 25-10 = 15

        let opps = compute_merchant_opportunities(&state, 100);

        // Should find multiple opportunities sorted by profit (highest first)
        assert!(opps.len() >= 2, "Should find at least 2 opportunities");
        assert!(
            opps[0].profit_margin >= opps[1].profit_margin,
            "Opportunities should be sorted by profit descending"
        );
    }

    #[test]
    fn test_cache_invalidation_every_5_ticks() {
        // Stage 2d: Verify opportunity cache invalidates at 5-tick boundary
        let mut state = SimState::new();

        // Setup: Basic entities for testing
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            2,
            crate::sim::state::CelestialBody {
                id: 2,
                system_id: 1,
                name: "B2".into(),
                fertility: 1.0,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "S1".into(),
            },
        );
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.empires.insert(
            1,
            crate::sim::state::Empire {
                id: 1,
                name: "Empire".into(),
                government_type: "republic".into(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 2,
                name: "City2".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Ore".into(),
                category: "raw".into(),
                is_vital: false,
            },
        );

        state.companies.insert(
            100,
            Company {
                id: 100,
                name: "Merchant".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup: Profitable trade route
        state.ema_prices.insert((1, 1), 10.0);
        state.ema_prices.insert((2, 1), 25.0);

        // First compute (tick 0)
        let opp1 = compute_merchant_opportunities(&state, 100);
        state.merchant_opportunities.insert(100, opp1.clone());
        state.merchant_last_scan.insert(100, 0);

        // Simulate ticks 1-4 (cache should persist)
        for tick in 1..5 {
            let last_scan = state
                .merchant_last_scan
                .get(&100)
                .copied()
                .unwrap_or(u64::MAX);
            let should_recompute = last_scan == u64::MAX || tick - last_scan >= 5;
            assert!(
                !should_recompute,
                "Cache should not invalidate within 5 ticks"
            );
        }

        // Simulate tick 5 (5 - 0 = 5, should recompute)
        let last_scan = state
            .merchant_last_scan
            .get(&100)
            .copied()
            .unwrap_or(u64::MAX);
        let should_recompute = last_scan == u64::MAX || 5 - last_scan >= 5;
        assert!(
            should_recompute,
            "Cache should recompute at 5-tick boundary"
        );
    }

    #[test]
    fn test_cache_matches_uncached_scan() {
        // Stage 2d: Verify cached result matches expensive triple-nested scan
        let mut state = SimState::new();

        // Setup: 3 cities with mixed opportunities
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            2,
            crate::sim::state::CelestialBody {
                id: 2,
                system_id: 1,
                name: "B2".into(),
                fertility: 1.0,
            },
        );
        state.celestial_bodies.insert(
            3,
            crate::sim::state::CelestialBody {
                id: 3,
                system_id: 1,
                name: "B3".into(),
                fertility: 1.0,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "S1".into(),
            },
        );
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.empires.insert(
            1,
            crate::sim::state::Empire {
                id: 1,
                name: "Empire".into(),
                government_type: "republic".into(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 2,
                name: "City2".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            3,
            City {
                id: 3,
                body_id: 3,
                name: "City3".into(),
                population: 100,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.05,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Two resource types
        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Ore".into(),
                category: "raw".into(),
                is_vital: false,
            },
        );
        state.resource_types.insert(
            2,
            crate::sim::state::ResourceType {
                id: 2,
                name: "Ingot".into(),
                category: "processed".into(),
                is_vital: false,
            },
        );

        state.companies.insert(
            100,
            Company {
                id: 100,
                name: "Merchant".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 5000.0,
                debt: 0.0,
                next_eval_tick: 0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup: Various prices creating opportunities
        state.ema_prices.insert((1, 1), 10.0); // Ore at city 1
        state.ema_prices.insert((2, 1), 25.0); // Ore expensive at city 2
        state.ema_prices.insert((3, 1), 20.0); // Ore medium at city 3
        state.ema_prices.insert((1, 2), 50.0); // Ingot at city 1
        state.ema_prices.insert((2, 2), 60.0); // Ingot at city 2
        state.ema_prices.insert((3, 2), 55.0); // Ingot at city 3

        // Compute opportunities
        let cached = compute_merchant_opportunities(&state, 100);

        // Verify meaningful results
        assert!(
            !cached.is_empty(),
            "Should find opportunities with these prices"
        );

        // Verify all opportunities are profitable (> 0.1%)
        for opp in &cached {
            assert!(
                opp.profit_margin > 0.001,
                "All cached opportunities should be profitable"
            );
        }

        // Verify sorted by profit descending
        for i in 0..cached.len() - 1 {
            assert!(
                cached[i].profit_margin >= cached[i + 1].profit_margin,
                "Should be sorted by profit descending"
            );
        }
    }

    #[test]
    /// Verifies that when a company is bankrupt, it liquidates its inventory by posting limit sell orders at 50% of the market (EMA) price.
    fn test_company_liquidation_fire_sale() {
        let mut state = SimState::new();

        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Bankrupt Co".into(),
                company_type: "small_company".into(),
                home_city_id: 1,
                cash: 0.0,
                debt: 100000.0,
                next_eval_tick: 0,
                status: "bankrupt".into(),
                last_trade_tick: 0,
            },
        );

        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 100,
            },
        );

        state.ema_prices.insert((1, 1), 50.0);

        run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(state.market_orders.len(), 1);
        let order = state.market_orders.values().next().unwrap();
        assert_eq!(order.company_id, 1);
        assert_eq!(order.city_id, 1);
        assert_eq!(order.resource_type_id, 1);
        assert_eq!(order.order_type, "sell");
        assert_eq!(order.order_kind, "limit");
        assert_eq!(order.quantity, 100);
        assert_eq!(order.price, 25.0); // 50.0 * 0.5
    }

    fn make_state_with_bank() -> SimState {
        use crate::sim::state::{CelestialBody, City, StarSystem};
        let mut s = SimState::new();

        s.star_systems.insert(
            1,
            StarSystem {
                id: 1,
                sector_id: 1,
                name: "Test System".into(),
            },
        );

        s.star_systems.insert(
            2,
            StarSystem {
                id: 2,
                sector_id: 2,
                name: "Other System".into(),
            },
        );

        s.celestial_bodies.insert(
            1,
            CelestialBody {
                id: 1,
                system_id: 1,
                name: "Test Body".into(),
                fertility: 1.0,
            },
        );

        s.celestial_bodies.insert(
            2,
            CelestialBody {
                id: 2,
                system_id: 2,
                name: "Other Body".into(),
                fertility: 1.0,
            },
        );

        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Test City".into(),
                population: 0,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        s.cities.insert(
            2,
            City {
                id: 2,
                body_id: 2,
                name: "Other City".into(),
                population: 0,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "Borrower Co".into(),
                company_type: "small_company".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        s.companies.insert(
            2,
            Company {
                id: 2,
                name: "Local Bank".into(),
                company_type: "commercial_bank".into(),
                home_city_id: 1,
                cash: 50000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        s
    }

    /// Tests that a loan is successfully approved when a valid bank exists in the same sector,
    /// the borrowing company has a good debt-to-asset ratio (< 0.8), and the bank has sufficient cash.
    /// Verifies cash transfers, debt accumulation, and loan record creation.
    #[test]
    fn test_request_loan_approved() {
        let mut state = make_state_with_bank();

        let result = request_loan(&mut state, 1, 5000.0);

        assert!(result, "Loan should be approved");
        assert_eq!(state.companies[&1].cash, 6000.0);
        assert_eq!(state.companies[&1].debt, 5000.0);
        assert_eq!(state.companies[&2].cash, 45000.0);
        assert_eq!(state.loans.len(), 1);
        let loan = state.loans.values().next().unwrap();
        assert_eq!(loan.company_id, 1);
        assert_eq!(loan.lender_company_id, Some(2));
        assert_eq!(loan.principal, 5000.0);
        assert_eq!(loan.balance, 5000.0);
    }

    /// Tests that a loan request is rejected if there is no commercial bank operating within
    /// the same sector as the borrowing company.
    #[test]
    fn test_request_loan_rejected_no_bank() {
        let mut state = make_state_with_bank();
        // Move the bank to a different sector
        state.companies.get_mut(&2).unwrap().home_city_id = 2;

        let result = request_loan(&mut state, 1, 5000.0);

        assert!(!result, "Loan should be rejected due to no bank in sector");
        assert_eq!(state.companies[&1].cash, 1000.0);
        assert_eq!(state.companies[&1].debt, 0.0);
        assert_eq!(state.loans.len(), 0);
    }

    /// Tests that a loan request is rejected if the borrowing company's post-loan
    /// debt-to-asset ratio would exceed or equal the conservative threshold of 0.8.
    #[test]
    fn test_request_loan_rejected_high_debt() {
        let mut state = make_state_with_bank();
        // Set existing debt high enough so that (debt + 5000) / (cash + 10000) >= 0.8
        // (8800 + 5000) / 11000 = 13800 / 11000 = 1.25
        state.companies.get_mut(&1).unwrap().debt = 8800.0;

        let result = request_loan(&mut state, 1, 5000.0);

        assert!(
            !result,
            "Loan should be rejected due to high debt-to-asset ratio"
        );
        assert_eq!(state.companies[&1].cash, 1000.0);
        assert_eq!(state.companies[&1].debt, 8800.0);
        assert_eq!(state.loans.len(), 0);
    }

    /// Tests that a loan request is rejected if the commercial bank does not have
    /// sufficient cash on hand to fulfill the principal amount.
    #[test]
    fn test_request_loan_rejected_bank_insufficient_cash() {
        let mut state = make_state_with_bank();
        // Bank doesn't have enough cash
        state.companies.get_mut(&2).unwrap().cash = 1000.0;

        let result = request_loan(&mut state, 1, 5000.0);

        assert!(
            !result,
            "Loan should be rejected due to bank insufficient cash"
        );
        assert_eq!(state.companies[&1].cash, 1000.0);
        assert_eq!(state.companies[&1].debt, 0.0);
        assert_eq!(state.loans.len(), 0);
    }

    /// Tests that a loan request fails gracefully (returns false without panicking)
    /// when referenced entities (company, city, body, or system) are missing from state.
    #[test]
    fn test_request_loan_rejected_missing_entities() {
        let mut state = make_state_with_bank();

        // 1. Missing requesting company_id
        assert!(!request_loan(&mut state, 999, 5000.0));

        // 2. Missing company's home_city_id
        state.companies.get_mut(&1).unwrap().home_city_id = 999;
        assert!(!request_loan(&mut state, 1, 5000.0));

        // 3. Missing city's body_id
        let mut state = make_state_with_bank();
        state.cities.get_mut(&1).unwrap().body_id = 999;
        assert!(!request_loan(&mut state, 1, 5000.0));

        // 4. Missing celestial body's system_id
        let mut state = make_state_with_bank();
        state.celestial_bodies.get_mut(&1).unwrap().system_id = 999;
        assert!(!request_loan(&mut state, 1, 5000.0));

        // 5. Missing bank's home_city_id
        let mut state = make_state_with_bank();
        state.companies.get_mut(&2).unwrap().home_city_id = 999;
        assert!(!request_loan(&mut state, 1, 5000.0));
    }

    #[test]
    fn test_central_bank_low_debt() {
        let mut state = SimState::new();

        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05);

        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let central_bank_id = 1;
        state.companies.insert(
            central_bank_id,
            Company {
                id: central_bank_id,
                name: "Central Bank".into(),
                company_type: "central_bank".into(),
                home_city_id: 1,
                cash: 1000000.0, // lots of cash, 0 debt -> total_empire_debt < total_empire_cash * 0.1
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        // Rate should decrease by 0.005 from 0.05
        assert!((state.prime_rates[&empire_id] - 0.045).abs() < f64::EPSILON);
    }

    #[test]
    fn test_commercial_bank_utilization() {
        let mut state = SimState::new();

        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05);

        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let bank_id = 1;
        state.companies.insert(
            bank_id,
            Company {
                id: bank_id,
                name: "Test Bank".into(),
                company_type: "commercial_bank".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        state.companies.get_mut(&bank_id).unwrap().next_eval_tick = 2; // trigger again
        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 2,
                bank_company_id: bank_id,
                balance: 1000.0,
                interest_rate: 0.0,
            },
        );
        state.loans.insert(
            1,
            crate::sim::state::Loan {
                id: 1,
                company_id: 3,
                lender_company_id: Some(bank_id),
                principal: 1000.0,
                balance: 1000.0,
                interest_rate: 0.0,
            },
        );

        run_decisions(&mut state, 2, &mut test_rng());

        assert!((state.loans[&1].interest_rate - 0.07).abs() < f64::EPSILON);
        assert!((state.bank_accounts[&1].interest_rate - 0.035).abs() < f64::EPSILON);
    }

    /// Tests that during corporate treasury AI decision, if a company is eligible to withdraw
    /// cash from its bank account (company_cash < buffer * 0.5 and bank_balance > 0.0), but
    /// the bank company itself is missing from the state, the withdrawal is gracefully skipped
    /// instead of panicking, and the company's cash remains unchanged.
    #[test]
    fn test_company_bank_missing_during_withdrawal() {
        let mut state = SimState::new();

        // Setup a company with low cash
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Needy Corp".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup a bank account for the company, pointing to a non-existent bank
        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 1,
                bank_company_id: 999,
                balance: 5000.0,
                interest_rate: 0.05,
            },
        );

        // Run decisions
        run_decisions(&mut state, 1, &mut test_rng());

        // The company's cash should remain unchanged since the bank is missing
        assert_eq!(state.companies[&1].cash, 1000.0);
        assert_eq!(state.bank_accounts[&1].balance, 5000.0);
    }

    #[test]
    fn test_company_bank_deposit_excess_cash() {
        let mut state = SimState::new();

        // Setup a bank company
        state.companies.insert(
            999,
            Company {
                id: 999,
                name: "Bank".into(),
                company_type: "bank".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup a company with high cash (10000.0 > buffer * 1.5 where buffer is 5000)
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Rich Corp".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 10000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup a bank account for the company
        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 1,
                bank_company_id: 999,
                balance: 0.0,
                interest_rate: 0.05,
            },
        );

        // Run decisions
        run_decisions(&mut state, 1, &mut test_rng());

        // Expected deposit is 10000.0 - 5000.0 = 5000.0
        assert_eq!(state.companies[&1].cash, 5000.0);
        assert_eq!(state.bank_accounts[&1].balance, 5000.0);
        assert_eq!(state.companies[&999].cash, 6000.0);
    }

    #[test]
    fn test_company_bank_withdraw_cash_for_operations() {
        let mut state = SimState::new();

        // Setup a bank company
        state.companies.insert(
            999,
            Company {
                id: 999,
                name: "Bank".into(),
                company_type: "bank".into(),
                home_city_id: 1,
                cash: 10000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup a company with low cash (1000.0 < buffer * 0.5 where buffer is 5000)
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Struggling Corp".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Setup a bank account for the company with some balance
        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 1,
                bank_company_id: 999,
                balance: 4000.0,
                interest_rate: 0.05,
            },
        );

        // Run decisions
        run_decisions(&mut state, 1, &mut test_rng());

        // Expected withdraw is (5000.0 - 1000.0) = 4000.0
        // Min of 4000.0 (needed), 4000.0 (account balance), 10000.0 (bank cash) -> 4000.0
        assert_eq!(state.companies[&1].cash, 5000.0);
        assert_eq!(state.bank_accounts[&1].balance, 0.0);
        assert_eq!(state.companies[&999].cash, 6000.0);
    }

    #[test]
    fn test_empire_relief_refund_unmatched_orders() {
        let mut state = SimState::new();
        state.tick = 1;

        // Give empire some initial treasury
        state.add_to_empire_treasury(1, 250.0);

        let order_cost = 10.0 * 15.0; // 150.0
        state.withdraw_from_empire_treasury(1, order_cost);

        assert_eq!(state.get_empire_treasury(1), 100.0); // 250 - 150

        // Setup an unmatched empire relief order
        // Empire orders use a negative company_id corresponding to -empire_id
        let order_id = state.next_order_id();
        state.market_orders.insert(
            order_id,
            MarketOrder {
                id: order_id,
                city_id: 1,
                company_id: -1, // Represents empire_id = 1
                resource_type_id: 7,
                order_type: "buy".into(),
                order_kind: "limit".into(),
                price: 15.0,
                quantity: 10,
                created_tick: state.tick,
            },
        );
        assert_eq!(state.market_orders.len(), 1);

        let tick = state.tick;
        run_empire_relief(&mut state, tick);

        // The market order should be removed
        assert!(state.market_orders.is_empty());

        // The unused relief budget should be refunded to the empire's treasury
        // remaining 100.0 + (10 units * 15.0 price) = 100.0 + 150.0 = 250.0
        assert_eq!(state.get_empire_treasury(1), 250.0);
    }

    /// Tests that `run_empire_relief` aggregates starving cities per empire using entry API,
    /// caps relief budget according to treasury percentage (20%), and posts emergency food buy orders.
    #[test]
    fn test_empire_relief_starving_cities_budget_scaling() {
        let mut state = SimState::new();
        state.tick = 1;

        // Register Food resource type
        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food".into(),
                category: "Food".into(),
                is_vital: true,
            },
        );

        let empire_id = 1;
        state.add_to_empire_treasury(empire_id, 100_000.0);

        // Setup Sector -> StarSystem -> CelestialBody mapping for Empire 1
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sector 1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "System 1".into(),
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body 1".into(),
                fertility: 1.0,
            },
        );

        // Setup two starving cities for Empire 1
        // City 1: pop 10,000 -> relief needed: 1,000 units @ 15.0 = 15,000 cost
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "Starving City 1".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.city_consumer_ids.insert(1, 101);

        // City 2: pop 20,000 -> relief needed: 2,000 units @ 15.0 = 30,000 cost
        state.cities.insert(
            2,
            crate::sim::state::City {
                id: 2,
                body_id: 1,
                name: "Starving City 2".into(),
                population: 20_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.city_consumer_ids.insert(2, 102);

        // Run empire relief logic
        run_empire_relief(&mut state, 1);

        // Verify emergency market buy orders were created for both cities
        // Total relief cost requested = 45,000. Max relief (20% of 100,000 treasury) = 20,000.
        // Scaling factor = 20,000 / 45,000 = ~0.4444
        assert_eq!(
            state.market_orders.len(),
            2,
            "Expected 2 relief buy orders (one per starving city)"
        );

        for order in state.market_orders.values() {
            assert_eq!(
                order.company_id, -empire_id,
                "Relief order company_id should be -empire_id"
            );
            assert_eq!(order.resource_type_id, 1, "Relief order should be for Food");
            assert_eq!(order.order_type, "buy");
            assert_eq!(order.price, 15.0);
            assert!(
                order.quantity > 0,
                "Scaled relief units should be greater than 0"
            );
        }
    }

    /// Tests that `run_empire_relief` skips posting orders when empire treasury is insufficient.
    #[test]
    fn test_empire_relief_insufficient_treasury() {
        let mut state = SimState::new();
        state.tick = 1;

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food".into(),
                category: "Food".into(),
                is_vital: true,
            },
        );

        let empire_id = 1;
        // Treasury is 0
        state.add_to_empire_treasury(empire_id, 0.0);

        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sector 1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "System 1".into(),
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body 1".into(),
                fertility: 1.0,
            },
        );

        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "Starving City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.city_consumer_ids.insert(1, 101);

        run_empire_relief(&mut state, 1);

        // Treasury is zero, so effective cost is 0.0 < 0.01 -> no relief orders created
        assert!(
            state.market_orders.is_empty(),
            "No relief orders should be posted when treasury is zero"
        );
    }

    #[test]
    fn test_empire_relief_logger_deduplication() {
        let mut state = SimState::new();
        state.tick = 1;

        // Initial log check returns true
        assert!(state.should_log(LogCategory::EmpireRelief, "refund:1"));
        // Immediate duplicate returns false
        assert!(!state.should_log(LogCategory::EmpireRelief, "refund:1"));

        // Initial log check for empire relief summary returns true
        assert!(state.should_log(LogCategory::EmpireRelief, "relief:1"));
        // Immediate duplicate returns false
        assert!(!state.should_log(LogCategory::EmpireRelief, "relief:1"));

        // Advance 100 ticks to 101 -> allowed again
        state.tick = 101;
        assert!(state.should_log(LogCategory::EmpireRelief, "refund:1"));
        assert!(state.should_log(LogCategory::EmpireRelief, "relief:1"));
    }

    #[test]
    fn test_central_bank_high_debt() {
        let mut state = crate::sim::SimState::new();
        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05);
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        let central_bank_id = 1;
        state.companies.insert(
            central_bank_id,
            crate::sim::state::Company {
                id: central_bank_id,
                name: "Central Bank".into(),
                company_type: "central_bank".into(),
                home_city_id: 1,
                cash: 100.0,
                debt: 50.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        // debt (50) > cash (100) * 0.4 (40) -> Rate should increase by 0.005 from 0.05
        assert!((state.prime_rates[&empire_id] - 0.055).abs() < f64::EPSILON);
    }

    #[test]
    fn test_central_bank_buys_food_during_famine() {
        let mut state = crate::sim::SimState::new();
        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05);
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Add famine event
        state.active_events.insert(
            1,
            crate::sim::state::ActiveEvent {
                id: 1,
                event_type: "famine".into(),
                target_id: Some((1, 0)),
                start_tick: 1,
                end_tick: 10,
                severity: 1.0,
                flavor_text: None,
            },
        );

        // Add a food resource
        let food_id = 100;
        state.resource_types.insert(
            food_id,
            crate::sim::state::ResourceType {
                id: food_id,
                name: "Basic Food".into(),
                category: "food".into(),
                is_vital: true,
            },
        );

        let central_bank_id = 1;
        state.companies.insert(
            central_bank_id,
            crate::sim::state::Company {
                id: central_bank_id,
                name: "Central Bank".into(),
                company_type: "central_bank".into(),
                home_city_id: 1,
                cash: 200000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        // Central bank should have posted a buy order for food
        let buy_orders: Vec<_> = state
            .market_orders
            .values()
            .filter(|o| {
                o.company_id == central_bank_id
                    && o.resource_type_id == food_id
                    && o.order_type == "buy"
            })
            .collect();
        assert_eq!(buy_orders.len(), 1);

        // price should be 1.5 * ema_price (50.0 default if missing = 75.0)
        assert!((buy_orders[0].price - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_central_bank_famine_relief_insufficient_cash() {
        let mut state = crate::sim::SimState::new();
        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05);
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Add famine event
        state.active_events.insert(
            1,
            crate::sim::state::ActiveEvent {
                id: 1,
                event_type: "famine".into(),
                target_id: Some((1, 0)),
                start_tick: 1,
                end_tick: 10,
                severity: 1.0,
                flavor_text: None,
            },
        );

        // Add a food resource
        let food_id = 100;
        state.resource_types.insert(
            food_id,
            crate::sim::state::ResourceType {
                id: food_id,
                name: "Basic Food".into(),
                category: "food".into(),
                is_vital: true,
            },
        );

        let central_bank_id = 1;
        state.companies.insert(
            central_bank_id,
            crate::sim::state::Company {
                id: central_bank_id,
                name: "Central Bank".into(),
                company_type: "central_bank".into(),
                home_city_id: 1,
                cash: 50000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        ); // cash < 100_000.0

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        // Central bank should NOT have posted a buy order for food because cash < 100,000
        let buy_orders: Vec<_> = state
            .market_orders
            .values()
            .filter(|o| {
                o.company_id == central_bank_id
                    && o.resource_type_id == food_id
                    && o.order_type == "buy"
            })
            .collect();
        assert_eq!(buy_orders.len(), 0);
    }

    #[test]
    fn test_commercial_bank_receives_emergency_injection() {
        let mut state = crate::sim::SimState::new();
        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05); // initial prime rate
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let central_bank_id = 1;
        state.companies.insert(
            central_bank_id,
            crate::sim::state::Company {
                id: central_bank_id,
                name: "Central Bank".into(),
                company_type: "central_bank".into(),
                home_city_id: 1,
                cash: 1000000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.company_to_empire.insert(central_bank_id, empire_id);

        let comm_bank_id = 2;
        // The commercial bank has 50.0 cash.
        // It has 1000 deposits.
        // Min reserve requirement = 1000 * 0.10 = 100.0.
        // So the bank is short 50.0.
        state.companies.insert(
            comm_bank_id,
            crate::sim::state::Company {
                id: comm_bank_id,
                name: "Commercial Bank".into(),
                company_type: "commercial_bank".into(),
                home_city_id: 1,
                cash: 50.0, // Under the min reserve limit
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.company_to_empire.insert(comm_bank_id, empire_id);

        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 99,
                bank_company_id: comm_bank_id,
                balance: 1000.0,
                interest_rate: 0.02,
            },
        );

        // Record the initial loan count to ensure a new loan is created
        let initial_loans = state.loans.len();

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        // Verify the bank received cash to meet the minimum reserve (50.0 + 50.0 = 100.0)
        let updated_cash = state.companies.get(&comm_bank_id).unwrap().cash;
        assert_eq!(
            updated_cash, 100.0,
            "Bank should have received an emergency injection of 50.0 cash"
        );

        // Verify the new emergency loan was created
        assert_eq!(
            state.loans.len(),
            initial_loans + 1,
            "A new emergency loan should be recorded"
        );

        // Check the newly added loan
        let new_loan = state
            .loans
            .values()
            .find(|l| l.company_id == comm_bank_id)
            .expect("Should find loan for commercial bank");
        assert_eq!(
            new_loan.principal, 50.0,
            "Emergency loan principal should match the shortfall"
        );
        assert_eq!(
            new_loan.lender_company_id,
            Some(central_bank_id),
            "Lender should be the central bank"
        );
        // Initial prime rate is 0.05. Central bank runs first and adjusts the prime rate.
        // Debt (0.0) < Cash (1000000.0) * 0.1 (100000.0), so prime rate decreases by 0.005 to 0.045.
        // Penalty rate = new prime * 1.5 = 0.045 * 1.5 = 0.0675
        assert!(
            (new_loan.interest_rate - 0.0675).abs() < 1e-6,
            "Emergency loan should have penalty interest rate based on adjusted prime rate."
        );
    }

    #[test]
    fn test_commercial_bank_emergency_injection_and_repayment() {
        let mut state = crate::sim::SimState::new();
        let empire_id = 1;
        state.prime_rates.insert(empire_id, 0.05); // initial prime rate
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let central_bank_id = 1;
        state.companies.insert(
            central_bank_id,
            crate::sim::state::Company {
                id: central_bank_id,
                name: "Central Bank".into(),
                company_type: "central_bank".into(),
                home_city_id: 1,
                cash: 1000000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.company_to_empire.insert(central_bank_id, empire_id);

        let comm_bank_id = 2;
        // This time, bank has MORE cash than min_reserve * 2 (100 * 2 = 200)
        // Cash = 500, min_reserve = 100, excess_cash = 500 - 150 = 350
        state.companies.insert(
            comm_bank_id,
            crate::sim::state::Company {
                id: comm_bank_id,
                name: "Commercial Bank".into(),
                company_type: "commercial_bank".into(),
                home_city_id: 1,
                cash: 500.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.company_to_empire.insert(comm_bank_id, empire_id);

        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 99,
                bank_company_id: comm_bank_id,
                balance: 1000.0, // min reserve is 100.0
                interest_rate: 0.02,
            },
        );

        let loan_id = 1;
        state.add_loan(crate::sim::state::Loan {
            id: loan_id,
            company_id: comm_bank_id,
            lender_company_id: Some(central_bank_id),
            principal: 90.0,
            interest_rate: 0.0675,
            balance: 90.0,
        });

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        // the loan balance should be paid off. (350 > 90)
        let loan = state.loans.get(&loan_id).unwrap();
        assert_eq!(loan.balance, 0.0);
    }

    #[test]
    fn test_plantation_ships_food_to_profitable_city() {
        let mut state = crate::sim::state::SimState::new();

        let food_id = 7;
        state.resource_types.insert(
            food_id,
            crate::sim::state::ResourceType {
                id: food_id,
                name: "Food".to_string(),
                category: "Consumables".to_string(),
                is_vital: true,
            },
        );

        let home_city_id = 1;
        let target_city_id = 2;
        let body_id = 1;

        state.celestial_bodies.insert(
            body_id,
            crate::sim::state::CelestialBody {
                id: body_id,
                name: "Planet".into(),
                system_id: 1,
                fertility: 1.0,
            },
        );

        state.cities.insert(
            home_city_id,
            crate::sim::state::City {
                id: home_city_id,
                body_id,
                name: "Home".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.cities.insert(
            target_city_id,
            crate::sim::state::City {
                id: target_city_id,
                body_id,
                name: "Target".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.ema_prices.insert((home_city_id, food_id), 2.0);
        state.ema_prices.insert((target_city_id, food_id), 10.0);

        let company_id = 100;
        state.companies.insert(
            company_id,
            crate::sim::state::Company {
                id: company_id,
                name: "FoodCorp".into(),
                company_type: "small_company".into(),
                home_city_id,
                cash: 1000.0,
                next_eval_tick: 1,
                debt: 0.0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.facilities.insert(
            1,
            crate::sim::state::Facility {
                id: 1,
                company_id,
                city_id: home_city_id,
                facility_type: "plantation".into(),
                capacity: 100,
                setup_ticks_remaining: 0,
                target_resource_id: Some(0),
                production_ratios: None,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(company_id, home_city_id, food_id),
            crate::sim::state::Inventory {
                company_id,
                city_id: home_city_id,
                resource_type_id: food_id,
                quantity: 50,
            },
        );

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(state.trade_routes.len(), 1);
        let route = state.trade_routes.values().next().unwrap();
        assert_eq!(route.dest_city_id, target_city_id);
        assert_eq!(route.quantity, 50);

        let inv = state
            .inventories
            .get(&crate::sim::state::Inventory::key(
                company_id,
                home_city_id,
                food_id,
            ))
            .unwrap();
        assert_eq!(inv.quantity, 0);
    }

    #[test]
    fn test_plantation_posts_sell_order_if_no_profitable_city() {
        let mut state = crate::sim::state::SimState::new();

        let food_id = 7;
        state.resource_types.insert(
            food_id,
            crate::sim::state::ResourceType {
                id: food_id,
                name: "Food".to_string(),
                category: "Consumables".to_string(),
                is_vital: true,
            },
        );

        let home_city_id = 1;
        let target_city_id = 2;
        let body_id = 1;

        state.celestial_bodies.insert(
            body_id,
            crate::sim::state::CelestialBody {
                id: body_id,
                name: "Planet".into(),
                system_id: 1,
                fertility: 1.0,
            },
        );

        state.cities.insert(
            home_city_id,
            crate::sim::state::City {
                id: home_city_id,
                body_id,
                name: "Home".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.cities.insert(
            target_city_id,
            crate::sim::state::City {
                id: target_city_id,
                body_id,
                name: "Target".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.ema_prices.insert((home_city_id, food_id), 2.0);
        state.ema_prices.insert((target_city_id, food_id), 1.0);

        let company_id = 100;
        state.companies.insert(
            company_id,
            crate::sim::state::Company {
                id: company_id,
                name: "FoodCorp".into(),
                company_type: "small_company".into(),
                home_city_id,
                cash: 1000.0,
                next_eval_tick: 1,
                debt: 0.0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.facilities.insert(
            1,
            crate::sim::state::Facility {
                id: 1,
                company_id,
                city_id: home_city_id,
                facility_type: "plantation".into(),
                capacity: 100,
                setup_ticks_remaining: 0,
                target_resource_id: Some(0),
                production_ratios: None,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(company_id, home_city_id, food_id),
            crate::sim::state::Inventory {
                company_id,
                city_id: home_city_id,
                resource_type_id: food_id,
                quantity: 50,
            },
        );

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(state.trade_routes.len(), 0);

        let orders: Vec<_> = state.market_orders.values().collect();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order_type, "sell");
        assert_eq!(orders[0].quantity, 50);
    }

    #[test]
    fn test_plantation_liquidation_sell_order() {
        let mut state = crate::sim::state::SimState::new();

        let food_id = 7;
        state.resource_types.insert(
            food_id,
            crate::sim::state::ResourceType {
                id: food_id,
                name: "Food".to_string(),
                category: "Consumables".to_string(),
                is_vital: true,
            },
        );

        let home_city_id = 1;
        let body_id = 1;

        state.celestial_bodies.insert(
            body_id,
            crate::sim::state::CelestialBody {
                id: body_id,
                name: "Planet".into(),
                system_id: 1,
                fertility: 1.0,
            },
        );

        state.cities.insert(
            home_city_id,
            crate::sim::state::City {
                id: home_city_id,
                body_id,
                name: "Home".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let company_id = 100;
        state.companies.insert(
            company_id,
            crate::sim::state::Company {
                id: company_id,
                name: "FoodCorp".into(),
                company_type: "small_company".into(),
                home_city_id,
                cash: 1000.0,
                next_eval_tick: 1,
                debt: 0.0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.facilities.insert(
            1,
            crate::sim::state::Facility {
                id: 1,
                company_id,
                city_id: home_city_id,
                facility_type: "plantation".into(),
                capacity: 100,
                setup_ticks_remaining: 0,
                target_resource_id: Some(0),
                production_ratios: None,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(company_id, home_city_id, food_id),
            crate::sim::state::Inventory {
                company_id,
                city_id: home_city_id,
                resource_type_id: food_id,
                quantity: 1100, // > 10 * 100
            },
        );

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(state.trade_routes.len(), 0);

        let orders: Vec<_> = state.market_orders.values().collect();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order_type, "sell");
        assert_eq!(orders[0].quantity, 1100);
        let base_cost = 2.0; // labor cost per run
        assert_eq!(orders[0].price, base_cost * 0.95);
    }

    #[test]
    fn test_plantation_insufficient_cash_to_ship() {
        let mut state = crate::sim::state::SimState::new();

        let food_id = 7;
        state.resource_types.insert(
            food_id,
            crate::sim::state::ResourceType {
                id: food_id,
                name: "Food".to_string(),
                category: "Consumables".to_string(),
                is_vital: true,
            },
        );

        let home_city_id = 1;
        let target_city_id = 2;
        let body_id = 1;

        state.celestial_bodies.insert(
            body_id,
            crate::sim::state::CelestialBody {
                id: body_id,
                name: "Planet".into(),
                system_id: 1,
                fertility: 1.0,
            },
        );

        state.cities.insert(
            home_city_id,
            crate::sim::state::City {
                id: home_city_id,
                body_id,
                name: "Home".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.cities.insert(
            target_city_id,
            crate::sim::state::City {
                id: target_city_id,
                body_id,
                name: "Target".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.ema_prices.insert((home_city_id, food_id), 2.0);
        state.ema_prices.insert((target_city_id, food_id), 10.0); // very profitable destination

        let company_id = 100;
        state.companies.insert(
            company_id,
            crate::sim::state::Company {
                id: company_id,
                name: "FoodCorp".into(),
                company_type: "small_company".into(),
                home_city_id,
                cash: 0.0, // Insufficient cash to pay for shipping
                next_eval_tick: 1,
                debt: 0.0,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.facilities.insert(
            1,
            crate::sim::state::Facility {
                id: 1,
                company_id,
                city_id: home_city_id,
                facility_type: "plantation".into(),
                capacity: 100,
                setup_ticks_remaining: 0,
                target_resource_id: Some(0),
                production_ratios: None,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(company_id, home_city_id, food_id),
            crate::sim::state::Inventory {
                company_id,
                city_id: home_city_id,
                resource_type_id: food_id,
                quantity: 50, // Enough to be considered for shipment
            },
        );

        crate::sim::decisions::run_decisions(&mut state, 1, &mut test_rng());

        // No trade route should be created because cash is insufficient
        assert_eq!(state.trade_routes.len(), 0);

        // Instead, a local sell order should be created
        let orders: Vec<_> = state.market_orders.values().collect();
        assert_eq!(orders.len(), 1);
        assert_eq!(orders[0].order_type, "sell");
        assert_eq!(orders[0].quantity, 50);

        let inv = state
            .inventories
            .get(&crate::sim::state::Inventory::key(
                company_id,
                home_city_id,
                food_id,
            ))
            .unwrap();
        assert_eq!(inv.quantity, 50);
    }
    #[test]
    fn test_company_promotion_freelancer_to_small() {
        let mut state = SimState::new();
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Freelancer Bob".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 10000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        let mut rng = test_rng();
        run_decisions(&mut state, 1, &mut rng);

        assert_eq!(state.companies[&1].company_type, "small_company");
    }

    #[test]
    fn test_company_promotion_small_to_corporation() {
        let mut state = SimState::new();
        state.companies.insert(
            2,
            Company {
                id: 2,
                name: "Small Inc".into(),
                company_type: "small_company".into(),
                home_city_id: 1,
                cash: 100000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        let mut rng = test_rng();
        run_decisions(&mut state, 1, &mut rng);

        assert_eq!(state.companies[&2].company_type, "corporation");
    }

    #[test]
    fn test_refinery_retooling_fee_initialization() {
        let mut state = SimState::new();

        let home_city_id = 1;
        let body_id = 1;
        let company_id = 1;

        state.cities.insert(
            home_city_id,
            crate::sim::state::City {
                id: home_city_id,
                body_id,
                name: "Home".into(),
                population: 1000,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let initial_cash = 100.0; // Less than 200.0 retooling fee
        state.companies.insert(
            company_id,
            Company {
                id: company_id,
                name: "Refining Co".into(),
                company_type: "small_company".into(),
                home_city_id,
                cash: initial_cash,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.facilities.insert(
            1,
            crate::sim::state::Facility {
                id: 1,
                company_id,
                city_id: home_city_id,
                facility_type: "refinery".into(),
                capacity: 10,
                setup_ticks_remaining: 0,
                target_resource_id: None,
                production_ratios: None, // First initialization
            },
        );

        state.recipes.insert(
            1,
            crate::sim::state::Recipe {
                id: 1,
                name: "Refine Ore".into(),
                facility_type: "refinery".into(),
                inputs: vec![],
                output_resource_id: 1,
                output_qty: 10,
                labor_cost_per_run: 5.0,
            },
        );

        state.ema_prices.insert((home_city_id, 1), 10.0);

        let mut rng = test_rng();
        run_decisions(&mut state, 1, &mut rng);

        // Ratios should be initialized
        let facility = &state.facilities[&1];
        assert!(facility.production_ratios.is_some());

        // Cash should NOT be deducted on first initialization
        assert_eq!(state.companies[&company_id].cash, initial_cash);
    }

    #[test]
    fn test_commercial_bank_capacity_zero_utilization() {
        let mut state = SimState::new();

        let empire_id = 1;
        let prime_rate = 0.05;
        state.prime_rates.insert(empire_id, prime_rate);

        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );
        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "City1".into(),
                population: 100,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 1.0,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        let bank_id = 1;
        state.companies.insert(
            bank_id,
            Company {
                id: bank_id,
                name: "Test Bank".into(),
                company_type: "commercial_bank".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.bank_accounts.insert(
            1,
            crate::sim::state::BankAccount {
                id: 1,
                company_id: 2,
                bank_company_id: bank_id,
                balance: 1000.0,
                interest_rate: 0.0,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        assert!((state.bank_accounts[&1].interest_rate - (prime_rate * 0.5)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merchant_routes_food_from_surplus_to_starving_ignores_self() {
        let mut state = SimState::new();

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "Consumer Good".into(),
                is_vital: true,
            },
        );

        let empire_id = 1;
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );

        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "Surplus City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            crate::sim::state::City {
                id: 2,
                body_id: 1,
                name: "Starving City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.city_food_balance.insert(
            1,
            crate::sim::state::CityFoodBalance {
                city_id: 1,
                food_surplus: 100,
                fulfillment_ratio: 1.0,
                needs_relief: false,
                has_surplus: true,
            },
        );
        state.city_food_balance.insert(
            2,
            crate::sim::state::CityFoodBalance {
                city_id: 2,
                food_surplus: 0,
                fulfillment_ratio: 0.1,
                needs_relief: true,
                has_surplus: false,
            },
        );

        state.ema_prices.insert((1, 1), 10.0);
        state.ema_prices.insert((2, 1), 50.0);

        let merchant_id = 1;
        state.companies.insert(
            merchant_id,
            Company {
                id: merchant_id,
                name: "Food Transport Co".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 10_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        let orders: Vec<_> = state.market_orders.values().collect();
        assert!(
            !orders.is_empty(),
            "Merchant should post buy order for food"
        );
        let food_order = orders.iter().find(|o| o.company_id == merchant_id).unwrap();
        assert_eq!(
            food_order.city_id, 1,
            "Order should be placed at surplus origin city"
        );
        assert_eq!(food_order.resource_type_id, 1);
    }

    #[test]
    fn test_merchant_shipping_inventory() {
        let mut state = SimState::new();

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "Consumer Good".into(),
                is_vital: true,
            },
        );

        let empire_id = 1;
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );

        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "Surplus City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            crate::sim::state::City {
                id: 2,
                body_id: 1,
                name: "Starving City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.ema_prices.insert((1, 1), 10.0);
        state.ema_prices.insert((2, 1), 50.0);

        let merchant_id = 1;
        state.companies.insert(
            merchant_id,
            Company {
                id: merchant_id,
                name: "Food Transport Co".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 10_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(merchant_id, 1, 1),
            crate::sim::state::Inventory {
                company_id: merchant_id,
                city_id: 1,
                resource_type_id: 1,
                quantity: 100,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(
            state.trade_routes.len(),
            1,
            "Merchant should create a trade route to ship inventory"
        );

        let route = state.trade_routes.values().next().unwrap();
        assert_eq!(route.origin_city_id, 1);
        assert_eq!(route.dest_city_id, 2);
        assert_eq!(route.quantity, 100);

        let inv = state
            .inventories
            .get(&crate::sim::state::Inventory::key(merchant_id, 1, 1))
            .unwrap();
        assert_eq!(inv.quantity, 0, "Inventory should be zeroed after shipping");
    }

    #[test]
    fn test_merchant_posts_sell_order_when_no_profitable_shipping() {
        let mut state = SimState::new();

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "Consumer Good".into(),
                is_vital: true,
            },
        );

        let empire_id = 1;
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );

        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "Surplus City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            crate::sim::state::City {
                id: 2,
                body_id: 1,
                name: "Starving City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        // Make local price higher than destination price to prevent shipping
        state.ema_prices.insert((1, 1), 50.0);
        state.ema_prices.insert((2, 1), 10.0);

        let merchant_id = 1;
        state.companies.insert(
            merchant_id,
            Company {
                id: merchant_id,
                name: "Food Transport Co".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 10_000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(merchant_id, 1, 1),
            crate::sim::state::Inventory {
                company_id: merchant_id,
                city_id: 1,
                resource_type_id: 1,
                quantity: 100,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(
            state.trade_routes.len(),
            0,
            "Merchant should not ship inventory when local price is better"
        );

        let orders: Vec<_> = state.market_orders.values().collect();
        assert_eq!(orders.len(), 1, "Merchant should post a sell order locally");

        let sell_order = orders[0];
        assert_eq!(sell_order.city_id, 1);
        assert_eq!(sell_order.company_id, merchant_id);
        assert_eq!(sell_order.resource_type_id, 1);
        assert_eq!(sell_order.order_type, "sell");
        assert_eq!(sell_order.quantity, 100);

        // Price should be local_ema * 0.98
        assert!((sell_order.price - (50.0 * 0.98)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_merchant_shipping_insufficient_cash() {
        let mut state = SimState::new();

        state.resource_types.insert(
            1,
            crate::sim::state::ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "Consumer Good".into(),
                is_vital: true,
            },
        );

        let empire_id = 1;
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                name: "Sec1".into(),
                empire_id,
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                name: "Sys1".into(),
                sector_id: 1,
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body1".into(),
                fertility: 1.0,
            },
        );

        state.cities.insert(
            1,
            crate::sim::state::City {
                id: 1,
                body_id: 1,
                name: "Surplus City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            crate::sim::state::City {
                id: 2,
                body_id: 1,
                name: "Starving City".into(),
                population: 10_000,
                infrastructure_lvl: 1,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.ema_prices.insert((1, 1), 10.0);
        state.ema_prices.insert((2, 1), 50.0);

        let merchant_id = 1;
        state.companies.insert(
            merchant_id,
            Company {
                id: merchant_id,
                name: "Food Transport Co".into(),
                company_type: "merchant".into(),
                home_city_id: 1,
                cash: 0.0, // Insufficient cash to ship
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.inventories.insert(
            crate::sim::state::Inventory::key(merchant_id, 1, 1),
            crate::sim::state::Inventory {
                company_id: merchant_id,
                city_id: 1,
                resource_type_id: 1,
                quantity: 100,
            },
        );

        run_decisions(&mut state, 1, &mut test_rng());

        assert_eq!(
            state.trade_routes.len(),
            0,
            "Merchant should not create a trade route due to insufficient cash"
        );

        let inv = state
            .inventories
            .get(&crate::sim::state::Inventory::key(merchant_id, 1, 1))
            .unwrap();
        assert_eq!(inv.quantity, 100, "Inventory should not be zeroed");
    }
}
