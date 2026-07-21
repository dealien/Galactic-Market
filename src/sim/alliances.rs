//! Alliance (treaty) formation and dissolution.
//!
//! Empires with low tension and neutral status for a prolonged period may form
//! alliances. Alliances provide shared defense, trade bonuses, and tension
//! reduction between members. They dissolve if tension rises too high.

use rand::Rng;
use tracing::info;

use crate::db::seed::{DIPLOMATIC_STATUS_ALLIANCE, DIPLOMATIC_STATUS_NEUTRAL};
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
///
/// # Examples
///
/// ```
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::alliances::run_alliances;
/// use rand::thread_rng;
///
/// let mut state = SimState::new();
/// let mut rng = thread_rng();
///
/// // Run the alliance check phase
/// run_alliances(&mut state, &mut rng);
/// ```
pub fn run_alliances(state: &mut SimState, rng: &mut impl Rng) {
    check_alliance_formation(state, rng);
    check_alliance_dissolution(state);
}

/// Check if any eligible empire pairs should form an alliance.
fn check_alliance_formation(state: &mut SimState, rng: &mut impl Rng) {
    // Collect eligible pairs (not already allied, low tension, neutral for long enough)
    let mut eligible_pairs: Vec<(i32, i32)> = Vec::new();

    for rel in state.diplomatic_relations.values() {
        if rel.status != DIPLOMATIC_STATUS_NEUTRAL {
            continue;
        }
        if rel.tension > ALLIANCE_MAX_TENSION {
            continue;
        }

        if state.tick.saturating_sub(rel.neutral_since_tick) < ALLIANCE_FORMATION_COOLDOWN {
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
                rel.status = DIPLOMATIC_STATUS_ALLIANCE.to_string();
                rel.neutral_since_tick = state.tick;
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
                        && rel.status == DIPLOMATIC_STATUS_ALLIANCE
                    {
                        rel.status = DIPLOMATIC_STATUS_NEUTRAL.to_string();
                        rel.neutral_since_tick = state.tick;
                    }
                }
            }
        }
    }
}

/// Check if forming an alliance between two empires would conflict with
/// existing war obligations (can't be allied with both sides of a war).
fn has_conflicting_alliances(state: &SimState, empire_a: i32, empire_b: i32) -> bool {
    let participant_side = |role: &str| -> Option<bool> {
        match role {
            "aggressor" | "aggressor_ally" => Some(true),
            "defender" | "defender_ally" => Some(false),
            // Legacy role from older persisted data has no side information.
            "ally" => None,
            _ => None,
        }
    };

    for war in state.wars.values() {
        if war.status != "active" {
            continue;
        }

        let a_side = war
            .participants
            .iter()
            .find(|(id, _)| *id == empire_a)
            .and_then(|(_, role)| participant_side(role));
        let b_side = war
            .participants
            .iter()
            .find(|(id, _)| *id == empire_b)
            .and_then(|(_, role)| participant_side(role));

        if a_side.is_some() && b_side.is_some() {
            // If on opposite sides, it's a conflict.
            if a_side != b_side {
                return true;
            }
        } else if war.participants.iter().any(|(id, _)| *id == empire_a)
            && war.participants.iter().any(|(id, _)| *id == empire_b)
        {
            // If both are in an active war but either side is unknown (legacy "ally" role),
            // reject alliance formation conservatively.
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{DiplomaticRelation, Empire, SimState, War};
    use rand::RngCore;
    use rand::SeedableRng;

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
                tax_rate: 0.05,
            },
        );
        state.empires.insert(
            2,
            Empire {
                id: 2,
                name: "Syndicate".to_string(),
                government_type: "Corporate".to_string(),
                tax_rate_base: 0.05,
                tax_rate: 0.05,
            },
        );

        state.diplomatic_relations.insert(
            (1, 2),
            DiplomaticRelation {
                empire_a_id: 1,
                empire_b_id: 2,
                tension: 5.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                neutral_since_tick: 0,
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
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().status =
            DIPLOMATIC_STATUS_ALLIANCE.to_string();
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 60.0;

        check_alliance_dissolution(&mut state);

        assert!(state.treaties.get(&1).unwrap().dissolved_tick.is_some());
        assert_eq!(
            state.diplomatic_relations.get(&(1, 2)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
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
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().status =
            DIPLOMATIC_STATUS_ALLIANCE.to_string();
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 10.0;

        check_alliance_dissolution(&mut state);

        assert!(state.treaties.get(&1).unwrap().dissolved_tick.is_none());
    }

    #[test]
    fn test_conflicting_alliances_detect_opposite_war_sides() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![
                    (1, "aggressor".to_string()),
                    (2, "defender".to_string()),
                    (3, "aggressor_ally".to_string()),
                    (4, "defender_ally".to_string()),
                ],
                theaters: vec![],
                start_tick: 1,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        assert!(has_conflicting_alliances(&state, 3, 4));
        assert!(!has_conflicting_alliances(&state, 1, 3));
    }

    #[test]
    fn test_conflicting_alliances_treat_legacy_ally_role_as_conflict() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![
                    (1, "aggressor".to_string()),
                    (2, "defender".to_string()),
                    (3, "ally".to_string()),
                ],
                theaters: vec![],
                start_tick: 1,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        assert!(has_conflicting_alliances(&state, 2, 3));
    }

    #[test]
    fn test_alliance_requires_neutral_cooldown_per_relation() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN + 1;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = state.tick - 10;

        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        check_alliance_formation(&mut state, &mut rng);

        assert!(state.treaties.is_empty());
    }

    #[test]
    fn test_alliance_forms_after_neutral_cooldown_per_relation() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;

        struct AlwaysFormRng;
        impl RngCore for AlwaysFormRng {
            fn next_u32(&mut self) -> u32 {
                0
            }

            fn next_u64(&mut self) -> u64 {
                0
            }

            fn fill_bytes(&mut self, dest: &mut [u8]) {
                for byte in dest {
                    *byte = 0;
                }
            }

            fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
                self.fill_bytes(dest);
                Ok(())
            }
        }

        let mut rng = AlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(state.treaties.len(), 1);
    }

    #[test]
    fn test_alliance_formation_skips_non_neutral() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;
        rel.status = "war".to_string(); // Not neutral

        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        check_alliance_formation(&mut state, &mut rng);

        assert!(
            state.treaties.is_empty(),
            "Should not form alliance if not neutral"
        );
    }

    #[test]
    fn test_alliance_formation_skips_high_tension() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;
        rel.tension = ALLIANCE_MAX_TENSION + 5.0; // High tension

        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        check_alliance_formation(&mut state, &mut rng);

        assert!(
            state.treaties.is_empty(),
            "Should not form alliance if tension is too high"
        );
    }

    #[test]
    fn test_alliance_formation_skips_already_allied() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;

        // Add an active treaty for the pair
        state.treaties.insert(
            1,
            Treaty {
                id: 1,
                alliance_name: "Existing Alliance".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 0,
                dissolved_tick: None,
            },
        );

        let mut rng = rand::rngs::StdRng::seed_from_u64(0);
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(
            state.treaties.len(),
            1,
            "Should not form a second alliance if already allied"
        );
    }

    #[test]
    fn test_has_conflicting_alliances_ignores_inactive_wars() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![
                    (1, "aggressor".to_string()),
                    (2, "defender".to_string()),
                    (3, "aggressor_ally".to_string()),
                    (4, "defender_ally".to_string()),
                ],
                theaters: vec![],
                start_tick: 1,
                end_tick: Some(10), // War has ended
                status: "concluded".to_string(),
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        assert!(
            !has_conflicting_alliances(&state, 3, 4),
            "Should ignore concluded wars"
        );
    }

    #[test]
    fn test_has_conflicting_alliances_unrecognized_role() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![
                    (1, "aggressor".to_string()),
                    (2, "defender".to_string()),
                    (3, "unknown_role".to_string()), // Should match `_` and return None
                    (4, "unknown_role_2".to_string()),
                ],
                theaters: vec![],
                start_tick: 1,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        // Since both have unknown roles, `participant_side` returns None.
        // It falls through to the legacy conservative check. Since both 3 and 4 are in the war,
        // it rejects the alliance conservatively.
        assert!(
            has_conflicting_alliances(&state, 3, 4),
            "Should conservatively reject alliance for unknown roles in same war"
        );
    }
}
