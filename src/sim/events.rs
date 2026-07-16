//! Random event generation and lifecycle management.
//!
//! Periodic events (such as solar flares, blockades, or market shocks) are
//! triggered and updated.

use crate::db::seed::DIPLOMATIC_STATUS_NEUTRAL;
use crate::sim::state::{ActiveEvent, SimState};
use rand::Rng;
use rand::distributions::{Distribution, WeightedIndex};
use tracing::info;

/// Phase 9: Random Events.
///
/// Advance lifetimes of active events and roll to trigger new random events
/// based on configured event weights.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::events::run_events;
/// use rand::thread_rng;
///
/// let mut state = SimState::new();
/// let mut rng = thread_rng();
/// run_events(&mut state, &mut rng);
/// ```
pub fn run_events(state: &mut SimState, rng: &mut impl Rng) {
    // 1. Expire old events — increment blockade_version if any blockade expires.
    let mut blockade_expired = false;
    state.active_events.retain(|_, event| {
        if state.tick > event.end_tick {
            info!("Event expired: {:?}", event.event_type);
            if event.event_type == "blockade_lane" {
                blockade_expired = true;
            }
            false
        } else {
            true
        }
    });
    if blockade_expired {
        state.blockade_version += 1;
    }

    // 2. Chance to trigger a new random event
    // Base 5% chance per tick to trigger an event if we have definitions
    if !state.event_definitions.is_empty() && rng.gen_bool(0.05) {
        trigger_random_event(state, rng);
    }

    // Note: Politics (tension, war, alliances) is now handled by the dedicated
    // politics phase in src/sim/politics.rs.
}

fn trigger_random_event(state: &mut SimState, rng: &mut impl Rng) {
    let valid_defs: Vec<_> = state
        .event_definitions
        .iter()
        .filter(|d| d.weight > 0)
        .collect();

    if valid_defs.is_empty() {
        info!("Skipping random event: no event definitions with positive weight");
        return;
    }

    let weights: Vec<u32> = valid_defs.iter().map(|d| d.weight).collect();
    let dist = match WeightedIndex::new(&weights) {
        Ok(d) => d,
        Err(err) => {
            info!(
                "Skipping random event due to invalid event weights: {}",
                err
            );
            return;
        }
    };
    let def = valid_defs[dist.sample(rng)].clone();

    let severity = rng.gen_range(def.severity_range[0]..=def.severity_range[1]);

    for effect_def in &def.effects {
        let duration = rng.gen_range(effect_def.duration_range[0]..=effect_def.duration_range[1]);

        let mut event = ActiveEvent {
            id: state.next_event_id,
            event_type: effect_def.effect_type.clone(),
            target_id: None,
            severity,
            start_tick: state.tick,
            end_tick: state.tick + duration,
            flavor_text: None,
        };
        state.next_event_id += 1;

        // Determine target and flavor text based on effect type
        match effect_def.effect_type.as_str() {
            "blockade_lane" => {
                if let Some(&(sys_a, sys_b)) = pick_random_lane(state, rng) {
                    event.target_id = Some((sys_a, sys_b));
                    let name_a = &state.star_systems[&sys_a].name;
                    let name_b = &state.star_systems[&sys_b].name;
                    event.flavor_text = Some(
                        def.flavor_text
                            .replace("{system_a}", name_a)
                            .replace("{system_b}", name_b),
                    );
                }
            }
            "infrastructure_damage" | "famine" => {
                if let Some(&city_id) = pick_random_city(state, rng) {
                    event.target_id = Some((city_id, 0));
                    let city_name = &state.cities[&city_id].name;
                    event.flavor_text = Some(def.flavor_text.replace("{city_name}", city_name));
                }
            }
            "tension_increase" => {
                if let Some((emp_a, emp_b)) = pick_random_empire_pair(state, rng) {
                    event.target_id = None; // Effect applied immediately to relations
                    let name_a = &state.empires[&emp_a].name;
                    let name_b = &state.empires[&emp_b].name;
                    event.flavor_text = Some(
                        def.flavor_text
                            .replace("{empire_a}", name_a)
                            .replace("{empire_b}", name_b),
                    );

                    // Apply tension increase immediately; create the relation if it doesn't exist.
                    let key = if emp_a < emp_b {
                        (emp_a, emp_b)
                    } else {
                        (emp_b, emp_a)
                    };
                    let rel = state.diplomatic_relations.entry(key).or_insert_with(|| {
                        crate::sim::state::DiplomaticRelation {
                            empire_a_id: key.0,
                            empire_b_id: key.1,
                            tension: 0.0,
                            status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                            neutral_since_tick: state.tick,
                        }
                    });
                    rel.tension += 10.0 * severity;
                    info!(
                        "Tension increased between {} and {}: {:.1}",
                        name_a, name_b, rel.tension
                    );
                }
            }
            _ => {}
        }

        if let Some(text) = &event.flavor_text {
            info!("EVENT: {}", text);
        }

        // Increment blockade_version when a new blockade is added so logistics
        // knows to recompute system distances.
        if event.event_type == "blockade_lane" {
            state.blockade_version += 1;
        }

        state.active_events.insert(event.id, event);
    }
}

fn pick_random_lane<'a>(state: &'a SimState, rng: &mut impl Rng) -> Option<&'a (i32, i32)> {
    let keys: Vec<_> = state.system_lanes.keys().collect();
    if keys.is_empty() {
        return None;
    }
    Some(keys[rng.gen_range(0..keys.len())])
}

fn pick_random_city<'a>(state: &'a SimState, rng: &mut impl Rng) -> Option<&'a i32> {
    let keys: Vec<_> = state.cities.keys().collect();
    if keys.is_empty() {
        return None;
    }
    Some(keys[rng.gen_range(0..keys.len())])
}

fn pick_random_empire_pair(state: &SimState, rng: &mut impl Rng) -> Option<(i32, i32)> {
    let keys: Vec<_> = state.empires.keys().cloned().collect();
    if keys.len() < 2 {
        return None;
    }
    let a = keys[rng.gen_range(0..keys.len())];
    let mut b = keys[rng.gen_range(0..keys.len())];
    while a == b {
        b = keys[rng.gen_range(0..keys.len())];
    }
    Some((a, b))
}

#[cfg(test)]
mod tests {
    use crate::sim::state::{SimState, StarSystem};

    #[test]
    fn test_lane_blockade_no_collision() {
        let mut state = SimState::new();

        // Create 4 star systems
        for i in 1..=4 {
            state.star_systems.insert(
                i,
                StarSystem {
                    id: i,
                    sector_id: 1,
                    name: format!("System-{}", i),
                },
            );
        }

        // Create two lanes: (1,2) and (3,4)
        state.system_lanes.insert(
            (1, 2),
            crate::sim::state::SystemLane {
                system_a_id: 1,
                system_b_id: 2,
                distance_ly: 10.0,
                lane_type: "standard".to_string(),
            },
        );

        state.system_lanes.insert(
            (3, 4),
            crate::sim::state::SystemLane {
                system_a_id: 3,
                system_b_id: 4,
                distance_ly: 10.0,
                lane_type: "standard".to_string(),
            },
        );

        // Create blockade events for both lanes
        let event1 = crate::sim::state::ActiveEvent {
            id: 1,
            event_type: "blockade_lane".to_string(),
            target_id: Some((1, 2)),
            severity: 1.0,
            start_tick: 0,
            end_tick: 100,
            flavor_text: None,
        };

        let event2 = crate::sim::state::ActiveEvent {
            id: 2,
            event_type: "blockade_lane".to_string(),
            target_id: Some((3, 4)),
            severity: 1.0,
            start_tick: 0,
            end_tick: 100,
            flavor_text: None,
        };

        state.active_events.insert(1, event1);
        state.active_events.insert(2, event2);

        // Build distances; this should recognize both blockades as separate lanes
        crate::sim::logistics::build_system_distances(&mut state);

        // Verify blockades are correctly identified as separate events
        assert_eq!(
            state.active_events.len(),
            2,
            "Both blockade events should exist"
        );
        let blockade_targets: Vec<_> = state
            .active_events
            .values()
            .filter(|e| e.event_type == "blockade_lane")
            .map(|e| e.target_id)
            .collect();
        assert_eq!(blockade_targets.len(), 2);
        assert!(blockade_targets.contains(&Some((1, 2))));
        assert!(blockade_targets.contains(&Some((3, 4))));
    }
    #[test]
    fn test_expire_events() {
        let mut state = SimState::new();
        state.tick = 100;
        state.blockade_version = 0;

        // Add an expired event
        let expired_event = crate::sim::state::ActiveEvent {
            id: 1,
            event_type: "some_event".to_string(),
            target_id: None,
            severity: 1.0,
            start_tick: 0,
            end_tick: 99,
            flavor_text: None,
        };
        state.active_events.insert(1, expired_event);

        // Add an active event
        let active_event = crate::sim::state::ActiveEvent {
            id: 2,
            event_type: "other_event".to_string(),
            target_id: None,
            severity: 1.0,
            start_tick: 0,
            end_tick: 101,
            flavor_text: None,
        };
        state.active_events.insert(2, active_event);

        // Add an expired blockade
        let expired_blockade = crate::sim::state::ActiveEvent {
            id: 3,
            event_type: "blockade_lane".to_string(),
            target_id: None,
            severity: 1.0,
            start_tick: 0,
            end_tick: 99,
            flavor_text: None,
        };
        state.active_events.insert(3, expired_blockade);

        // Add an active blockade
        let active_blockade = crate::sim::state::ActiveEvent {
            id: 4,
            event_type: "blockade_lane".to_string(),
            target_id: None,
            severity: 1.0,
            start_tick: 0,
            end_tick: 101,
            flavor_text: None,
        };
        state.active_events.insert(4, active_blockade);

        let mut rng = rand::thread_rng();
        super::run_events(&mut state, &mut rng);

        assert_eq!(
            state.active_events.len(),
            2,
            "Only active events should remain"
        );
        assert!(
            state.active_events.contains_key(&2),
            "active_event should remain"
        );
        assert!(
            state.active_events.contains_key(&4),
            "active_blockade should remain"
        );

        assert_eq!(
            state.blockade_version, 1,
            "Blockade version should increment when a blockade expires"
        );
    }

    #[test]
    fn test_trigger_infrastructure_damage_event() {
        let mut state = SimState::new();
        state.tick = 10;
        state.next_event_id = 1;

        // Create a city so pick_random_city can find it
        let city = crate::sim::state::City {
            id: 1,
            name: "Test City".to_string(),
            population: 1000,
            body_id: 1,
            infrastructure_lvl: 3,
            port_tier: 1,
            port_fee_per_unit: 1.0,
            port_max_throughput: 1000,
        };
        state.cities.insert(1, city);

        let effect_def = crate::sim::state::EventEffectDefinition {
            effect_type: "infrastructure_damage".to_string(),
            duration_range: [10, 10],
        };

        let def = crate::sim::state::EventDefinition {
            id: "test_damage".to_string(),
            weight: 100,
            severity_range: [1.0, 1.0],
            effects: vec![effect_def],
            flavor_text: "Damage to {city_name}".to_string(),
        };

        state.event_definitions.push(def);

        let mut rng = rand::thread_rng();
        super::trigger_random_event(&mut state, &mut rng);

        assert_eq!(state.active_events.len(), 1, "Event should be created");
        let event = state.active_events.get(&1).unwrap();
        assert_eq!(event.event_type, "infrastructure_damage");
        assert_eq!(event.target_id, Some((1, 0)));
        assert_eq!(event.flavor_text.as_deref(), Some("Damage to Test City"));
    }

    #[test]
    fn test_trigger_tension_increase_event() {
        let mut state = SimState::new();
        state.tick = 10;
        state.next_event_id = 1;

        // Create two empires so pick_random_empire_pair can find them
        let emp1 = crate::sim::state::Empire {
            id: 1,
            name: "Empire Alpha".to_string(),
            government_type: "Democracy".to_string(),
            tax_rate_base: 0.1,
        };
        let emp2 = crate::sim::state::Empire {
            id: 2,
            name: "Empire Beta".to_string(),
            government_type: "Dictatorship".to_string(),
            tax_rate_base: 0.1,
        };
        state.empires.insert(1, emp1);
        state.empires.insert(2, emp2);

        let effect_def = crate::sim::state::EventEffectDefinition {
            effect_type: "tension_increase".to_string(),
            duration_range: [0, 0],
        };

        let def = crate::sim::state::EventDefinition {
            id: "test_tension".to_string(),
            weight: 100,
            severity_range: [1.0, 1.0],
            effects: vec![effect_def],
            flavor_text: "Tension between {empire_a} and {empire_b}".to_string(),
        };

        state.event_definitions.push(def);

        let mut rng = rand::thread_rng();
        super::trigger_random_event(&mut state, &mut rng);

        assert_eq!(state.active_events.len(), 1, "Event should be created");
        let event = state.active_events.get(&1).unwrap();
        assert_eq!(event.event_type, "tension_increase");
        assert_eq!(event.target_id, None);
        // It could be Alpha and Beta or Beta and Alpha
        let flavor = event.flavor_text.as_deref().unwrap();
        assert!(
            flavor == "Tension between Empire Alpha and Empire Beta"
                || flavor == "Tension between Empire Beta and Empire Alpha"
        );

        let key = (1, 2);
        assert!(state.diplomatic_relations.contains_key(&key));
        let rel = state.diplomatic_relations.get(&key).unwrap();
        assert_eq!(
            rel.tension, 10.0,
            "Tension should increase by 10.0 * severity"
        );
    }
}
