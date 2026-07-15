use rand::SeedableRng;
use rand::rngs::StdRng;
use std::collections::HashMap;

use galactic_market::sim::SimState;
use galactic_market::sim::consumption::run_migration;
use galactic_market::sim::military::{apply_maintenance_costs, spawn_initial_units};
use galactic_market::sim::politics::run_politics;
use galactic_market::sim::production::run_production;
use galactic_market::sim::state::{
    City, CityFoodBalance, Company, DiplomaticRelation, Facility, Inventory, Recipe, RecipeInput,
    Sector, StarSystem, SystemLane, War,
};

fn setup_test_state() -> SimState {
    let mut state = SimState::new();

    // Setup two empires
    state.empires.insert(
        1,
        galactic_market::sim::state::Empire {
            id: 1,
            name: "Empire 1".to_string(),
            government_type: "Democracy".to_string(),
            tax_rate_base: 0.1,
        },
    );
    state.empires.insert(
        2,
        galactic_market::sim::state::Empire {
            id: 2,
            name: "Empire 2".to_string(),
            government_type: "Democracy".to_string(),
            tax_rate_base: 0.1,
        },
    );

    // Setup sectors
    state.sectors.insert(
        1,
        Sector {
            id: 1,
            empire_id: 1,
            name: "Sector 1".to_string(),
        },
    );
    state.sectors.insert(
        2,
        Sector {
            id: 2,
            empire_id: 2,
            name: "Sector 2".to_string(),
        },
    );

    // Setup star systems
    state.star_systems.insert(
        1,
        StarSystem {
            id: 1,
            sector_id: 1,
            name: "System 1".to_string(),
        },
    );
    state.star_systems.insert(
        2,
        StarSystem {
            id: 2,
            sector_id: 2,
            name: "System 2".to_string(),
        },
    );

    // Setup jump lane between them
    state.system_lanes.insert(
        (1, 2),
        SystemLane {
            system_a_id: 1,
            system_b_id: 2,
            distance_ly: 5.0,
            lane_type: "standard".to_string(),
        },
    );

    // Setup celestial bodies
    state.celestial_bodies.insert(
        1,
        galactic_market::sim::state::CelestialBody {
            id: 1,
            system_id: 1,
            name: "Planet 1".to_string(),
            fertility: 1.0,
        },
    );
    state.celestial_bodies.insert(
        2,
        galactic_market::sim::state::CelestialBody {
            id: 2,
            system_id: 2,
            name: "Planet 2".to_string(),
            fertility: 1.0,
        },
    );

    // Setup cities
    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "City 1".to_string(),
            population: 1000,
            infrastructure_lvl: 5,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );
    state.cities.insert(
        2,
        City {
            id: 2,
            body_id: 2,
            name: "City 2".to_string(),
            population: 1000,
            infrastructure_lvl: 5,
            port_tier: 1,
            port_fee_per_unit: 0.1,
            port_max_throughput: 1000,
        },
    );

    // Setup companies
    state.companies.insert(
        1,
        Company {
            id: 1,
            name: "Company 1".to_string(),
            company_type: "freelancer".to_string(),
            home_city_id: 1,
            cash: 10000.0,
            debt: 0.0,
            next_eval_tick: 1,
            status: "active".to_string(),
            last_trade_tick: 0,
        },
    );

    // Setup recipe and refinery
    state.recipes.insert(
        1,
        Recipe {
            id: 1,
            name: "Refining".to_string(),
            output_resource_id: 2,
            output_qty: 1,
            facility_type: "refinery".to_string(),
            inputs: vec![RecipeInput {
                resource_type_id: 1,
                quantity: 1,
            }],
            labor_cost_per_run: 0.0,
        },
    );

    state.facilities.insert(
        1,
        Facility {
            id: 1,
            city_id: 1,
            company_id: 1,
            facility_type: "refinery".to_string(),
            capacity: 10,
            setup_ticks_remaining: 0,
            target_resource_id: None,
            production_ratios: Some(HashMap::from([("1".to_string(), 1.0)])),
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

    state
}

#[test]
fn test_labor_scaling_production() {
    let mut state = setup_test_state();

    // Capacity is 10. Since 10 capacity requires 10 * 100 = 1000 population,
    // and population is 1000, labor ratio = 1.0 (100% capacity output = 10 runs).
    run_production(&mut state);
    let key = Inventory::key(1, 1, 2);
    assert_eq!(state.inventories.get(&key).unwrap().quantity, 10);

    // Reset inventory
    state.inventories.insert(
        Inventory::key(1, 1, 2),
        Inventory {
            company_id: 1,
            city_id: 1,
            resource_type_id: 2,
            quantity: 0,
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

    // Lower population to 500. Labor ratio should be 500 / 1000 = 0.5.
    // 10 capacity * 0.5 ratio = 5 runs expected.
    state.cities.get_mut(&1).unwrap().population = 500;
    run_production(&mut state);
    assert_eq!(state.inventories.get(&key).unwrap().quantity, 5);
}

#[test]
fn test_population_migration() {
    let mut state = SimState::new();

    // Setup an empire and sector
    state.sectors.insert(
        1,
        Sector {
            id: 1,
            empire_id: 1,
            name: "S1".into(),
        },
    );
    state.star_systems.insert(
        1,
        StarSystem {
            id: 1,
            sector_id: 1,
            name: "Sys1".into(),
        },
    );
    state.celestial_bodies.insert(
        1,
        galactic_market::sim::state::CelestialBody {
            id: 1,
            system_id: 1,
            name: "B1".into(),
            fertility: 1.0,
        },
    );

    // Two cities in the same empire
    state.cities.insert(
        1,
        City {
            id: 1,
            body_id: 1,
            name: "City 1".into(),
            population: 1000,
            infrastructure_lvl: 5,
            port_tier: 1,
            port_fee_per_unit: 0.0,
            port_max_throughput: 0,
        },
    );
    state.cities.insert(
        2,
        City {
            id: 2,
            body_id: 1,
            name: "City 2".into(),
            population: 1000,
            infrastructure_lvl: 5,
            port_tier: 1,
            port_fee_per_unit: 0.0,
            port_max_throughput: 0,
        },
    );

    // Set city food balances (different fulfillment ratios to trigger migration)
    state.city_food_balance.insert(
        1,
        CityFoodBalance {
            city_id: 1,
            food_surplus: -100,
            fulfillment_ratio: 0.2,
            needs_relief: true,
            has_surplus: false,
        },
    );
    state.city_food_balance.insert(
        2,
        CityFoodBalance {
            city_id: 2,
            food_surplus: 100,
            fulfillment_ratio: 1.5,
            needs_relief: false,
            has_surplus: true,
        },
    );

    // Run migration at tick 50 (migration interval)
    state.tick = 50;
    run_migration(&mut state);

    // City 1 should lose population (it has low fulfillment)
    // City 2 should gain population
    let pop_1 = state.cities.get(&1).unwrap().population;
    let pop_2 = state.cities.get(&2).unwrap().population;
    assert!(pop_1 < 1000);
    assert!(pop_2 > 1000);
    assert_eq!(pop_1 + pop_2, 2000);
}

#[test]
fn test_wartime_maintenance_multiplier() {
    let mut state = setup_test_state();

    // Spawns initial units (costs maintenance)
    spawn_initial_units(&mut state);
    state.empire_treasuries.insert(1, 1000.0);
    state.empire_treasuries.insert(2, 1000.0);

    // Apply maintenance in peacetime
    apply_maintenance_costs(&mut state);
    let peacetime_treasury_1 = state.get_empire_treasury(1);

    // Reset treasury and declare active war
    state.empire_treasuries.insert(1, 1000.0);
    state.wars.insert(
        1,
        War {
            id: 1,
            aggressor_id: 1,
            defender_id: 2,
            participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
            theaters: vec![1],
            start_tick: 1,
            end_tick: None,
            status: "active".to_string(),
            cumulative_losses: 0.0,
        },
    );

    // Apply maintenance in wartime
    apply_maintenance_costs(&mut state);
    let wartime_treasury_1 = state.get_empire_treasury(1);

    // Wartime treasury drain should be larger than peacetime
    assert!(1000.0 - wartime_treasury_1 > 1000.0 - peacetime_treasury_1);
}

#[test]
fn test_war_capitulation_threshold() {
    let mut state = setup_test_state();
    let mut rng = StdRng::seed_from_u64(42);

    // Insert active war
    state.wars.insert(
        1,
        War {
            id: 1,
            aggressor_id: 1,
            defender_id: 2,
            participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
            theaters: vec![2],
            start_tick: 1,
            end_tick: None,
            status: "active".to_string(),
            cumulative_losses: 0.0,
        },
    );

    // System 2 belongs to Empire 2 (Sector 2 is Empire 2)
    // Occupy System 2 by Empire 1
    state.occupied_systems.insert(
        2,
        galactic_market::sim::state::Occupation {
            system_id: 2,
            occupier_empire_id: 1,
            since_tick: 1,
        },
    );

    // Empire 2 only has System 2. So occupation of System 2 is 100% loss (>= 50% threshold)
    run_politics(&mut state, &mut rng);

    // War should be concluded via capitulation
    let war = state.wars.get(&1).unwrap();
    assert_eq!(war.status, "concluded");
    assert!(war.end_tick.is_some());
}

#[test]
fn test_war_infrastructure_damage_and_repair() {
    let mut state = setup_test_state();
    let mut rng = StdRng::seed_from_u64(42);

    // Spawns initial military units so they are present in border systems
    spawn_initial_units(&mut state);
    for unit in state.military_units.values_mut() {
        unit.status = "deployed".to_string();
        unit.system_id = 2;
    }

    // Set high tension to trigger war
    state.diplomatic_relations.insert(
        (1, 2),
        DiplomaticRelation {
            empire_a_id: 1,
            empire_b_id: 2,
            tension: 110.0,
            status: "neutral".to_string(),
            neutral_since_tick: 0,
        },
    );

    // Run politics to declare war and resolve combat
    run_politics(&mut state, &mut rng);

    // Let's verify war is active
    let active_war_id = state
        .wars
        .values()
        .find(|w| w.status == "active")
        .map(|w| w.id);
    assert!(active_war_id.is_some());
    let active_war_id = active_war_id.unwrap();

    // Run ticks to accumulate infrastructure damage
    for _ in 0..10 {
        state.tick += 1;
        run_politics(&mut state, &mut rng);
    }

    // At least one of the cities in the theater (City 1 or City 2) should have damaged infrastructure
    let city_1 = state.cities.get(&1).unwrap();
    let city_2 = state.cities.get(&2).unwrap();
    assert!(city_1.infrastructure_lvl < 5 || city_2.infrastructure_lvl < 5);

    // Conclude the war
    if let Some(war) = state.wars.get_mut(&active_war_id) {
        war.status = "concluded".to_string();
        war.end_tick = Some(state.tick);
    }

    // Reset relations so no new war starts
    state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 0.0;
    state.diplomatic_relations.get_mut(&(1, 2)).unwrap().status = "neutral".to_string();

    // Advance ticks to repair infrastructure
    let initial_lvl_1 = state.cities.get(&1).unwrap().infrastructure_lvl;
    let initial_lvl_2 = state.cities.get(&2).unwrap().infrastructure_lvl;

    // Peacetime repair runs every 100 ticks
    state.tick = 100;
    run_politics(&mut state, &mut rng);

    let final_lvl_1 = state.cities.get(&1).unwrap().infrastructure_lvl;
    let final_lvl_2 = state.cities.get(&2).unwrap().infrastructure_lvl;

    // Both cities should have repaired or stayed at 5
    assert!(final_lvl_1 >= initial_lvl_1);
    assert!(final_lvl_2 >= initial_lvl_2);
    if initial_lvl_1 < 5 {
        assert_eq!(final_lvl_1, initial_lvl_1 + 1);
    }
    if initial_lvl_2 < 5 {
        assert_eq!(final_lvl_2, initial_lvl_2 + 1);
    }
}
