use tracing::debug;

use crate::sim::state::{Inventory, Recipe, SimState};

/// Phase 2: Production / refining.
///
/// For each refinery facility, look up all applicable recipes and attempt to
/// run them. Inputs are consumed from the company's local inventory; outputs
/// are added. One recipe execution per facility per tick (limited by capacity).
///
/// # Examples
/// ```
/// use galactic_market::sim::state::SimState;
/// use galactic_market::sim::production::run_production;
/// let mut state = SimState::new();
/// run_production(&mut state); // no facilities, nothing panics
/// ```
pub fn run_production(state: &mut SimState) {
    // Collect the recipes by facility_type up front to avoid borrow conflicts
    let recipes: Vec<Recipe> = state.recipes.values().cloned().collect();

    for facility in state.facilities.values() {
        if facility.facility_type != "refinery" {
            continue;
        }

        // Find all recipes that can run in this facility type
        for recipe in recipes
            .iter()
            .filter(|r| r.facility_type == facility.facility_type)
        {
            // How many times can we run this recipe this tick? (up to capacity)
            let runs = compute_max_runs(state, facility.company_id, facility.city_id, recipe);
            let runs = runs.min(facility.capacity as i64);

            if runs == 0 {
                continue;
            }

            // Consume inputs
            for input in &recipe.inputs {
                let key = Inventory::key(facility.company_id, facility.city_id, input.resource_type_id);
                if let Some(inv) = state.inventories.get_mut(&key) {
                    inv.quantity -= input.quantity as i64 * runs;
                }
            }

            // Produce outputs
            let out_key = Inventory::key(facility.company_id, facility.city_id, recipe.output_resource_id);
            let entry = state.inventories.entry(out_key).or_insert(Inventory {
                company_id: facility.company_id,
                city_id: facility.city_id,
                resource_type_id: recipe.output_resource_id,
                quantity: 0,
            });
            entry.quantity += recipe.output_qty as i64 * runs;

            debug!(
                company_id = facility.company_id,
                recipe = %recipe.name,
                runs,
                "Production run complete"
            );
        }
    }
}

/// Returns the maximum number of times a recipe can run given current inventory.
fn compute_max_runs(state: &SimState, company_id: i32, city_id: i32, recipe: &Recipe) -> i64 {
    recipe
        .inputs
        .iter()
        .map(|input| {
            let key = Inventory::key(company_id, city_id, input.resource_type_id);
            let have = state.inventories.get(&key).map(|i| i.quantity).unwrap_or(0);
            have / input.quantity as i64
        })
        .min()
        .unwrap_or(0)
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, Facility, Inventory, Recipe, RecipeInput, SimState};

    fn make_state() -> SimState {
        let mut s = SimState::new();

        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Test City".into(),
            },
        );

        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "Test Refiner".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
            },
        );

        // Seed 30 Iron Ore in inventory
        s.inventories.insert(
            Inventory::key(1, 1, 1),
            Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1, // Iron Ore
                quantity: 30,
            },
        );

        s.facilities.insert(
            1,
            Facility {
                id: 1,
                city_id: 1,
                company_id: 1,
                facility_type: "refinery".into(),
                capacity: 5, // max 5 runs per tick
            },
        );

        // Recipe: 3 Iron Ore → 1 Iron Ingot
        s.recipes.insert(
            1,
            Recipe {
                id: 1,
                name: "Iron Ingot Smelting".into(),
                output_resource_id: 2, // Iron Ingot
                output_qty: 1,
                facility_type: "refinery".into(),
                inputs: vec![RecipeInput {
                    resource_type_id: 1,
                    quantity: 3,
                }],
            },
        );

        s
    }

    #[test]
    fn refinery_consumes_ore_and_produces_ingots() {
        let mut state = make_state();
        run_production(&mut state);

        let ore_key = Inventory::key(1, 1, 1);
        let ingot_key = Inventory::key(1, 1, 2);

        // 5 runs (capacity), 3 ore each → 15 ore consumed
        assert_eq!(state.inventories[&ore_key].quantity, 15);
        // 5 ingots produced
        assert_eq!(state.inventories[&ingot_key].quantity, 5);
    }

    #[test]
    fn refinery_limited_by_inventory() {
        let mut state = make_state();
        // Only 6 ore available → max 2 runs
        state.inventories.get_mut(&Inventory::key(1, 1, 1)).unwrap().quantity = 6;
        run_production(&mut state);

        let ingot_key = Inventory::key(1, 1, 2);
        assert_eq!(state.inventories[&ingot_key].quantity, 2);
    }
}
