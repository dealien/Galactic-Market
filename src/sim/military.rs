//! Military unit management, combat resolution, and maintenance costs.
//!
//! Units are either **fleets** (mobile, offensive/defensive) or **garrisons**
//! (stationary, defend a star system). Each unit has strength (combat power)
//! and morale (effectiveness multiplier).

use std::collections::HashSet;

use rand::Rng;
use tracing::info;

use crate::sim::state::{MilitaryUnit, SimState};

/// Per-tick maintenance cost per unit of strength.
const MAINTENANCE_COST_PER_STRENGTH: f64 = 0.5;

/// Morale penalty when a unit loses combat but survives (retreats).
const MORALE_LOSS_ON_RETREAT: f64 = 0.15;

/// Morale recovery per tick when stationed peacefully.
const MORALE_RECOVERY_PER_TICK: f64 = 0.02;

/// Morale recovery per tick for deployed units away from active theaters.
const DEPLOYED_MORALE_RECOVERY_PER_TICK: f64 = 0.01;

/// Random variance factor in combat (±percentage of effective strength).
const COMBAT_VARIANCE: f64 = 0.2;

/// Calculate total military strength for an empire at a specific system.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::military::calculate_military_strength;
/// use galactic_market::sim::state::{MilitaryUnit, SimState};
///
/// let mut state = SimState::new();
/// state.military_units.insert(
///     1,
///     MilitaryUnit {
///         id: 1,
///         empire_id: 1,
///         unit_type: "fleet".to_string(),
///         strength: 80.0,
///         system_id: 10,
///         status: "stationed".to_string(),
///         morale: 1.0,
///     },
/// );
///
/// assert_eq!(calculate_military_strength(&state, 1, 10), 80.0);
/// ```
pub fn calculate_military_strength(state: &SimState, empire_id: i32, system_id: i32) -> f64 {
    state
        .military_units
        .values()
        .filter(|u| u.empire_id == empire_id && u.system_id == system_id)
        .map(|u| u.strength * u.morale)
        .sum()
}

/// Resolve combat between two empires at a contested system.
///
/// Returns `(winner_empire_id, loser_empire_id)`. The loser's units in that
/// system are destroyed or retreat (strength reduced, morale penalized).
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::military::resolve_combat;
/// use galactic_market::sim::state::{MilitaryUnit, Sector, SimState, StarSystem};
/// use rand::rngs::StdRng;
/// use rand::SeedableRng;
///
/// let mut state = SimState::new();
/// state.sectors.insert(
///     1,
///     Sector {
///         id: 1,
///         empire_id: 1,
///         name: "Alpha".to_string(),
///     },
/// );
/// state.star_systems.insert(
///     10,
///     StarSystem {
///         id: 10,
///         sector_id: 1,
///         name: "Sol".to_string(),
///     },
/// );
/// state.military_units.insert(
///     1,
///     MilitaryUnit {
///         id: 1,
///         empire_id: 1,
///         unit_type: "fleet".to_string(),
///         strength: 100.0,
///         system_id: 10,
///         status: "stationed".to_string(),
///         morale: 1.0,
///     },
/// );
/// state.military_units.insert(
///     2,
///     MilitaryUnit {
///         id: 2,
///         empire_id: 2,
///         unit_type: "fleet".to_string(),
///         strength: 1.0,
///         system_id: 10,
///         status: "stationed".to_string(),
///         morale: 1.0,
///     },
/// );
///
/// let mut rng = StdRng::seed_from_u64(7);
/// let winner = resolve_combat(&mut state, 1, 2, 10, &mut rng);
///
/// assert_eq!(winner, Some(1));
/// ```
pub fn resolve_combat(
    state: &mut SimState,
    attacker_empire_id: i32,
    defender_empire_id: i32,
    system_id: i32,
    rng: &mut impl Rng,
) -> Option<i32> {
    let attacker_strength = calculate_military_strength(state, attacker_empire_id, system_id);
    let defender_strength = calculate_military_strength(state, defender_empire_id, system_id);

    if attacker_strength <= 0.0 && defender_strength <= 0.0 {
        return None;
    }

    // Apply random variance
    let attacker_roll =
        attacker_strength * (1.0 + rng.gen_range(-COMBAT_VARIANCE..COMBAT_VARIANCE));
    let defender_roll =
        defender_strength * (1.0 + rng.gen_range(-COMBAT_VARIANCE..COMBAT_VARIANCE));

    let (winner_id, loser_id) = if attacker_roll >= defender_roll {
        (attacker_empire_id, defender_empire_id)
    } else {
        (defender_empire_id, attacker_empire_id)
    };

    // Damage calculation: loser takes proportionally more damage
    let damage_ratio = if attacker_roll + defender_roll > 0.0 {
        (attacker_roll.max(defender_roll)) / (attacker_roll + defender_roll)
    } else {
        0.5
    };

    // Apply damage to loser's units
    let loser_unit_ids: Vec<i32> = state
        .military_units
        .values()
        .filter(|u| u.empire_id == loser_id && u.system_id == system_id)
        .map(|u| u.id)
        .collect();

    for unit_id in loser_unit_ids {
        if let Some(unit) = state.military_units.get_mut(&unit_id) {
            unit.strength *= 1.0 - damage_ratio;
            unit.morale = (unit.morale - MORALE_LOSS_ON_RETREAT).max(0.1);
            unit.status = "deployed".to_string();

            // Units with negligible strength are destroyed
            if unit.strength < 5.0 {
                state.military_units.remove(&unit_id);
            }
        }
    }

    // Winner takes moderate damage
    let winner_unit_ids: Vec<i32> = state
        .military_units
        .values()
        .filter(|u| u.empire_id == winner_id && u.system_id == system_id)
        .map(|u| u.id)
        .collect();

    for unit_id in winner_unit_ids {
        if let Some(unit) = state.military_units.get_mut(&unit_id) {
            unit.strength *= 1.0 - (1.0 - damage_ratio) * 0.5;
            unit.status = "in_combat".to_string();
        }
    }

    info!(
        "Combat at system {}: empire {} (str {:.0}) vs empire {} (str {:.0}) → winner: empire {}",
        system_id,
        attacker_empire_id,
        attacker_strength,
        defender_empire_id,
        defender_strength,
        winner_id
    );

    Some(winner_id)
}

/// Deduct military maintenance costs from empire treasuries each tick.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::military::apply_maintenance_costs;
/// use galactic_market::sim::state::{MilitaryUnit, SimState};
///
/// let mut state = SimState::new();
/// state.empire_treasuries.insert(1, 100.0);
/// state.military_units.insert(
///     1,
///     MilitaryUnit {
///         id: 1,
///         empire_id: 1,
///         unit_type: "fleet".to_string(),
///         strength: 10.0,
///         system_id: 10,
///         status: "stationed".to_string(),
///         morale: 1.0,
///     },
/// );
///
/// apply_maintenance_costs(&mut state);
///
/// assert!(state.empire_treasuries.get(&1).copied().unwrap() < 100.0);
/// ```
pub fn apply_maintenance_costs(state: &mut SimState) {
    // Collect per-empire total maintenance
    let mut empire_costs: std::collections::HashMap<i32, f64> = std::collections::HashMap::new();
    for unit in state.military_units.values() {
        *empire_costs.entry(unit.empire_id).or_insert(0.0) +=
            unit.strength * MAINTENANCE_COST_PER_STRENGTH;
    }

    for (empire_id, cost) in empire_costs {
        state.withdraw_from_empire_treasury(empire_id, cost);
    }
}

/// Recover morale for units that are stationed (not in combat).
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::military::recover_morale;
/// use galactic_market::sim::state::{MilitaryUnit, SimState};
///
/// let mut state = SimState::new();
/// state.military_units.insert(
///     1,
///     MilitaryUnit {
///         id: 1,
///         empire_id: 1,
///         unit_type: "garrison".to_string(),
///         strength: 50.0,
///         system_id: 10,
///         status: "stationed".to_string(),
///         morale: 0.5,
///     },
/// );
///
/// recover_morale(&mut state);
///
/// assert!(state.military_units.get(&1).unwrap().morale > 0.5);
/// ```
pub fn recover_morale(state: &mut SimState) {
    let active_theater_systems: HashSet<i32> = state
        .wars
        .values()
        .filter(|war| war.status == "active")
        .flat_map(|war| war.theaters.iter().copied())
        .collect();

    for unit in state.military_units.values_mut() {
        if unit.status == "stationed" {
            unit.morale = (unit.morale + MORALE_RECOVERY_PER_TICK).min(1.0);
        } else if unit.status == "deployed" && !active_theater_systems.contains(&unit.system_id) {
            unit.morale = (unit.morale + DEPLOYED_MORALE_RECOVERY_PER_TICK).min(1.0);
        }
    }
}

/// Seed initial military units for each empire based on their system count.
///
/// Each empire gets 2 fleets + 1 garrison per system they control.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::military::spawn_initial_units;
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
///     1,
///     StarSystem {
///         id: 1,
///         sector_id: 1,
///         name: "Home".to_string(),
///     },
/// );
///
/// spawn_initial_units(&mut state);
///
/// assert_eq!(state.military_units.len(), 3);
/// ```
pub fn spawn_initial_units(state: &mut SimState) {
    // Build empire → systems mapping from sectors
    let mut empire_systems: std::collections::HashMap<i32, Vec<i32>> =
        std::collections::HashMap::new();
    for system in state.star_systems.values() {
        if let Some(sector) = state.sectors.get(&system.sector_id) {
            empire_systems
                .entry(sector.empire_id)
                .or_default()
                .push(system.id);
        }
    }

    for (empire_id, systems) in empire_systems {
        for &system_id in &systems {
            // 2 fleets per system
            for _ in 0..2 {
                let id = state.next_military_unit_id();
                state.military_units.insert(
                    id,
                    MilitaryUnit {
                        id,
                        empire_id,
                        unit_type: "fleet".to_string(),
                        strength: 100.0,
                        system_id,
                        status: "stationed".to_string(),
                        morale: 1.0,
                    },
                );
            }

            // 1 garrison per system
            let id = state.next_military_unit_id();
            state.military_units.insert(
                id,
                MilitaryUnit {
                    id,
                    empire_id,
                    unit_type: "garrison".to_string(),
                    strength: 150.0,
                    system_id,
                    status: "stationed".to_string(),
                    morale: 1.0,
                },
            );
        }
    }

    info!(
        count = state.military_units.len(),
        "Spawned initial military units."
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{Sector, SimState, StarSystem};

    fn setup_combat_state() -> SimState {
        let mut state = SimState::new();

        // Two empires, one system
        state.sectors.insert(
            1,
            Sector {
                id: 1,
                empire_id: 1,
                name: "Sector A".to_string(),
            },
        );
        state.star_systems.insert(
            1,
            StarSystem {
                id: 1,
                sector_id: 1,
                name: "System 1".to_string(),
            },
        );

        // Attacker fleet
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

        // Defender garrison
        state.military_units.insert(
            2,
            MilitaryUnit {
                id: 2,
                empire_id: 2,
                unit_type: "garrison".to_string(),
                strength: 80.0,
                system_id: 1,
                status: "stationed".to_string(),
                morale: 1.0,
            },
        );

        state
    }

    #[test]
    fn test_military_strength_calculation() {
        let state = setup_combat_state();
        let strength = calculate_military_strength(&state, 1, 1);
        assert!((strength - 100.0).abs() < 0.01);
    }

    #[test]
    fn test_combat_produces_winner() {
        let mut state = setup_combat_state();
        let mut rng = rand::thread_rng();
        let winner = resolve_combat(&mut state, 1, 2, 1, &mut rng);
        assert!(winner.is_some());
        // Winner should be one of the two empires
        let w = winner.unwrap();
        assert!(w == 1 || w == 2);
    }

    #[test]
    fn test_maintenance_costs() {
        let mut state = setup_combat_state();
        state.empire_treasuries.insert(1, 10000.0);
        state.empire_treasuries.insert(2, 10000.0);

        apply_maintenance_costs(&mut state);

        // Empire 1 has unit with strength 100, cost = 100 * 0.5 = 50
        assert!((state.get_empire_treasury(1) - 9950.0).abs() < 0.01);
        // Empire 2 has unit with strength 80, cost = 80 * 0.5 = 40
        assert!((state.get_empire_treasury(2) - 9960.0).abs() < 0.01);
    }

    #[test]
    fn test_recover_morale_for_stationed_and_idle_deployed_units() {
        let mut state = SimState::new();
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
                sector_id: 1,
                name: "System 2".to_string(),
            },
        );
        state.star_systems.insert(
            3,
            StarSystem {
                id: 3,
                sector_id: 1,
                name: "System 3".to_string(),
            },
        );
        state.wars.insert(
            1,
            crate::sim::state::War {
                id: 1,
                aggressor_id: 1,
                defender_id: 2,
                participants: vec![(1, "aggressor".to_string()), (2, "defender".to_string())],
                theaters: vec![3],
                start_tick: 1,
                end_tick: None,
                status: "active".to_string(),
                cumulative_losses: 0.0,
            },
        );
        state.military_units.insert(
            1,
            MilitaryUnit {
                id: 1,
                empire_id: 1,
                unit_type: "garrison".to_string(),
                strength: 100.0,
                system_id: 1,
                status: "stationed".to_string(),
                morale: 0.5,
            },
        );
        state.military_units.insert(
            2,
            MilitaryUnit {
                id: 2,
                empire_id: 1,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 2,
                status: "deployed".to_string(),
                morale: 0.5,
            },
        );
        state.military_units.insert(
            3,
            MilitaryUnit {
                id: 3,
                empire_id: 2,
                unit_type: "fleet".to_string(),
                strength: 100.0,
                system_id: 3,
                status: "deployed".to_string(),
                morale: 0.5,
            },
        );

        recover_morale(&mut state);

        assert!((state.military_units.get(&1).unwrap().morale - 0.52).abs() < 0.0001);
        assert!((state.military_units.get(&2).unwrap().morale - 0.51).abs() < 0.0001);
        assert!((state.military_units.get(&3).unwrap().morale - 0.5).abs() < 0.0001);
    }

    #[test]
    fn test_spawn_initial_units() {
        let mut state = SimState::new();
        state.sectors.insert(
            1,
            Sector {
                id: 1,
                empire_id: 1,
                name: "Sector A".to_string(),
            },
        );
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
                sector_id: 1,
                name: "System 2".to_string(),
            },
        );

        spawn_initial_units(&mut state);

        // 2 systems × (2 fleets + 1 garrison) = 6 units
        assert_eq!(state.military_units.len(), 6);
    }
}
