//! Dedicated politics phase: tension management, war execution, occupation,
//! sector control, and military maintenance.
//!
//! This module replaces the inline `process_politics` logic that was in events.rs.

use rand::Rng;
use tracing::info;

use crate::sim::military;
use crate::sim::state::{Occupation, SectorControl, SimState, War};

/// Tension reduction per tick for allied empires.
const ALLIED_TENSION_DECAY_RATE: f64 = 0.1;

/// Tension threshold that triggers war declaration.
const WAR_TENSION_THRESHOLD: f64 = 100.0;

/// Natural tension decay per tick during neutral status.
const TENSION_DECAY_RATE: f64 = 0.01;

/// Tension increase per tick for empires sharing a split sector.
const SECTOR_SPLIT_TENSION_INCREASE: f64 = 0.1;

/// Production penalty for systems in a split sector (fractional reduction).
const SECTOR_SPLIT_PRODUCTION_PENALTY: f64 = 0.15;

/// Production penalty for occupied systems (fractional reduction).
const OCCUPATION_PRODUCTION_PENALTY: f64 = 0.25;

/// Production penalty for systems in an active war theater.
const WAR_THEATER_PRODUCTION_PENALTY: f64 = 0.50;

/// War exhaustion threshold — cumulative losses that force peace.
const WAR_EXHAUSTION_THRESHOLD: f64 = 500.0;

/// Run the full politics phase for one tick.
///
/// Sub-steps:
/// 1. Update tension (natural decay)
/// 2. Check war declaration thresholds
/// 3. Resolve active combats
/// 4. Process occupations
/// 5. Compute sector control and apply split penalties
/// 6. Deduct military maintenance
/// 7. Recover morale for stationed units
pub fn run_politics(state: &mut SimState, rng: &mut impl Rng) {
    update_tension(state);
    check_war_declarations(state);
    resolve_active_wars(state, rng);
    process_occupations(state);
    compute_sector_control(state);
    military::apply_maintenance_costs(state);
    military::recover_morale(state);
}

/// Natural tension decay and allied empire protection.
fn update_tension(state: &mut SimState) {
    // Collect allied empire pairs (cannot have tension between them)
    let mut allied_pairs: Vec<(i32, i32)> = Vec::new();
    for treaty in state.treaties.values() {
        if treaty.dissolved_tick.is_none() {
            let members = &treaty.member_empire_ids;
            for i in 0..members.len() {
                for j in (i + 1)..members.len() {
                    let (a, b) = if members[i] < members[j] {
                        (members[i], members[j])
                    } else {
                        (members[j], members[i])
                    };
                    allied_pairs.push((a, b));
                }
            }
        }
    }

    for rel in state.diplomatic_relations.values_mut() {
        let key = if rel.empire_a_id < rel.empire_b_id {
            (rel.empire_a_id, rel.empire_b_id)
        } else {
            (rel.empire_b_id, rel.empire_a_id)
        };

        // Allied empires cannot have tension increase
        if allied_pairs.contains(&key) {
            rel.tension = (rel.tension - ALLIED_TENSION_DECAY_RATE).max(0.0);
            continue;
        }

        // Natural tension decay for neutral relations
        if rel.status == "neutral" {
            rel.tension = (rel.tension - TENSION_DECAY_RATE).max(0.0);
        }
    }
}

/// Check if any pair has crossed the war threshold and declare war.
fn check_war_declarations(state: &mut SimState) {
    let mut new_wars: Vec<(i32, i32)> = Vec::new();

    for rel in state.diplomatic_relations.values_mut() {
        if rel.status == "neutral" && rel.tension >= WAR_TENSION_THRESHOLD {
            rel.status = "war".to_string();
            new_wars.push((rel.empire_a_id, rel.empire_b_id));
        }
    }

    for (aggressor_id, defender_id) in new_wars {
        let name_a = state
            .empires
            .get(&aggressor_id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let name_b = state
            .empires
            .get(&defender_id)
            .map(|e| e.name.clone())
            .unwrap_or_default();
        info!("WAR DECLARED: {} vs {}!", name_a, name_b);

        // Determine border systems as theaters
        let theaters = find_border_systems(state, aggressor_id, defender_id);

        let war_id = state.next_war_id();
        let mut participants = vec![
            (aggressor_id, "aggressor".to_string()),
            (defender_id, "defender".to_string()),
        ];

        // Pull in allies
        let aggressor_allies = get_allies(state, aggressor_id);
        let defender_allies = get_allies(state, defender_id);

        for ally_id in &defender_allies {
            if *ally_id != aggressor_id && *ally_id != defender_id {
                participants.push((*ally_id, "defender_ally".to_string()));
                // Set ally's relation with aggressor to war
                set_war_status(state, *ally_id, aggressor_id);
            }
        }
        for ally_id in &aggressor_allies {
            if *ally_id != aggressor_id && *ally_id != defender_id {
                participants.push((*ally_id, "aggressor_ally".to_string()));
                set_war_status(state, *ally_id, defender_id);
            }
        }

        state.wars.insert(
            war_id,
            War {
                id: war_id,
                aggressor_id,
                defender_id,
                participants,
                theaters,
                start_tick: state.tick,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        // Create a war event for event tracking
        let event_id = state.next_event_id;
        state.next_event_id += 1;
        state.active_events.insert(
            event_id,
            crate::sim::state::ActiveEvent {
                id: event_id,
                event_type: "war".to_string(),
                target_id: None,
                severity: 1.0,
                start_tick: state.tick,
                end_tick: state.tick + 1000,
                flavor_text: Some(format!("The {} and {} are at open war!", name_a, name_b)),
            },
        );
    }
}

/// Find systems at the border between two empires (adjacent systems owned by different empires).
fn find_border_systems(state: &SimState, empire_a: i32, empire_b: i32) -> Vec<i32> {
    let mut a_systems: Vec<i32> = Vec::new();
    let mut b_systems: Vec<i32> = Vec::new();

    for system in state.star_systems.values() {
        if let Some(sector) = state.sectors.get(&system.sector_id) {
            if sector.empire_id == empire_a {
                a_systems.push(system.id);
            } else if sector.empire_id == empire_b {
                b_systems.push(system.id);
            }
        }
    }

    // Border systems: systems that have a lane connecting to enemy territory
    let mut theaters = Vec::new();
    for &sys_a in &a_systems {
        for lane in state.system_lanes.values() {
            let neighbor = if lane.system_a_id == sys_a {
                Some(lane.system_b_id)
            } else if lane.system_b_id == sys_a {
                Some(lane.system_a_id)
            } else {
                None
            };
            if let Some(n) = neighbor
                && b_systems.contains(&n)
            {
                if !theaters.contains(&sys_a) {
                    theaters.push(sys_a);
                }
                if !theaters.contains(&n) {
                    theaters.push(n);
                }
            }
        }
    }

    theaters
}

/// Get all empires allied with the given empire through active treaties.
fn get_allies(state: &SimState, empire_id: i32) -> Vec<i32> {
    let mut allies = Vec::new();
    for treaty in state.treaties.values() {
        if treaty.dissolved_tick.is_none() && treaty.member_empire_ids.contains(&empire_id) {
            for &member in &treaty.member_empire_ids {
                if member != empire_id && !allies.contains(&member) {
                    allies.push(member);
                }
            }
        }
    }
    allies
}

/// Set the diplomatic relation between two empires to "war".
fn set_war_status(state: &mut SimState, empire_a: i32, empire_b: i32) {
    let key = if empire_a < empire_b {
        (empire_a, empire_b)
    } else {
        (empire_b, empire_a)
    };
    if let Some(rel) = state.diplomatic_relations.get_mut(&key) {
        rel.status = "war".to_string();
    }
}

/// Resolve active combats in war theaters.
fn resolve_active_wars(state: &mut SimState, rng: &mut impl Rng) {
    // Collect active wars and their theaters
    let active_wars: Vec<(i32, i32, i32, Vec<i32>)> = state
        .wars
        .values()
        .filter(|w| w.status == "active")
        .map(|w| (w.id, w.aggressor_id, w.defender_id, w.theaters.clone()))
        .collect();

    for (war_id, aggressor_id, defender_id, theaters) in active_wars {
        let mut aggressor_losses = 0.0_f64;
        let mut defender_losses = 0.0_f64;

        for &system_id in &theaters {
            let pre_attacker =
                military::calculate_military_strength(state, aggressor_id, system_id);
            let pre_defender = military::calculate_military_strength(state, defender_id, system_id);

            if pre_attacker > 0.0 && pre_defender > 0.0 {
                // Both sides present → combat
                let _winner =
                    military::resolve_combat(state, aggressor_id, defender_id, system_id, rng);

                let post_attacker =
                    military::calculate_military_strength(state, aggressor_id, system_id);
                let post_defender =
                    military::calculate_military_strength(state, defender_id, system_id);

                aggressor_losses += (pre_attacker - post_attacker).max(0.0);
                defender_losses += (pre_defender - post_defender).max(0.0);
            }

            // Check for occupation: if one side has no units, system is occupied
            let attacker_str =
                military::calculate_military_strength(state, aggressor_id, system_id);
            let defender_str = military::calculate_military_strength(state, defender_id, system_id);

            if attacker_str > 0.0 && defender_str <= 0.0 {
                // Check if system belongs to the defender
                if let Some(system) = state.star_systems.get(&system_id)
                    && let Some(sector) = state.sectors.get(&system.sector_id)
                    && sector.empire_id == defender_id
                    && !state.occupied_systems.contains_key(&system_id)
                {
                    state.occupied_systems.insert(
                        system_id,
                        Occupation {
                            system_id,
                            occupier_empire_id: aggressor_id,
                            since_tick: state.tick,
                        },
                    );
                    info!("System {} occupied by empire {}!", system_id, aggressor_id);
                }
            } else if defender_str > 0.0 && attacker_str <= 0.0 {
                // Defender retakes their own system
                if let Some(system) = state.star_systems.get(&system_id)
                    && let Some(sector) = state.sectors.get(&system.sector_id)
                    && sector.empire_id == defender_id
                {
                    state.occupied_systems.remove(&system_id);
                }
            }
        }

        // Accumulate tick losses into the war's cumulative total, then check
        // war exhaustion against the running sum so endings are reachable.
        let tick_losses = aggressor_losses + defender_losses;
        let cumulative_losses = if let Some(war) = state.wars.get_mut(&war_id) {
            war.cumulative_losses += tick_losses;
            war.cumulative_losses
        } else {
            0.0
        };

        if cumulative_losses > WAR_EXHAUSTION_THRESHOLD {
            if let Some(war) = state.wars.get_mut(&war_id) {
                war.status = "concluded".to_string();
                war.end_tick = Some(state.tick);
                info!(
                    "War {} concluded due to exhaustion (cumulative losses: {:.0}).",
                    war_id, cumulative_losses
                );
            }

            // Reset diplomatic status to neutral
            let key = if aggressor_id < defender_id {
                (aggressor_id, defender_id)
            } else {
                (defender_id, aggressor_id)
            };
            if let Some(rel) = state.diplomatic_relations.get_mut(&key) {
                rel.status = "neutral".to_string();
                rel.tension = 50.0; // Post-war residual tension
            }
        }

        // Also end war if all theaters have been resolved (no contested systems remain)
        let still_contested = theaters.iter().any(|&sys_id| {
            let a = military::calculate_military_strength(state, aggressor_id, sys_id);
            let b = military::calculate_military_strength(state, defender_id, sys_id);
            a > 0.0 && b > 0.0
        });

        if !still_contested
            && let Some(war) = state.wars.get_mut(&war_id)
            && war.status == "active"
        {
            war.status = "concluded".to_string();
            war.end_tick = Some(state.tick);
            info!("War {} concluded: no contested systems remain.", war_id);

            // Reset diplomatic status
            let key = if aggressor_id < defender_id {
                (aggressor_id, defender_id)
            } else {
                (defender_id, aggressor_id)
            };
            if let Some(rel) = state.diplomatic_relations.get_mut(&key) {
                rel.status = "neutral".to_string();
                rel.tension = 50.0;
            }
        }
    }
}

/// Process existing occupations: check garrison maintenance, apply penalties.
fn process_occupations(state: &mut SimState) {
    let occupied_ids: Vec<i32> = state.occupied_systems.keys().cloned().collect();

    for system_id in occupied_ids {
        let occupier_id = match state.occupied_systems.get(&system_id) {
            Some(occ) => occ.occupier_empire_id,
            None => continue,
        };

        // Check if occupier still has garrison/fleet in the system
        let occupier_strength =
            military::calculate_military_strength(state, occupier_id, system_id);
        if occupier_strength <= 0.0 {
            // Liberation! Occupier lost their garrison
            state.occupied_systems.remove(&system_id);
            info!(
                "System {} liberated (occupier {} has no garrison).",
                system_id, occupier_id
            );
        }
    }
}

/// Compute sector control: for each sector, count systems held by each empire.
/// A system's effective controller is its occupier (if occupied) or its sector's owner.
pub fn compute_sector_control(state: &mut SimState) {
    state.sector_control.clear();

    // Build sector → system list
    let mut sector_systems: std::collections::HashMap<i32, Vec<i32>> =
        std::collections::HashMap::new();
    for system in state.star_systems.values() {
        sector_systems
            .entry(system.sector_id)
            .or_default()
            .push(system.id);
    }

    for (&sector_id, systems) in &sector_systems {
        let sector_owner = state
            .sectors
            .get(&sector_id)
            .map(|s| s.empire_id)
            .unwrap_or(0);

        let mut empire_system_counts: std::collections::HashMap<i32, usize> =
            std::collections::HashMap::new();

        for &sys_id in systems {
            let effective_controller = if let Some(occ) = state.occupied_systems.get(&sys_id) {
                occ.occupier_empire_id
            } else {
                sector_owner
            };
            *empire_system_counts
                .entry(effective_controller)
                .or_insert(0) += 1;
        }

        let is_split = empire_system_counts.len() > 1;
        let total_systems = systems.len();

        state.sector_control.insert(
            sector_id,
            SectorControl {
                sector_id,
                empire_system_counts,
                total_systems,
                is_split,
            },
        );
    }

    // Apply tension increase for split sectors
    for control in state.sector_control.values() {
        if control.is_split {
            let empires_in_sector: Vec<i32> =
                control.empire_system_counts.keys().cloned().collect();
            for i in 0..empires_in_sector.len() {
                for j in (i + 1)..empires_in_sector.len() {
                    let (a, b) = if empires_in_sector[i] < empires_in_sector[j] {
                        (empires_in_sector[i], empires_in_sector[j])
                    } else {
                        (empires_in_sector[j], empires_in_sector[i])
                    };
                    if let Some(rel) = state.diplomatic_relations.get_mut(&(a, b)) {
                        rel.tension += SECTOR_SPLIT_TENSION_INCREASE;
                    }
                }
            }
        }
    }
}

/// Check if a system is in an active war theater (used by production/logistics phases).
pub fn is_system_in_war_theater(state: &SimState, system_id: i32) -> bool {
    state
        .wars
        .values()
        .any(|w| w.status == "active" && w.theaters.contains(&system_id))
}

/// Get the production penalty multiplier for a system (0.0 = no penalty, approaches 1.0 for max penalty).
///
/// Combines war theater, occupation, and sector split penalties.
pub fn get_system_production_penalty(state: &SimState, system_id: i32) -> f64 {
    let mut penalty = 0.0;

    // War theater penalty
    if is_system_in_war_theater(state, system_id) {
        penalty += WAR_THEATER_PRODUCTION_PENALTY;
    }

    // Occupation penalty
    if state.occupied_systems.contains_key(&system_id) {
        penalty += OCCUPATION_PRODUCTION_PENALTY;
    }

    // Sector split penalty
    if let Some(system) = state.star_systems.get(&system_id)
        && let Some(control) = state.sector_control.get(&system.sector_id)
        && control.is_split
    {
        penalty += SECTOR_SPLIT_PRODUCTION_PENALTY;
    }

    penalty.min(0.9) // Cap at 90% reduction
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{
        DiplomaticRelation, Empire, MilitaryUnit, Sector, StarSystem, SystemLane,
    };

    fn setup_political_state() -> SimState {
        let mut state = SimState::new();
        state.tick = 100;

        // Two empires
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

        // Two sectors (one per empire)
        state.sectors.insert(
            1,
            Sector {
                id: 1,
                empire_id: 1,
                name: "Core".to_string(),
            },
        );
        state.sectors.insert(
            2,
            Sector {
                id: 2,
                empire_id: 2,
                name: "Rim".to_string(),
            },
        );

        // Systems
        state.star_systems.insert(
            1,
            StarSystem {
                id: 1,
                sector_id: 1,
                name: "Sol".to_string(),
            },
        );
        state.star_systems.insert(
            2,
            StarSystem {
                id: 2,
                sector_id: 1,
                name: "Alpha".to_string(),
            },
        );
        state.star_systems.insert(
            3,
            StarSystem {
                id: 3,
                sector_id: 2,
                name: "Beta".to_string(),
            },
        );
        state.star_systems.insert(
            4,
            StarSystem {
                id: 4,
                sector_id: 2,
                name: "Gamma".to_string(),
            },
        );

        // Lane connecting sectors (border)
        state.system_lanes.insert(
            (2, 3),
            SystemLane {
                system_a_id: 2,
                system_b_id: 3,
                distance_ly: 5.0,
                lane_type: "standard".to_string(),
            },
        );

        // Diplomatic relation
        state.diplomatic_relations.insert(
            (1, 2),
            DiplomaticRelation {
                empire_a_id: 1,
                empire_b_id: 2,
                tension: 0.0,
                status: "neutral".to_string(),
            },
        );

        state
    }

    #[test]
    fn test_tension_decay() {
        let mut state = setup_political_state();
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 50.0;

        update_tension(&mut state);

        let rel = state.diplomatic_relations.get(&(1, 2)).unwrap();
        assert!(rel.tension < 50.0);
    }

    #[test]
    fn test_war_declaration_at_threshold() {
        let mut state = setup_political_state();
        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 100.0;

        check_war_declarations(&mut state);

        let rel = state.diplomatic_relations.get(&(1, 2)).unwrap();
        assert_eq!(rel.status, "war");
        assert_eq!(state.wars.len(), 1);
    }

    #[test]
    fn test_border_systems_found() {
        let state = setup_political_state();
        let theaters = find_border_systems(&state, 1, 2);
        // Systems 2 and 3 are connected across empires
        assert!(theaters.contains(&2));
        assert!(theaters.contains(&3));
    }

    #[test]
    fn test_sector_control_computation() {
        let mut state = setup_political_state();
        compute_sector_control(&mut state);

        // No occupation → each sector controlled by one empire
        assert!(!state.sector_control.get(&1).unwrap().is_split);
        assert!(!state.sector_control.get(&2).unwrap().is_split);
    }

    #[test]
    fn test_sector_split_with_occupation() {
        let mut state = setup_political_state();
        // Empire 2 occupies system 2 (which belongs to empire 1's sector)
        state.occupied_systems.insert(
            2,
            Occupation {
                system_id: 2,
                occupier_empire_id: 2,
                since_tick: 50,
            },
        );

        compute_sector_control(&mut state);

        assert!(state.sector_control.get(&1).unwrap().is_split);
    }

    #[test]
    fn test_occupation_liberation() {
        let mut state = setup_political_state();
        // System 3 occupied by empire 1, but no garrison present
        state.occupied_systems.insert(
            3,
            Occupation {
                system_id: 3,
                occupier_empire_id: 1,
                since_tick: 50,
            },
        );

        process_occupations(&mut state);

        // Should be liberated since no military units present
        assert!(!state.occupied_systems.contains_key(&3));
    }

    #[test]
    fn test_occupation_maintained_with_garrison() {
        let mut state = setup_political_state();
        state.occupied_systems.insert(
            3,
            Occupation {
                system_id: 3,
                occupier_empire_id: 1,
                since_tick: 50,
            },
        );
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 1,
                unit_type: "garrison".to_string(),
                strength: 100.0,
                system_id: 3,
                status: "stationed".to_string(),
                morale: 1.0,
            },
        );

        process_occupations(&mut state);

        // Should remain occupied
        assert!(state.occupied_systems.contains_key(&3));
    }
}
