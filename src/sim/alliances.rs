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
    fn test_dissolution_ordering_and_missing_relations() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        state.empires.insert(
            3,
            crate::sim::state::Empire {
                id: 3,
                name: "Empire C".to_string(),
                government_type: "republic".to_string(),
                tax_rate_base: 0.1,
                tax_rate: 0.1,
            },
        );
        state.diplomatic_relations.insert(
            (2, 3),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 2,
                empire_b_id: 3,
                tension: 80.0,
                status: DIPLOMATIC_STATUS_ALLIANCE.to_string(),
                neutral_since_tick: 0,
            },
        );

        state.treaties.insert(
            100,
            Treaty {
                id: 100,
                alliance_name: "Test Alliance".to_string(),
                member_empire_ids: vec![3, 2],
                formed_tick: 0,
                dissolved_tick: None,
            },
        );

        check_alliance_dissolution(&mut state);

        // Assert that alliance dissolved
        assert!(state.treaties.get(&100).unwrap().dissolved_tick.is_some());
        assert_eq!(
            state.diplomatic_relations.get(&(2, 3)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
        );
    }
    #[test]
    fn test_alliance_ordering_and_missing_relations() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        // Add a relationship where empire_a > empire_b by using different IDs, e.g., 3 and 2.
        state.empires.insert(
            3,
            crate::sim::state::Empire {
                id: 3,
                name: "Empire C".to_string(),
                government_type: "republic".to_string(),
                tax_rate_base: 0.1,
                tax_rate: 0.1,
            },
        );
        state.diplomatic_relations.insert(
            (2, 3),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 2,
                empire_b_id: 3,
                tension: 0.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                neutral_since_tick: 0,
            },
        );

        // Remove name from empire 3 to hit the unwrap_or_default() branch

        state.empires.remove(&3);

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        // Assert that an alliance formed with the removed empire!
        assert_eq!(state.treaties.len(), 2);
    }

    /// Tests that the check_alliance_formation fallback logic works correctly for handling missing empire records.
    /// This targets the fallback logic for `.unwrap_or_default()` when evaluating empire names.
    #[test]
    fn test_alliance_ordering_and_missing_relations_coverage() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        // Make an empire relation but delete the source empire to trigger unwrap_or_default() fallback
        state.empires.remove(&1);

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(state.treaties.len(), 1);
        let first_treaty = state.treaties.values().next().unwrap();
        // Since we removed empire 1, name_a defaults to empty string, leading to "-Empire B Accord"
        assert_eq!(first_treaty.alliance_name, "-Syndicate Accord");
    }

    #[test]
    fn test_dissolution_missing_relations_coverage() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        // Force a multi-member treaty where a relation is missing
        state.treaties.insert(
            101,
            crate::sim::state::Treaty {
                id: 101,
                alliance_name: "Test Accord".to_string(),
                member_empire_ids: vec![1, 2, 3],
                formed_tick: 1,
                dissolved_tick: None,
            },
        );

        // Intentionally do NOT insert the relation between 2 and 3.
        // It will trigger the missing `let Some(rel)` case when evaluating should_dissolve.
        // It should also hit the nested loop block when resetting diplomatic statuses.
        check_alliance_dissolution(&mut state);

        assert!(state.treaties.get(&101).unwrap().dissolved_tick.is_none());
    }

    #[test]
    fn test_dissolution_resets_diplomatic_status_nested_loop_coverage() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        state.treaties.insert(
            102,
            crate::sim::state::Treaty {
                id: 102,
                alliance_name: "Test Accord 2".to_string(),
                member_empire_ids: vec![1, 2, 3],
                formed_tick: 1,
                dissolved_tick: None,
            },
        );

        state.diplomatic_relations.insert(
            (2, 3),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 2,
                empire_b_id: 3,
                tension: ALLIANCE_DISSOLUTION_TENSION + 10.0,
                status: DIPLOMATIC_STATUS_ALLIANCE.to_string(),
                neutral_since_tick: 0,
            },
        );
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().status =
            DIPLOMATIC_STATUS_ALLIANCE.to_string();

        check_alliance_dissolution(&mut state);

        // The high tension between 2 and 3 dissolves the treaty.
        assert!(state.treaties.get(&102).unwrap().dissolved_tick.is_some());

        // This verifies the reset logic works correctly across multiple members
        assert_eq!(
            state.diplomatic_relations.get(&(1, 2)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
        );
        assert_eq!(
            state.diplomatic_relations.get(&(2, 3)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
        );
    }

    #[test]
    fn test_alliance_formation_tension_too_high() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;
        rel.tension = ALLIANCE_MAX_TENSION + 1.0;

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert!(
            state.treaties.is_empty(),
            "High tension should prevent alliance formation"
        );
    }

    #[test]
    fn test_alliance_formation_status_not_neutral() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;
        rel.status = "hostile".to_string();

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert!(
            state.treaties.is_empty(),
            "Non-neutral status should prevent alliance formation"
        );
    }

    #[test]
    fn test_alliance_formation_already_allied() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;

        state.treaties.insert(
            103,
            crate::sim::state::Treaty {
                id: 103,
                alliance_name: "Test Accord 3".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 1,
                dissolved_tick: None,
            },
        );

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(
            state.treaties.len(),
            1,
            "Should not form duplicate alliance"
        );
    }

    #[test]
    fn test_alliance_formation_missing_relation_key() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        // Remove the existing relation (1, 2)
        state.diplomatic_relations.remove(&(1, 2));

        // Add a relation (2, 1) where empire_a > empire_b to test the key swapping logic
        // Because eligible_pairs extracts (empire_a, empire_b) = (2, 1)
        // Then line 110-112 swaps it to (1, 2)
        // Then it tries to get_mut(&(1, 2)), which we deleted.
        // This hits the missing branch `if let Some(rel)`!

        state.diplomatic_relations.insert(
            (2, 1),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 2,
                empire_b_id: 1,
                tension: 0.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                neutral_since_tick: 0,
            },
        );

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(state.treaties.len(), 1, "Should form an alliance");
        // The relation status for (2, 1) should remain neutral because it tried to update (1, 2)
        assert_eq!(
            state.diplomatic_relations.get(&(2, 1)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
        );
    }

    #[test]
    fn test_alliance_formation_opposite_key_ordering() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        // Setup diplomatic relation for (2, 3) and (3, 2) properly
        state.empires.insert(
            3,
            crate::sim::state::Empire {
                id: 3,
                name: "Empire C".to_string(),
                government_type: "republic".to_string(),
                tax_rate_base: 0.1,
                tax_rate: 0.1,
            },
        );

        state.diplomatic_relations.insert(
            (2, 3),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 2,
                empire_b_id: 3,
                tension: 0.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                neutral_since_tick: 0,
            },
        );

        // Force a relation key the other way around. Wait, `state.diplomatic_relations` has tuples as keys, which implies we only need the proper order because of line 109.
        // If we add `(3, 2)` to eligible_pairs, it'll swap it to `(2, 3)` before accessing `diplomatic_relations`.
        // To do this we have to bypass `eligible_pairs` gathering in `run_alliances` or just mock the input. But we can't because `eligible_pairs` is built by iterating `state.diplomatic_relations.values()`.
        // If we insert the key as `(3, 2)`, then `rel.empire_a_id` = 3, `rel.empire_b_id` = 2.
        state.diplomatic_relations.remove(&(1, 2));
        state.diplomatic_relations.remove(&(2, 3));

        state.diplomatic_relations.insert(
            (3, 2),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 3,
                empire_b_id: 2,
                tension: 0.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                neutral_since_tick: 0,
            },
        );
        // Then when updating, it'll sort the key to (2, 3) which doesn't exist! So the if let Some(rel) evaluates to false. This hits the missing branch.

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(state.treaties.len(), 1, "Should form an alliance");
        // Status of (3, 2) remains unchanged because it tried to update (2, 3)
        assert_eq!(
            state.diplomatic_relations.get(&(3, 2)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
        );
    }

    #[test]
    fn test_dissolution_no_members_fallthrough() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        // Treaty with 0 members. The i and j loops will be empty, should_dissolve will remain false.
        state.treaties.insert(
            104,
            crate::sim::state::Treaty {
                id: 104,
                alliance_name: "Empty Accord".to_string(),
                member_empire_ids: vec![],
                formed_tick: 1,
                dissolved_tick: None,
            },
        );

        check_alliance_dissolution(&mut state);

        assert!(state.treaties.get(&104).unwrap().dissolved_tick.is_none());
    }

    #[test]
    fn test_dissolution_skips_already_dissolved() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        state.treaties.insert(
            105,
            crate::sim::state::Treaty {
                id: 105,
                alliance_name: "Dissolved Accord".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 1,
                dissolved_tick: Some(10),
            },
        );

        check_alliance_dissolution(&mut state);

        // It should still have the dissolved tick from before, not the current state.tick
        assert_eq!(state.treaties.get(&105).unwrap().dissolved_tick, Some(10));
    }

    #[test]
    fn test_alliance_formation_missing_relation_key_misses_update() {
        let mut state = setup_alliance_state();
        state.tick = ALLIANCE_FORMATION_COOLDOWN;

        state.empires.insert(
            3,
            crate::sim::state::Empire {
                id: 3,
                name: "Empire C".to_string(),
                government_type: "republic".to_string(),
                tax_rate_base: 0.1,
                tax_rate: 0.1,
            },
        );

        // Remove (1, 2) and add (3, 2). It iterates and finds (3, 2).
        // pair=(3, 2). It's not already allied. Add to eligible_pairs.
        // RNG fires. Name fetched. Alliance formed!
        // Then `let key = if 3 < 2 { (3, 2) } else { (2, 3) }` => `(2, 3)`
        // `get_mut(&(2, 3))` returns None because the map has `(3, 2)`.

        state.diplomatic_relations.remove(&(1, 2));
        state.diplomatic_relations.insert(
            (3, 2),
            crate::sim::state::DiplomaticRelation {
                empire_a_id: 3,
                empire_b_id: 2,
                tension: 0.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                neutral_since_tick: 0,
            },
        );

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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
            fn try_fill_bytes(&mut self, _dest: &mut [u8]) -> Result<(), rand::Error> {
                Ok(())
            }
        }

        let mut rng = LocalAlwaysFormRng;
        check_alliance_formation(&mut state, &mut rng);

        assert_eq!(state.treaties.len(), 1, "Should form an alliance");
        // Because of the key mismatch, it couldn't update the status! So it stays neutral.
        assert_eq!(
            state.diplomatic_relations.get(&(3, 2)).unwrap().status,
            DIPLOMATIC_STATUS_NEUTRAL
        );
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

    #[test]
    fn test_alliance_dissolution_due_to_high_tension() {
        let mut state = setup_alliance_state();

        // Add an active treaty for the pair
        state.treaties.insert(
            1,
            crate::sim::state::Treaty {
                id: 1,
                alliance_name: "Test Alliance".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 0,
                dissolved_tick: None,
            },
        );

        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.status = "alliance".to_string();
        rel.tension = 60.0; // Over ALLIANCE_DISSOLUTION_TENSION (which is 50.0)

        check_alliance_dissolution(&mut state);

        let treaty = state.treaties.get(&1).unwrap();
        assert!(
            treaty.dissolved_tick.is_some(),
            "Treaty should be dissolved"
        );

        let rel_after = state.diplomatic_relations.get(&(1, 2)).unwrap();
        assert_eq!(
            rel_after.status, "neutral",
            "Status should return to neutral"
        );
    }

    #[test]
    fn test_alliance_dissolution_ignores_dissolved_treaty() {
        let mut state = setup_alliance_state();

        state.treaties.insert(
            1,
            crate::sim::state::Treaty {
                id: 1,
                alliance_name: "Test Alliance".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 0,
                dissolved_tick: Some(5), // Already dissolved
            },
        );

        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.status = "neutral".to_string(); // Already updated
        rel.tension = 60.0; // High tension

        check_alliance_dissolution(&mut state);

        let rel_after = state.diplomatic_relations.get(&(1, 2)).unwrap();
        assert_eq!(rel_after.status, "neutral", "Status shouldn't change");
    }

    #[test]
    fn test_has_conflicting_alliances_with_defender_and_ally() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            crate::sim::state::War {
                id: 1,
                aggressor_id: 3,
                defender_id: 1,
                status: "active".to_string(),
                start_tick: 0,
                end_tick: None,
                participants: vec![
                    (3, "aggressor".to_string()),
                    (1, "defender".to_string()),
                    (2, "aggressor_ally".to_string()), // 2 is an aggressor ally against 1
                ],
                theaters: vec![],
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        // Empire 1 is defender, Empire 2 is aggressor ally. They shouldn't ally.
        assert!(
            has_conflicting_alliances(&state, 1, 2),
            "Should detect conflict when one is defender and other is aggressor ally"
        );
    }

    #[test]
    fn test_has_conflicting_alliances_with_ally_role() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            crate::sim::state::War {
                id: 1,
                aggressor_id: 3,
                defender_id: 4,
                status: "active".to_string(),
                start_tick: 0,
                end_tick: None,
                participants: vec![
                    (3, "aggressor".to_string()),
                    (4, "defender".to_string()),
                    (1, "ally".to_string()), // legacy role, side unknown
                    (2, "ally".to_string()), // legacy role, side unknown
                ],
                theaters: vec![],
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        // Both are in the same active war, but their sides are "ally" (unknown).
        // It should reject it conservatively.
        assert!(
            has_conflicting_alliances(&state, 1, 2),
            "Should conservatively detect conflict with legacy 'ally' roles"
        );
    }

    #[test]
    fn test_has_conflicting_alliances_unrecognized_role_returns_none() {
        let mut state = setup_alliance_state();

        state.wars.insert(
            1,
            crate::sim::state::War {
                id: 1,
                aggressor_id: 3,
                defender_id: 4,
                status: "active".to_string(),
                start_tick: 0,
                end_tick: None,
                participants: vec![
                    (1, "unknown_role".to_string()),
                    (2, "another_unknown".to_string()),
                ],
                theaters: vec![],
                cumulative_losses: 0.0,
                aggressor_exhaustion: 0.0,
                defender_exhaustion: 0.0,
            },
        );

        // Unknown roles should evaluate to None for participant_side, meaning no conflict detected
        // via the clear opposite-side check.
        assert!(
            has_conflicting_alliances(&state, 1, 2),
            "Should still fall back to the conservative 'both are in active war' check"
        );
    }

    #[test]
    fn test_run_alliances_calls_formation_and_dissolution() {
        let mut state = setup_alliance_state();
        state.tick = crate::sim::alliances::ALLIANCE_FORMATION_COOLDOWN;
        let rel = state.diplomatic_relations.get_mut(&(1, 2)).unwrap();
        rel.neutral_since_tick = 0;

        struct LocalAlwaysFormRng;
        impl rand::RngCore for LocalAlwaysFormRng {
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

        let mut rng = LocalAlwaysFormRng;
        run_alliances(&mut state, &mut rng);
        assert_eq!(state.treaties.len(), 1, "Should have formed an alliance");
    }
}
