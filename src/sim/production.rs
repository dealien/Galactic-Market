use tracing::debug;

use crate::sim::state::{Inventory, Recipe, SimState};

/// Phase 2: Production / refining.
///
/// Handles two types of production:
/// 1. Refineries: Transform inputs into outputs (limited by input availability)
/// 2. Plantations: Produce Food Rations based on planet fertility (renewable)
///
/// For each facility, look up applicable recipes and attempt to run them.
/// Inputs are consumed from the company's local inventory; outputs are added.
/// One recipe execution per facility per tick (limited by capacity).
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

    let mut active_refineries = Vec::new();
    let mut active_plantations = Vec::new();

    for facility in state.facilities.values_mut() {
        if facility.facility_type == "refinery" {
            if facility.setup_ticks_remaining > 0 {
                facility.setup_ticks_remaining -= 1;
                continue;
            }

            let ratios = match &facility.production_ratios {
                Some(r) => r.clone(),
                None => continue,
            };

            active_refineries.push((
                facility.id,
                facility.city_id,
                facility.company_id,
                facility.capacity,
                ratios,
            ));
        } else if facility.facility_type == "plantation" {
            if facility.setup_ticks_remaining > 0 {
                facility.setup_ticks_remaining -= 1;
                continue;
            }

            active_plantations.push((
                facility.id,
                facility.city_id,
                facility.company_id,
                facility.capacity,
            ));
        }
    }

    // Process refineries (original logic)
    for (_facility_id, city_id, company_id, capacity, ratios) in active_refineries {
        let company = match state.companies.get(&company_id) {
            Some(c) => c,
            None => continue,
        };

        if company.status != "active" {
            continue;
        }

        for (recipe_id_str, ratio) in ratios {
            let recipe_id = match recipe_id_str.parse::<i32>() {
                Ok(id) => id,
                Err(_) => continue,
            };

            let recipe = match recipes.iter().find(|r| r.id == recipe_id) {
                Some(r) => r,
                None => continue,
            };

            let allocated_capacity = (capacity as f64 * ratio).round() as i64;
            if allocated_capacity <= 0 {
                continue;
            }

            let max_runs = compute_max_runs(state, company_id, city_id, recipe);
            let runs = max_runs.min(allocated_capacity);

            if runs == 0 {
                continue;
            }

            // Consume inputs
            for input in &recipe.inputs {
                let key = Inventory::key(company_id, city_id, input.resource_type_id);
                if let Some(inv) = state.inventories.get_mut(&key) {
                    inv.quantity -= input.quantity as i64 * runs;
                }
            }

            // Produce outputs
            let out_key = Inventory::key(company_id, city_id, recipe.output_resource_id);
            let entry = state.inventories.entry(out_key).or_insert(Inventory {
                company_id,
                city_id,
                resource_type_id: recipe.output_resource_id,
                quantity: 0,
            });
            entry.quantity += recipe.output_qty as i64 * runs;

            debug!(
                company_id = company_id,
                recipe = %recipe.name,
                runs,
                "Production run complete"
            );
        }
    }

    // Process plantations (fertility-driven food production)
    for (_facility_id, city_id, company_id, capacity) in active_plantations {
        let company = match state.companies.get(&company_id) {
            Some(c) => c,
            None => continue,
        };

        if company.status != "active" {
            continue;
        }

        // Get the city and its body (planet) to access fertility
        let city = match state.cities.get(&city_id) {
            Some(c) => c,
            None => continue,
        };

        let body = match state.celestial_bodies.get(&city.body_id) {
            Some(b) => b,
            None => continue,
        };

        // Find the plantation recipe
        let plantation_recipe = recipes
            .iter()
            .find(|r| r.facility_type == "plantation")
            .cloned();

        if let Some(recipe) = plantation_recipe {
            // Production is: capacity * (1.0 + fertility_bonus)
            // fertility_bonus ranges from 0.0 (0.0x) to 3.0 (3.0x)
            let fertility_multiplier = body.fertility;
            let adjusted_capacity = (capacity as f64 * (1.0 + fertility_multiplier)).round() as i64;

            // Produce outputs
            let out_key = Inventory::key(company_id, city_id, recipe.output_resource_id);
            let entry = state.inventories.entry(out_key).or_insert(Inventory {
                company_id,
                city_id,
                resource_type_id: recipe.output_resource_id,
                quantity: 0,
            });
            entry.quantity += recipe.output_qty as i64 * adjusted_capacity;

            debug!(
                company_id = company_id,
                city_id = city_id,
                fertility = body.fertility,
                adjusted_capacity = adjusted_capacity,
                output_qty = recipe.output_qty as i64 * adjusted_capacity,
                "Plantation production complete"
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
                population: 0,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
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
                status: "active".into(),
                last_trade_tick: 0,
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
                setup_ticks_remaining: 0,
                target_resource_id: None,
                production_ratios: Some(std::collections::HashMap::from([("1".to_string(), 1.0)])),
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
        state
            .inventories
            .get_mut(&Inventory::key(1, 1, 1))
            .unwrap()
            .quantity = 6;
        run_production(&mut state);

        let ingot_key = Inventory::key(1, 1, 2);
        assert_eq!(state.inventories[&ingot_key].quantity, 2);
    }

    #[test]
    fn plantation_produces_food_at_base_fertility() {
        use crate::sim::state::CelestialBody;

        let mut state = SimState::new();

        // Set up city and planet with 1.0x fertility
        state.celestial_bodies.insert(
            1,
            CelestialBody {
                id: 1,
                system_id: 1,
                name: "Fertile Planet".into(),
                fertility: 1.0,
            },
        );

        state.cities.insert(
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

        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Farmer".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Plantation with base capacity 10
        state.facilities.insert(
            1,
            Facility {
                id: 1,
                city_id: 1,
                company_id: 1,
                facility_type: "plantation".into(),
                capacity: 10,
                setup_ticks_remaining: 0,
                target_resource_id: None,
                production_ratios: None,
            },
        );

        // Plantation recipe: produces Food Rations (resource_id 3)
        state.recipes.insert(
            1,
            Recipe {
                id: 1,
                name: "Food Ration Growth".into(),
                output_resource_id: 3,
                output_qty: 1,
                facility_type: "plantation".into(),
                inputs: vec![],
            },
        );

        run_production(&mut state);

        // At 1.0x fertility, adjusted_capacity = 10 * (1.0 + 1.0) = 20
        // output = 1 * 20 = 20 Food Rations
        let food_key = Inventory::key(1, 1, 3);
        assert_eq!(
            state
                .inventories
                .get(&food_key)
                .map(|i| i.quantity)
                .unwrap_or(0),
            20
        );
    }

    #[test]
    fn plantation_produces_more_on_high_fertility() {
        use crate::sim::state::CelestialBody;

        let mut state = SimState::new();

        // Set up city and planet with 2.0x fertility (double production)
        state.celestial_bodies.insert(
            1,
            CelestialBody {
                id: 1,
                system_id: 1,
                name: "Very Fertile Planet".into(),
                fertility: 2.0,
            },
        );

        state.cities.insert(
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

        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Farmer".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 1000.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        // Plantation with base capacity 10
        state.facilities.insert(
            1,
            Facility {
                id: 1,
                city_id: 1,
                company_id: 1,
                facility_type: "plantation".into(),
                capacity: 10,
                setup_ticks_remaining: 0,
                target_resource_id: None,
                production_ratios: None,
            },
        );

        // Plantation recipe
        state.recipes.insert(
            1,
            Recipe {
                id: 1,
                name: "Food Ration Growth".into(),
                output_resource_id: 3,
                output_qty: 1,
                facility_type: "plantation".into(),
                inputs: vec![],
            },
        );

        run_production(&mut state);

        // At 2.0x fertility, adjusted_capacity = 10 * (1.0 + 2.0) = 30
        // output = 1 * 30 = 30 Food Rations
        let food_key = Inventory::key(1, 1, 3);
        assert_eq!(
            state
                .inventories
                .get(&food_key)
                .map(|i| i.quantity)
                .unwrap_or(0),
            30
        );
    }
}
