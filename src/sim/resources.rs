use tracing::debug;

use crate::sim::state::{Inventory, SimState};

/// Phase 1: Resource extraction.
///
/// For each mine facility, extract up to `capacity` units of ore from the linked
/// deposit on the facility's planet and add them to the company's inventory.
/// Extraction cost is debited from company cash; if cash runs dry the cost rolls
/// into debt instead.
///
/// # Examples
/// ```
/// use galactic_market::sim::state::SimState;
/// use galactic_market::sim::resources::run_extraction;
/// let mut state = SimState::new();
/// run_extraction(&mut state);
/// ```
pub fn run_extraction(state: &mut SimState) {
    let mut facilities_processed = 0;
    let mut extraction_count = 0;

    let mut active_mines = Vec::new();

    for facility in state.facilities.values_mut() {
        if facility.facility_type != "mine" {
            continue;
        }
        facilities_processed += 1;

        if facility.setup_ticks_remaining > 0 {
            facility.setup_ticks_remaining -= 1;
            continue;
        }

        if let Some(target_id) = facility.target_resource_id {
            active_mines.push((
                facility.id,
                facility.city_id,
                facility.company_id,
                facility.capacity,
                target_id,
            ));
        } else {
            debug!(facility_id = facility.id, "Mine has no target_resource_id");
        }
    }

    if !active_mines.is_empty() {
        debug!(count = active_mines.len(), "Found active mines to process");
    }

    for (_facility_id, city_id, company_id, capacity, target_resource_id) in active_mines {
        debug!(city_id, company_id, "Processing miner info");
        let planet_id = match state.cities.get(&city_id) {
            Some(c) => c.body_id,
            None => {
                debug!(city_id = city_id, "City not found for facility");
                continue;
            }
        };

        let deposit = state.deposits.values_mut().find(|d| {
            d.body_id == planet_id
                && d.resource_type_id == target_resource_id
                && d.size_remaining > 0
        });

        if deposit.is_none() {
            debug!(
                city_id,
                planet_id, target_resource_id, "No eligible deposit found for miner"
            );
        }

        let deposit = match deposit {
            Some(d) => d,
            None => continue,
        };

        debug!(deposit_id = deposit.id, "Deposit found for miner");

        // Add to company inventory at its home city
        let key = Inventory::key(company_id, city_id, deposit.resource_type_id);

        // Check current inventory levels
        let current_inv = state.inventories.get(&key).map(|i| i.quantity).unwrap_or(0);

        // Stop extracting if we have a massive stockpile (e.g. 10x capacity)
        // or if the company is deep in debt (e.g. > 1000 credits).
        let company = match state.companies.get_mut(&company_id) {
            Some(c) => c,
            None => {
                debug!(company_id, "Company not found for miner");
                continue;
            }
        };

        debug!("Company found for miner");

        if company.status != "active" {
            debug!(
                company_id,
                status = company.status,
                "Skipping extraction for non-active company"
            );
            continue;
        }

        let market_price = state
            .ema_prices
            .get(&(city_id, deposit.resource_type_id))
            .copied()
            .unwrap_or(deposit.extraction_cost_per_unit * 1.5);

        // Dynamic Throttle: Stop producing if profit margins are extremely low and we have a moderate surplus.
        // Or if inventory is actually full (10x capacity).
        if market_price <= deposit.extraction_cost_per_unit * 1.05
            && current_inv > (capacity * 2) as i64
        {
            debug!(
                company_id,
                current_inv, "Throttling extraction due to near-zero profit vs surplus"
            );
            continue;
        }

        if current_inv > (capacity * 10) as i64 {
            debug!(current_inv, "Skipping extraction due to massive stockpile");
            continue;
        }

        if company.debt > 1000.0 {
            debug!(debt = company.debt, "Skipping extraction due to high debt");
            continue;
        }

        // Extract up to capacity units
        let extract_qty = capacity.min(deposit.size_remaining as i32) as i64;
        if extract_qty <= 0 {
            continue;
        }

        // Debit extraction cost from company
        let cost = extract_qty as f64 * deposit.extraction_cost_per_unit;

        if company.cash >= cost {
            company.cash -= cost;
        } else {
            // Cash deficit rolls into debt
            let shortfall = cost - company.cash;
            company.cash = 0.0;
            company.debt += shortfall;
        }

        // Deplete the deposit
        deposit.size_remaining -= extract_qty;
        extraction_count += 1;

        debug!(
            company_id,
            city_id,
            extracted = extract_qty,
            "Resource extracted"
        );

        // Add to company inventory at its home city
        let key = Inventory::key(company_id, city_id, deposit.resource_type_id);
        let entry = state.inventories.entry(key).or_insert(Inventory {
            company_id,
            city_id,
            resource_type_id: deposit.resource_type_id,
            quantity: 0,
        });
        entry.quantity += extract_qty;
    }

    if extraction_count > 0 || facilities_processed > 0 {
        debug!(
            facilities_processed,
            extraction_count, "Extraction phase complete"
        );
    }
}

/// Remove a deposit from the state if it is exhausted.
pub fn prune_exhausted_deposits(state: &mut SimState) {
    state.deposits.retain(|_, d| d.size_remaining > 0);
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, Deposit, Facility, SimState};

    fn make_state() -> SimState {
        let mut s = SimState::new();

        // One city on body 1
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

        // A deposit on body 1
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

        // A company with enough cash
        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "Test Miner".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 500.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // A mine facility with capacity 10
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

        s
    }

    #[test]
    fn extraction_depletes_deposit() {
        let mut state = make_state();
        run_extraction(&mut state);
        assert_eq!(state.deposits[&1].size_remaining, 990);
    }

    #[test]
    fn extraction_adds_to_inventory() {
        let mut state = make_state();
        run_extraction(&mut state);
        let key = Inventory::key(1, 1, 1);
        assert_eq!(state.inventories[&key].quantity, 10);
    }

    #[test]
    fn extraction_debits_cash() {
        let mut state = make_state();
        run_extraction(&mut state);
        // 10 units * 2.0 cost = 20.0 debited
        assert!((state.companies[&1].cash - 480.0).abs() < f64::EPSILON);
    }

    #[test]
    fn extraction_skips_if_inventory_too_high() {
        let mut state = make_state();
        // Set inventory to 110 (capacity 10 * 11)
        state.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: 110,
            },
        );
        run_extraction(&mut state);
        // Quantity should NOT change
        assert_eq!(state.inventories[&Inventory::key(1, 1, 1)].quantity, 110);
    }

    #[test]
    fn extraction_skips_if_debt_too_high() {
        let mut state = make_state();
        state.companies.get_mut(&1).unwrap().debt = 2000.0;
        run_extraction(&mut state);
        // Deposit should NOT change
        assert_eq!(state.deposits[&1].size_remaining, 1000);
    }
}
