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
    for facility in state.facilities.values() {
        // Only mine facilities participate in Phase 1
        if facility.facility_type != "mine" {
            continue;
        }

        // Find a discovered deposit on the same planet as this facility's city
        let planet_id = match state.cities.get(&facility.city_id) {
            Some(c) => c.body_id,
            None => continue,
        };

        let deposit = state
            .deposits
            .values_mut()
            .find(|d| d.body_id == planet_id && d.size_remaining > 0);

        let deposit = match deposit {
            Some(d) => d,
            None => continue,
        };

        // Extract up to capacity units
        let extract_qty = facility.capacity.min(deposit.size_remaining as i32) as i64;
        if extract_qty <= 0 {
            continue;
        }

        // Debit extraction cost from company
        let cost = extract_qty as f64 * deposit.extraction_cost_per_unit;
        let company = match state.companies.get_mut(&facility.company_id) {
            Some(c) => c,
            None => continue,
        };

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

        debug!(
            company_id = facility.company_id,
            city_id = facility.city_id,
            extracted = extract_qty,
            deposit_remaining = deposit.size_remaining,
            "Extraction complete"
        );

        // Add to company inventory at its home city
        let key = Inventory::key(facility.company_id, facility.city_id, deposit.resource_type_id);
        let entry = state.inventories.entry(key).or_insert(Inventory {
            company_id: facility.company_id,
            city_id: facility.city_id,
            resource_type_id: deposit.resource_type_id,
            quantity: 0,
        });
        entry.quantity += extract_qty;
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
    fn extraction_rolls_into_debt_when_cash_insufficient() {
        let mut state = make_state();
        state.companies.get_mut(&1).unwrap().cash = 5.0; // only enough for 2.5 units
        run_extraction(&mut state);
        // cost is 20.0; cash covers 5.0, so 15.0 goes to debt
        assert!((state.companies[&1].debt - 15.0).abs() < f64::EPSILON);
        assert_eq!(state.companies[&1].cash, 0.0);
    }
}
