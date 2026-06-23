//! Alliance (treaty) formation and dissolution.
//!
//! Empires with low tension and neutral status for a prolonged period may form
//! alliances. Alliances provide shared defense, trade bonuses, and tension
//! reduction between members. They dissolve if tension rises too high.

use rand::Rng;
use tracing::info;

use crate::sim::state::{SimState, Treaty};

/// Minimum ticks at neutral before alliance can form.
const ALLIANCE_FORMATION_COOLDOWN: u64 = 500;

/// Maximum tension for alliance formation.
const ALLIANCE_MAX_TENSION: f64 = 20.0;

/// Tension threshold that dissolves an alliance.
const ALLIANCE_DISSOLUTION_TENSION: f64 = 50.0;

/// Probability per tick that eligible empires form an alliance (1%).
const ALLIANCE_FORMATION_CHANCE: f64 = 0.01;

/// Run alliance logic: formation and dissolution checks.
pub fn run_alliances(state: &mut SimState, rng: &mut impl Rng) {
    check_alliance_formation(state, rng);
    check_alliance_dissolution(state);
}

/// Check if any eligible empire pairs should form an alliance.
fn check_alliance_formation(state: &mut SimState, rng: &mut impl Rng) {
    // Collect eligible pairs (not already allied, low tension, neutral for long enough)
    let mut eligible_pairs: Vec<(i32, i32)> = Vec::new();

    for rel in state.diplomatic_relations.values() {
        if rel.status != "neutral" {
            continue;
        }
        if rel.tension > ALLIANCE_MAX_TENSION {
            continue;
        }

        // Check they've been neutral long enough (approximate: use tick count since we
        // don't track when they became neutral, require tick > cooldown)
        if state.tick < ALLIANCE_FORMATION_COOLDOWN {
            continue;
        }

        let pair = (rel.empire_a_id, rel.empire_b_id);

        // Check they're not already in an active treaty together
        let already_allied = state.treaties.values().any(|t| {
            t.dissolved_tick.is_none()
                && t.member_empire_ids.contains(&pair.0)
                && t.member_empire_ids.contains(&pair.1)
        });

        if !already_allied {
            eligible_pairs.push(pair);
        }
    }

    for (empire_a, empire_b) in eligible_pairs {
        if rng.gen_bool(ALLIANCE_FORMATION_CHANCE) {
            // Check they're not at war with each other's allies
            if has_conflicting_alliances(state, empire_a, empire_b) {
                continue;
            }

            let name_a = state
                .empires
                .get(&empire_a)
                .map(|e| e.name.clone())
                .unwrap_or_default();
            let name_b = state
                .empires
                .get(&empire_b)
                .map(|e| e.name.clone())
                .unwrap_or_default();

            let treaty_id = state.next_treaty_id();
            let alliance_name = format!("{}-{} Accord", name_a, name_b);

            state.treaties.insert(
                treaty_id,
                Treaty {
                    id: treaty_id,
                    alliance_name: alliance_name.clone(),
                    member_empire_ids: vec![empire_a, empire_b],
                    formed_tick: state.tick,
                    dissolved_tick: None,
                },
            );

            // Update diplomatic status
            let key = if empire_a < empire_b {
                (empire_a, empire_b)
            } else {
                (empire_b, empire_a)
            };
            if let Some(rel) = state.diplomatic_relations.get_mut(&key) {
                rel.status = "alliance".to_string();
                rel.tension = 0.0;
            }

            info!(
                "ALLIANCE FORMED: {} (empires {} and {})",
                alliance_name, empire_a, empire_b
            );
        }
    }
}

/// Check if any active alliances should dissolve due to high tension.
fn check_alliance_dissolution(state: &mut SimState) {
    let mut treaties_to_dissolve: Vec<i32> = Vec::new();

    for treaty in state.treaties.values() {
        if treaty.dissolved_tick.is_some() {
            continue;
        }

        // Check tension between all member pairs
        let members = &treaty.member_empire_ids;
        let mut should_dissolve = false;

        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                let (a, b) = if members[i] < members[j] {
                    (members[i], members[j])
                } else {
                    (members[j], members[i])
                };
                if let Some(rel) = state.diplomatic_relations.get(&(a, b))
                    && rel.tension >= ALLIANCE_DISSOLUTION_TENSION
                {
                    should_dissolve = true;
                    break;
                }
            }
            if should_dissolve {
                break;
            }
        }

        if should_dissolve {
            treaties_to_dissolve.push(treaty.id);
        }
    }

    for treaty_id in treaties_to_dissolve {
        if let Some(treaty) = state.treaties.get_mut(&treaty_id) {
            treaty.dissolved_tick = Some(state.tick);
            info!("ALLIANCE DISSOLVED: {}", treaty.alliance_name);

            // Reset diplomatic status for all member pairs to neutral
            let members = treaty.member_empire_ids.clone();
            for i in 0..members.len() {
                for j in (i + 1)..members.len() {
                    let (a, b) = if members[i] < members[j] {
                        (members[i], members[j])
                    } else {
                        (members[j], members[i])
                    };
                    if let Some(rel) = state.diplomatic_relations.get_mut(&(a, b))
                        && rel.status == "alliance"
                    {
                        rel.status = "neutral".to_string();
                    }
                }
            }
        }
    }
}

/// Check if forming an alliance between two empires would conflict with
/// existing war obligations (can't be allied with both sides of a war).
fn has_conflicting_alliances(state: &SimState, empire_a: i32, empire_b: i32) -> bool {
    for war in state.wars.values() {
        if war.status != "active" {
            continue;
        }

        let a_involved = war.participants.iter().any(|(id, _)| *id == empire_a);
        let b_involved = war.participants.iter().any(|(id, _)| *id == empire_b);

        if a_involved && b_involved {
            // Check if on opposite sides
            let a_side = war
                .participants
                .iter()
                .find(|(id, _)| *id == empire_a)
                .map(|(_, role)| role.as_str());
            let b_side = war
                .participants
                .iter()
                .find(|(id, _)| *id == empire_b)
                .map(|(_, role)| role.as_str());

            let a_is_aggressor = matches!(a_side, Some("aggressor") | Some("ally"));
            let b_is_aggressor = matches!(b_side, Some("aggressor") | Some("ally"));

            if a_is_aggressor != b_is_aggressor {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{DiplomaticRelation, Empire, SimState};

    fn setup_alliance_state() -> SimState {
        let mut state = SimState::new();
        state.tick = 1000; // Past the cooldown

        state.empires.insert(
            1,
            Empire {
                id: 1,
                name: "Republic".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.15,
            },
        );
        state.empires.insert(
            2,
            Empire {
                id: 2,
                name: "Syndicate".to_string(),
                government_type: "Corporate".to_string(),
                tax_rate_base: 0.05,
            },
        );

        state.diplomatic_relations.insert(
            (1, 2),
            DiplomaticRelation {
                empire_a_id: 1,
                empire_b_id: 2,
                tension: 5.0,
                status: "neutral".to_string(),
            },
        );

        state
    }

    #[test]
    fn test_alliance_dissolution_on_high_tension() {
        let mut state = setup_alliance_state();

        // Create an active treaty
        state.treaties.insert(
            1,
            Treaty {
                id: 1,
                alliance_name: "Test Alliance".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 500,
                dissolved_tick: None,
            },
        );
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().status = "alliance".to_string();
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 60.0;

        check_alliance_dissolution(&mut state);

        assert!(state.treaties.get(&1).unwrap().dissolved_tick.is_some());
        assert_eq!(
            state.diplomatic_relations.get(&(1, 2)).unwrap().status,
            "neutral"
        );
    }

    #[test]
    fn test_alliance_not_dissolved_when_tension_low() {
        let mut state = setup_alliance_state();

        state.treaties.insert(
            1,
            Treaty {
                id: 1,
                alliance_name: "Test Alliance".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 500,
                dissolved_tick: None,
            },
        );
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().status = "alliance".to_string();
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 10.0;

        check_alliance_dissolution(&mut state);

        assert!(state.treaties.get(&1).unwrap().dissolved_tick.is_none());
    }
}
