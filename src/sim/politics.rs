//! Dedicated politics phase: tension management, war execution, occupation,
//! sector control, and military maintenance.

use std::collections::HashSet;

use rand::Rng;
use tracing::info;

use crate::db::seed::{DIPLOMATIC_STATUS_NEUTRAL, DIPLOMATIC_STATUS_WAR};
use crate::sim::military;
use crate::sim::state::{Occupation, SectorControl, SimState, War};

const ALLIED_TENSION_DECAY_RATE: f64 = 0.1;
const WAR_TENSION_THRESHOLD: f64 = 100.0;
const TENSION_DECAY_RATE: f64 = 0.01;
const SECTOR_SPLIT_TENSION_INCREASE: f64 = 0.1;
const SECTOR_SPLIT_PRODUCTION_PENALTY: f64 = 0.15;
const OCCUPATION_PRODUCTION_PENALTY: f64 = 0.25;
const WAR_THEATER_PRODUCTION_PENALTY: f64 = 0.50;
const WAR_EXHAUSTION_THRESHOLD: f64 = 500.0;

/// Run the politics phase over the current simulation state.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::politics::run_politics;
/// use galactic_market::sim::state::SimState;
/// use rand::rngs::StdRng;
/// use rand::SeedableRng;
///
/// let mut state = SimState::new();
/// let mut rng = StdRng::seed_from_u64(42);
///
/// run_politics(&mut state, &mut rng);
/// ```
pub fn run_politics(state: &mut SimState, rng: &mut impl Rng) {
    update_tension(state);
    check_war_declarations(state);
    resolve_active_wars(state, rng);
    process_occupations(state);
    compute_sector_control(state);
    military::apply_maintenance_costs(state);
    military::recover_morale(state);
}

fn update_tension(state: &mut SimState) {
    let allied_pairs = active_treaty_pairs(state);

    for rel in state.diplomatic_relations.values_mut() {
        let key = if rel.empire_a_id < rel.empire_b_id {
            (rel.empire_a_id, rel.empire_b_id)
        } else {
            (rel.empire_b_id, rel.empire_a_id)
        };

        if allied_pairs.contains(&key) {
            rel.tension = (rel.tension - ALLIED_TENSION_DECAY_RATE).max(0.0);
            continue;
        }

        if rel.status == DIPLOMATIC_STATUS_NEUTRAL {
            rel.tension = (rel.tension - TENSION_DECAY_RATE).max(0.0);
        }
    }
}

fn active_treaty_pairs(state: &SimState) -> HashSet<(i32, i32)> {
    let mut allied_pairs = HashSet::new();

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
                    allied_pairs.insert((a, b));
                }
            }
        }
    }

    allied_pairs
}

fn station_participant_units_in_system(
    state: &mut SimState,
    participant_empire_ids: &HashSet<i32>,
    system_id: i32,
) {
    for unit in state.military_units.values_mut() {
        if unit.system_id == system_id && participant_empire_ids.contains(&unit.empire_id) {
            unit.status = "stationed".to_string();
        }
    }
}

fn station_participant_units_in_theaters(
    state: &mut SimState,
    participant_empire_ids: &HashSet<i32>,
    theaters: &[i32],
) {
    for &system_id in theaters {
        station_participant_units_in_system(state, participant_empire_ids, system_id);
    }
}

fn check_war_declarations(state: &mut SimState) {
    let mut new_wars: Vec<(i32, i32)> = Vec::new();

    for rel in state.diplomatic_relations.values_mut() {
        if rel.status == DIPLOMATIC_STATUS_NEUTRAL && rel.tension >= WAR_TENSION_THRESHOLD {
            rel.status = DIPLOMATIC_STATUS_WAR.to_string();
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

        let theaters = find_border_systems(state, aggressor_id, defender_id);

        let war_id = state.next_war_id();
        let mut participants = vec![
            (aggressor_id, "aggressor".to_string()),
            (defender_id, "defender".to_string()),
        ];

        let aggressor_allies = get_allies(state, aggressor_id);
        let defender_allies = get_allies(state, defender_id);

        let mut aggressor_side = vec![aggressor_id];
        let mut defender_side = vec![defender_id];

        for ally_id in &defender_allies {
            if *ally_id != aggressor_id && *ally_id != defender_id {
                participants.push((*ally_id, "defender_ally".to_string()));
                defender_side.push(*ally_id);
            }
        }
        for ally_id in &aggressor_allies {
            if *ally_id != aggressor_id && *ally_id != defender_id {
                participants.push((*ally_id, "aggressor_ally".to_string()));
                aggressor_side.push(*ally_id);
            }
        }

        for &a in &aggressor_side {
            for &b in &defender_side {
                if a != b {
                    set_war_status(state, a, b);
                }
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

fn resolve_active_wars(state: &mut SimState, rng: &mut impl Rng) {
    let active_wars: Vec<(i32, i32, i32, Vec<i32>)> = state
        .wars
        .values()
        .filter(|w| w.status == "active")
        .map(|w| (w.id, w.aggressor_id, w.defender_id, w.theaters.clone()))
        .collect();

    for (war_id, aggressor_id, defender_id, theaters) in active_wars {
        let participant_empire_ids: HashSet<i32> = state
            .wars
            .get(&war_id)
            .map(|war| {
                war.participants
                    .iter()
                    .map(|(empire_id, _)| *empire_id)
                    .collect()
            })
            .unwrap_or_default();
        let mut aggressor_losses = 0.0_f64;
        let mut defender_losses = 0.0_f64;
        let mut war_concluded = false;

        for &system_id in &theaters {
            let pre_attacker =
                military::calculate_military_strength(state, aggressor_id, system_id);
            let pre_defender = military::calculate_military_strength(state, defender_id, system_id);

            if pre_attacker > 0.0 && pre_defender > 0.0 {
                let _winner =
                    military::resolve_combat(state, aggressor_id, defender_id, system_id, rng);

                let post_attacker =
                    military::calculate_military_strength(state, aggressor_id, system_id);
                let post_defender =
                    military::calculate_military_strength(state, defender_id, system_id);

                aggressor_losses += (pre_attacker - post_attacker).max(0.0);
                defender_losses += (pre_defender - post_defender).max(0.0);
            }

            let attacker_str =
                military::calculate_military_strength(state, aggressor_id, system_id);
            let defender_str = military::calculate_military_strength(state, defender_id, system_id);

            let system_owner_empire_id = state
                .star_systems
                .get(&system_id)
                .and_then(|system| state.sectors.get(&system.sector_id))
                .map(|sector| sector.empire_id);

            if let Some(owner_empire_id) = system_owner_empire_id {
                let new_occupier_id = if attacker_str > 0.0
                    && defender_str <= 0.0
                    && owner_empire_id == defender_id
                {
                    Some(aggressor_id)
                } else if defender_str > 0.0
                    && attacker_str <= 0.0
                    && owner_empire_id == aggressor_id
                {
                    Some(defender_id)
                } else {
                    None
                };

                if let Some(occupier_empire_id) = new_occupier_id {
                    let existing_occupier_id = state
                        .occupied_systems
                        .get(&system_id)
                        .map(|occ| occ.occupier_empire_id);
                    if existing_occupier_id != Some(occupier_empire_id) {
                        state.occupied_systems.insert(
                            system_id,
                            Occupation {
                                system_id,
                                occupier_empire_id,
                                since_tick: state.tick,
                            },
                        );
                        info!(
                            "System {} occupied by empire {}!",
                            system_id, occupier_empire_id
                        );
                    }
                }

                if let Some(current_occupier_id) = state
                    .occupied_systems
                    .get(&system_id)
                    .map(|occ| occ.occupier_empire_id)
                {
                    let owner_strength =
                        military::calculate_military_strength(state, owner_empire_id, system_id);
                    let occupier_strength = military::calculate_military_strength(
                        state,
                        current_occupier_id,
                        system_id,
                    );

                    if owner_strength > 0.0 && occupier_strength <= 0.0 {
                        state.occupied_systems.remove(&system_id);
                    }
                }
            }

            let system_contested =
                military::calculate_military_strength(state, aggressor_id, system_id) > 0.0
                    && military::calculate_military_strength(state, defender_id, system_id) > 0.0;

            if !system_contested {
                station_participant_units_in_system(state, &participant_empire_ids, system_id);
            }
        }

        let tick_losses = aggressor_losses + defender_losses;
        if let Some(war) = state.wars.get_mut(&war_id) {
            war.cumulative_losses += tick_losses;
            if war.cumulative_losses > WAR_EXHAUSTION_THRESHOLD {
                war.status = "concluded".to_string();
                war.end_tick = Some(state.tick);
                war_concluded = true;
                info!(
                    "War {} concluded due to exhaustion (cumulative losses: {:.0}).",
                    war_id, war.cumulative_losses
                );
            }
        }

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
            war_concluded = true;
            info!("War {} concluded: no contested systems remain.", war_id);
        }

        if war_concluded {
            station_participant_units_in_theaters(state, &participant_empire_ids, &theaters);

            let mut aggressor_side = Vec::new();
            let mut defender_side = Vec::new();
            let mut legacy_allies = Vec::new();
            let mut participants_list = Vec::new();

            if let Some(war) = state.wars.get(&war_id) {
                participants_list = war.participants.clone();
                for (empire_id, role) in &war.participants {
                    match role.as_str() {
                        "aggressor" | "aggressor_ally" => aggressor_side.push(*empire_id),
                        "defender" | "defender_ally" => defender_side.push(*empire_id),
                        "ally" => legacy_allies.push(*empire_id),
                        _ => {}
                    }
                }
            }

            let mut reset_pairs = HashSet::new();

            // Cross-side pairs
            for &a in &aggressor_side {
                for &b in &defender_side {
                    let key = if a < b { (a, b) } else { (b, a) };
                    reset_pairs.insert(key);
                }
            }

            // Legacy ally pairs (treated conservatively as unknown side)
            for &x in &legacy_allies {
                for &(y, _) in &participants_list {
                    if x != y {
                        let key = if x < y { (x, y) } else { (y, x) };
                        reset_pairs.insert(key);
                    }
                }
            }

            for (a, b) in reset_pairs {
                let has_other_active_war = state.wars.values().any(|w| {
                    w.id != war_id
                        && w.status == "active"
                        && w.participants.iter().any(|(p_id, _)| *p_id == a)
                        && w.participants.iter().any(|(p_id, _)| *p_id == b)
                });

                if !has_other_active_war
                    && let Some(rel) = state.diplomatic_relations.get_mut(&(a, b))
                    && rel.status == DIPLOMATIC_STATUS_WAR
                {
                    rel.status = DIPLOMATIC_STATUS_NEUTRAL.to_string();
                    rel.tension = 50.0;
                }
            }
        }
    }
}

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

fn set_war_status(state: &mut SimState, empire_a: i32, empire_b: i32) {
    let key = if empire_a < empire_b {
        (empire_a, empire_b)
    } else {
        (empire_b, empire_a)
    };
    if let Some(rel) = state.diplomatic_relations.get_mut(&key) {
        rel.status = DIPLOMATIC_STATUS_WAR.to_string();
    }
}

fn process_occupations(state: &mut SimState) {
    let occupied_ids: Vec<i32> = state.occupied_systems.keys().cloned().collect();

    for system_id in occupied_ids {
        let occupier_id = match state.occupied_systems.get(&system_id) {
            Some(occ) => occ.occupier_empire_id,
            None => continue,
        };

        let occupier_strength =
            military::calculate_military_strength(state, occupier_id, system_id);
        if occupier_strength <= 0.0 {
            state.occupied_systems.remove(&system_id);
            info!(
                "System {} liberated (occupier {} has no garrison).",
                system_id, occupier_id
            );
        }
    }
}

/// Rebuild sector control from the current ownership and occupation state.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::politics::compute_sector_control;
/// use galactic_market::sim::state::{Sector, SimState, StarSystem};
///
/// let mut state = SimState::new();
/// state.sectors.insert(
///     1,
///     Sector {
///         id: 1,
///         empire_id: 1,
///         name: "Core".to_string(),
///     },
/// );
/// state.star_systems.insert(
///     10,
///     StarSystem {
///         id: 10,
///         sector_id: 1,
///         name: "Home".to_string(),
///     },
/// );
///
/// compute_sector_control(&mut state);
///
/// assert_eq!(state.sector_control.get(&1).unwrap().total_systems, 1);
/// ```
pub fn compute_sector_control(state: &mut SimState) {
    state.sector_control.clear();
    let allied_pairs = active_treaty_pairs(state);

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
                    if allied_pairs.contains(&(a, b)) {
                        continue;
                    }

                    if let Some(rel) = state.diplomatic_relations.get_mut(&(a, b)) {
                        rel.tension += SECTOR_SPLIT_TENSION_INCREASE;
                    }
                }
            }
        }
    }
}

/// Determine whether a system is part of any active war theater.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::politics::is_system_in_war_theater;
/// use galactic_market::sim::state::{SimState, War};
///
/// let mut state = SimState::new();
/// state.wars.insert(
///     1,
///     War {
///         id: 1,
///         aggressor_id: 1,
///         defender_id: 2,
///         participants: vec![
///             (1, "aggressor".to_string()),
///             (2, "defender".to_string()),
///         ],
///         theaters: vec![10],
///         start_tick: 0,
///         end_tick: None,
///         status: "active".to_string(),
///         cumulative_losses: 0.0,
///     },
/// );
///
/// assert!(is_system_in_war_theater(&state, 10));
/// ```
pub fn is_system_in_war_theater(state: &SimState, system_id: i32) -> bool {
    state
        .wars
        .values()
        .any(|w| w.status == "active" && w.theaters.contains(&system_id))
}

/// Calculate the production penalty affecting a system.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::politics::get_system_production_penalty;
/// use galactic_market::sim::state::SimState;
///
/// let state = SimState::new();
///
/// assert_eq!(get_system_production_penalty(&state, 10), 0.0);
/// ```
pub fn get_system_production_penalty(state: &SimState, system_id: i32) -> f64 {
    let mut penalty = 0.0;

    if is_system_in_war_theater(state, system_id) {
        penalty += WAR_THEATER_PRODUCTION_PENALTY;
    }

    if state.occupied_systems.contains_key(&system_id) {
        penalty += OCCUPATION_PRODUCTION_PENALTY;
    }

    if let Some(system) = state.star_systems.get(&system_id)
        && let Some(control) = state.sector_control.get(&system.sector_id)
        && control.is_split
    {
        penalty += SECTOR_SPLIT_PRODUCTION_PENALTY;
    }

    penalty.min(0.9)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{
        DiplomaticRelation, Empire, MilitaryUnit, Sector, StarSystem, SystemLane, Treaty,
    };
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn setup_political_state() -> SimState {
        let mut state = SimState::new();
        state.tick = 100;

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

        state.system_lanes.insert(
            (2, 3),
            SystemLane {
                system_a_id: 2,
                system_b_id: 3,
                distance_ly: 5.0,
                lane_type: "standard".to_string(),
            },
        );

        state.diplomatic_relations.insert(
            (1, 2),
            DiplomaticRelation {
                empire_a_id: 1,
                empire_b_id: 2,
                tension: 0.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
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
        assert_eq!(rel.status, DIPLOMATIC_STATUS_WAR);
        assert_eq!(state.wars.len(), 1);
    }

    #[test]
    fn test_border_systems_found() {
        let state = setup_political_state();
        let theaters = find_border_systems(&state, 1, 2);
        assert!(theaters.contains(&2));
        assert!(theaters.contains(&3));
    }

    #[test]
    fn test_sector_control_computation() {
        let mut state = setup_political_state();
        compute_sector_control(&mut state);
        assert!(!state.sector_control.get(&1).unwrap().is_split);
        assert!(!state.sector_control.get(&2).unwrap().is_split);
    }

    #[test]
    fn test_sector_split_with_occupation() {
        let mut state = setup_political_state();
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
    fn test_sector_split_tension_skips_active_treaty_pairs() {
        let mut state = setup_political_state();

        state.empires.insert(
            3,
            Empire {
                id: 3,
                name: "Consortium".to_string(),
                government_type: "Corporate".to_string(),
                tax_rate_base: 0.08,
            },
        );

        state.sectors.insert(
            3,
            Sector {
                id: 3,
                empire_id: 3,
                name: "Outer".to_string(),
            },
        );

        state.star_systems.insert(
            5,
            StarSystem {
                id: 5,
                sector_id: 1,
                name: "Delta".to_string(),
            },
        );

        state.diplomatic_relations.insert(
            (1, 3),
            DiplomaticRelation {
                empire_a_id: 1,
                empire_b_id: 3,
                tension: 5.0,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
            },
        );
        state.diplomatic_relations.insert(
            (2, 3),
            DiplomaticRelation {
                empire_a_id: 2,
                empire_b_id: 3,
                tension: 7.5,
                status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
            },
        );

        state.treaties.insert(
            1,
            Treaty {
                id: 1,
                alliance_name: "Republic-Syndicate Accord".to_string(),
                member_empire_ids: vec![1, 2],
                formed_tick: 90,
                dissolved_tick: None,
            },
        );

        state.occupied_systems.insert(
            2,
            Occupation {
                system_id: 2,
                occupier_empire_id: 2,
                since_tick: 95,
            },
        );
        state.occupied_systems.insert(
            5,
            Occupation {
                system_id: 5,
                occupier_empire_id: 3,
                since_tick: 95,
            },
        );

        compute_sector_control(&mut state);

        assert!(state.sector_control.get(&1).unwrap().is_split);

        let allied_rel = state.diplomatic_relations.get(&(1, 2)).unwrap();
        assert!((allied_rel.tension - 0.0).abs() < f64::EPSILON);

        let rel_1_3 = state.diplomatic_relations.get(&(1, 3)).unwrap();
        assert!((rel_1_3.tension - 5.1).abs() < f64::EPSILON);

        let rel_2_3 = state.diplomatic_relations.get(&(2, 3)).unwrap();
        assert!((rel_2_3.tension - 7.6).abs() < f64::EPSILON);
    }

    #[test]
    fn test_occupation_liberation() {
        let mut state = setup_political_state();
        state.occupied_systems.insert(
            3,
            Occupation {
                system_id: 3,
                occupier_empire_id: 1,
                since_tick: 50,
            },
        );
        process_occupations(&mut state);
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
        assert!(state.occupied_systems.contains_key(&3));
    }

    #[test]
    fn test_defender_can_occupy_aggressor_system() {
        let mut state = setup_political_state();
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 2,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 1,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![1],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(7);
        resolve_active_wars(&mut state, &mut rng);

        let occupation = state.occupied_systems.get(&1).unwrap();
        assert_eq!(occupation.occupier_empire_id, 2);
    }

    #[test]
    fn test_uncontested_theater_resets_participant_units_to_stationed() {
        let mut state = setup_political_state();
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 1,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 1,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.military_units.insert(
            2,
            MilitaryUnit {
                id: 2,
                empire_id: 2,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 1,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.military_units.insert(
            3,
            MilitaryUnit {
                id: 3,
                empire_id: 1,
                unit_type: "fleet".to_string(),
                strength: 80.0,
                system_id: 2,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![1, 2],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(7);
        resolve_active_wars(&mut state, &mut rng);

        assert_eq!(state.military_units.get(&3).unwrap().status, "stationed");
        assert_eq!(state.wars.get(&1).unwrap().status, "active");
    }

    #[test]
    fn test_war_conclusion_resets_participant_units_in_theaters() {
        let mut state = setup_political_state();
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 1,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 3,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![3],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(9);
        resolve_active_wars(&mut state, &mut rng);

        assert_eq!(state.wars.get(&1).unwrap().status, "concluded");
        assert_eq!(state.military_units.get(&1).unwrap().status, "stationed");
    }

    #[test]
    fn test_owner_reclaims_occupied_system_when_occupier_absent() {
        let mut state = setup_political_state();
        state.occupied_systems.insert(
            1,
            Occupation {
                system_id: 1,
                occupier_empire_id: 2,
                since_tick: 95,
            },
        );
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 1,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 1,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![1],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(9);
        resolve_active_wars(&mut state, &mut rng);

        assert!(!state.occupied_systems.contains_key(&1));
    }

    #[test]
    fn test_war_exhaustion_accumulates_across_ticks() {
        let mut state = setup_political_state();
        state.star_systems.insert(
            5,
            StarSystem {
                id: 5,
                sector_id: 1,
                name: "Frontier".to_string(),
            },
        );
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 1,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 5,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.military_units.insert(
            2,
            MilitaryUnit {
                id: 2,
                empire_id: 2,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 5,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.wars.insert(
            1,
            War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![5],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(42);

        resolve_active_wars(&mut state, &mut rng);
        let first_tick_losses = state.wars.get(&1).unwrap().cumulative_losses;
        assert!(first_tick_losses > 0.0);

        resolve_active_wars(&mut state, &mut rng);
        let second_tick_losses = state.wars.get(&1).unwrap().cumulative_losses;

        assert!(second_tick_losses > first_tick_losses);
    }

    #[test]
    fn test_war_conclusion_resets_ally_relations() {
        let mut state = setup_political_state();

        state.empires.insert(
            3,
            Empire {
                id: 3,
                name: "AggressorAlly".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.1,
            },
        );
        state.empires.insert(
            4,
            Empire {
                id: 4,
                name: "DefenderAlly".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.1,
            },
        );
        state.empires.insert(
            5,
            Empire {
                id: 5,
                name: "LegacyAlly".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.1,
            },
        );

        let relations_to_insert = vec![(1, 2), (1, 4), (3, 2), (3, 4), (1, 5), (2, 5), (3, 5)];

        for (a, b) in relations_to_insert {
            let key = if a < b { (a, b) } else { (b, a) };
            state.diplomatic_relations.insert(
                key,
                DiplomaticRelation {
                    empire_a_id: key.0,
                    empire_b_id: key.1,
                    tension: 100.0,
                    status: DIPLOMATIC_STATUS_WAR.to_string(),
                },
            );
        }

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
                    (5, "ally".to_string()),
                ],
                theaters: vec![3],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(9);
        resolve_active_wars(&mut state, &mut rng);

        assert_eq!(state.wars.get(&1).unwrap().status, "concluded");

        let reset_keys = vec![(1, 2), (1, 4), (2, 3), (3, 4), (1, 5), (2, 5)];

        for (a, b) in reset_keys {
            let key = if a < b { (a, b) } else { (b, a) };
            let rel = state.diplomatic_relations.get(&key).unwrap();
            assert_eq!(rel.status, DIPLOMATIC_STATUS_NEUTRAL);
            assert_eq!(rel.tension, 50.0);
        }
    }

    #[test]
    fn test_war_conclusion_retains_relations_if_other_active_war() {
        let mut state = setup_political_state();

        state.empires.insert(
            3,
            Empire {
                id: 3,
                name: "AggressorAlly".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.1,
            },
        );

        for (a, b) in vec![(1, 2), (2, 3)] {
            let key = if a < b { (a, b) } else { (b, a) };
            state.diplomatic_relations.insert(
                key,
                DiplomaticRelation {
                    empire_a_id: key.0,
                    empire_b_id: key.1,
                    tension: 100.0,
                    status: DIPLOMATIC_STATUS_WAR.to_string(),
                },
            );
        }

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
                ],
                theaters: vec![3],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        state.wars.insert(
            2,
            War {
                id: 2,
                aggressor_id: 3,
                defender_id: 2,
                participants: vec![(3, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![3],
                start_tick: 99,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );

        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 3,
                unit_type: "fleet".to_string(),
                strength: 50.0,
                system_id: 3,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );
        state.military_units.insert(
            2,
            MilitaryUnit {
                id: 2,
                empire_id: 2,
                unit_type: "fleet".to_string(),
                strength: 50.0,
                system_id: 3,
                status: "deployed".to_string(),
                morale: 1.0,
            },
        );

        let mut rng = StdRng::seed_from_u64(9);
        resolve_active_wars(&mut state, &mut rng);

        assert_eq!(state.wars.get(&1).unwrap().status, "concluded");
        assert_eq!(state.wars.get(&2).unwrap().status, "active");

        let rel_1_2 = state.diplomatic_relations.get(&(1, 2)).unwrap();
        assert_eq!(rel_1_2.status, DIPLOMATIC_STATUS_NEUTRAL);

        let rel_2_3 = state.diplomatic_relations.get(&(2, 3)).unwrap();
        assert_eq!(rel_2_3.status, DIPLOMATIC_STATUS_WAR);
    }

    #[test]
    fn test_war_declaration_escalates_allies_and_opposing_allies() {
        let mut state = setup_political_state();

        state.empires.insert(
            3,
            Empire {
                id: 3,
                name: "AggressorAlly".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.1,
            },
        );
        state.empires.insert(
            4,
            Empire {
                id: 4,
                name: "DefenderAlly".to_string(),
                government_type: "Democracy".to_string(),
                tax_rate_base: 0.1,
            },
        );

        state.treaties.insert(
            1,
            Treaty {
                id: 1,
                alliance_name: "Aggressor Alliance".to_string(),
                member_empire_ids: vec![1, 3],
                formed_tick: 90,
                dissolved_tick: None,
            },
        );
        state.treaties.insert(
            2,
            Treaty {
                id: 2,
                alliance_name: "Defender Alliance".to_string(),
                member_empire_ids: vec![2, 4],
                formed_tick: 90,
                dissolved_tick: None,
            },
        );

        let pairs = vec![(1, 3), (1, 4), (2, 3), (2, 4), (3, 4)];
        for (a, b) in pairs {
            let key = if a < b { (a, b) } else { (b, a) };
            state.diplomatic_relations.insert(
                key,
                DiplomaticRelation {
                    empire_a_id: key.0,
                    empire_b_id: key.1,
                    tension: 0.0,
                    status: DIPLOMATIC_STATUS_NEUTRAL.to_string(),
                },
            );
        }

        state.diplomatic_relations.get_mut(&(1, 2)).unwrap().tension = 100.0;

        check_war_declarations(&mut state);

        assert_eq!(state.wars.len(), 1);
        let war = state.wars.values().next().unwrap();
        assert_eq!(war.status, "active");

        let participants: HashSet<_> = war
            .participants
            .iter()
            .map(|(id, role)| (*id, role.clone()))
            .collect();
        assert!(participants.contains(&(1, "aggressor".to_string())));
        assert!(participants.contains(&(2, "defender".to_string())));
        assert!(participants.contains(&(3, "aggressor_ally".to_string())));
        assert!(participants.contains(&(4, "defender_ally".to_string())));

        let cross_pairs = vec![(1, 2), (1, 4), (2, 3), (3, 4)];
        for (a, b) in cross_pairs {
            let key = if a < b { (a, b) } else { (b, a) };
            let rel = state.diplomatic_relations.get(&key).unwrap();
            assert_eq!(rel.status, DIPLOMATIC_STATUS_WAR);
        }
    }
}
