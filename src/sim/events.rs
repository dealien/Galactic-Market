use crate::sim::state::{ActiveEvent, SimState};
use rand::Rng;
use rand::distributions::{Distribution, WeightedIndex};
use tracing::info;

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

    // 3. Process Diplomatic Tensions & War status
    process_politics(state, rng);
}

fn trigger_random_event(state: &mut SimState, rng: &mut impl Rng) {
    let weights: Vec<u32> = state.event_definitions.iter().map(|d| d.weight).collect();
    let dist = WeightedIndex::new(&weights).unwrap();
    let def = &state.event_definitions[dist.sample(rng)].clone();

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
                    let rel = state
                        .diplomatic_relations
                        .entry(key)
                        .or_insert_with(|| crate::sim::state::DiplomaticRelation {
                            empire_a_id: key.0,
                            empire_b_id: key.1,
                            tension: 0.0,
                            status: "neutral".to_string(),
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

fn process_politics(state: &mut SimState, _rng: &mut impl Rng) {
    for rel in state.diplomatic_relations.values_mut() {
        // Natural tension decay
        if rel.status == "neutral" {
            rel.tension = (rel.tension - 0.01).max(0.0);
        }

        // Trigger War if tension too high
        if rel.status == "neutral" && rel.tension >= 100.0 {
            rel.status = "war".to_string();
            let name_a = &state.empires[&rel.empire_a_id].name;
            let name_b = &state.empires[&rel.empire_b_id].name;
            info!("WAR DECLARED between {} and {}!", name_a, name_b);

            // Create a war event
            let event = ActiveEvent {
                id: state.next_event_id,
                event_type: "war".to_string(),
                target_id: None, // Targets both empires
                severity: 1.0,
                start_tick: state.tick,
                end_tick: state.tick + 1000, // Long duration
                flavor_text: Some(format!("The {} and {} are at open war!", name_a, name_b)),
            };
            state.next_event_id += 1;
            state.active_events.insert(event.id, event);
        }
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
    use super::*;
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
}
