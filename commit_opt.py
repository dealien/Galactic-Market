import re

with open("src/sim/politics.rs", "r") as f:
    content = f.read()

# 1. Optimize deploy_fleets_to_theaters
search1 = """fn deploy_fleets_to_theaters(state: &mut SimState) {
    // Ensure system distances are built (resilient for unit tests)
    crate::sim::logistics::build_system_distances(state);

    let active_wars: Vec<(i32, Vec<i32>, Vec<i32>)> = state
        .wars
        .values()
        .filter(|w| w.status == "active")
        .map(|w| {
            let participant_ids = w.participants.iter().map(|(p, _)| *p).collect();
            (w.id, w.theaters.clone(), participant_ids)
        })
        .collect();

    for (war_id, theaters, participant_ids) in active_wars {
        if theaters.is_empty() {
            continue;
        }

        for &empire_id in &participant_ids {
            let fleet_ids: Vec<i32> = state
                .military_units
                .values()
                .filter(|u| {
                    u.empire_id == empire_id && u.unit_type == "fleet" && u.status == "stationed"
                })
                .map(|u| u.id)
                .collect();

            for fleet_id in fleet_ids {"""

replace1 = """fn deploy_fleets_to_theaters(state: &mut SimState) {
    // Ensure system distances are built (resilient for unit tests)
    crate::sim::logistics::build_system_distances(state);

    let active_wars: Vec<(i32, Vec<i32>, Vec<i32>)> = state
        .wars
        .values()
        .filter(|w| w.status == "active")
        .map(|w| {
            let participant_ids = w.participants.iter().map(|(p, _)| *p).collect();
            (w.id, w.theaters.clone(), participant_ids)
        })
        .collect();

    if active_wars.is_empty() {
        return;
    }

    // Bolt optimization: Pre-group stationed fleets by empire to avoid O(wars * units) scaling
    let mut stationed_fleets_by_empire: std::collections::HashMap<i32, Vec<i32>> = std::collections::HashMap::new();
    for unit in state.military_units.values() {
        if unit.unit_type == "fleet" && unit.status == "stationed" {
            stationed_fleets_by_empire.entry(unit.empire_id).or_default().push(unit.id);
        }
    }

    for (war_id, theaters, participant_ids) in active_wars {
        if theaters.is_empty() {
            continue;
        }

        for &empire_id in &participant_ids {
            let fleet_ids = stationed_fleets_by_empire.get(&empire_id).map(|v| v.as_slice()).unwrap_or(&[]);

            for &fleet_id in fleet_ids {"""

content = content.replace(search1, replace1)

# 2. Optimize resolve_active_wars

search2 = """fn resolve_active_wars(state: &mut SimState, rng: &mut impl Rng) {
    let active_wars: Vec<(i32, i32, i32, Vec<i32>)> = state
        .wars
        .values()
        .filter(|w| w.status == "active")
        .map(|w| (w.id, w.aggressor_id, w.defender_id, w.theaters.clone()))
        .collect();"""

replace2 = """fn resolve_active_wars(state: &mut SimState, rng: &mut impl Rng) {
    let active_wars: Vec<(i32, i32, i32, Vec<i32>)> = state
        .wars
        .values()
        .filter(|w| w.status == "active")
        .map(|w| (w.id, w.aggressor_id, w.defender_id, w.theaters.clone()))
        .collect();

    // Bolt optimization: Pre-calculate territory state for all empires to avoid O(wars * systems) iteration.
    let mut empire_total_systems: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();
    let mut empire_occupied_systems: std::collections::HashMap<i32, i32> = std::collections::HashMap::new();

    if !active_wars.is_empty() {
        for s in state.star_systems.values() {
            if let Some(sec) = state.sectors.get(&s.sector_id) {
                *empire_total_systems.entry(sec.empire_id).or_insert(0) += 1;
                if state.occupied_systems.contains_key(&s.id) {
                    *empire_occupied_systems.entry(sec.empire_id).or_insert(0) += 1;
                }
            }
        }
    }"""

content = content.replace(search2, replace2)


search3 = """                        state.occupied_systems.insert(
                            system_id,
                            Occupation {
                                system_id,
                                occupier_empire_id,
                                since_tick: state.tick,
                            },
                        );"""

replace3 = """                        state.occupied_systems.insert(
                            system_id,
                            Occupation {
                                system_id,
                                occupier_empire_id,
                                since_tick: state.tick,
                            },
                        );
                        // Update cached occupation state
                        if let Some(sec) = state.star_systems.get(&system_id).and_then(|s| state.sectors.get(&s.sector_id)) {
                            *empire_occupied_systems.entry(sec.empire_id).or_insert(0) += 1;
                        }"""

content = content.replace(search3, replace3)


search4 = """                    if owner_strength > 0.0 && occupier_strength <= 0.0 {
                        state.occupied_systems.remove(&system_id);
                    }"""

replace4 = """                    if owner_strength > 0.0 && occupier_strength <= 0.0 {
                        if state.occupied_systems.remove(&system_id).is_some() {
                            if let Some(sec) = state.star_systems.get(&system_id).and_then(|s| state.sectors.get(&s.sector_id)) {
                                if let Some(count) = empire_occupied_systems.get_mut(&sec.empire_id) {
                                    *count = (*count - 1).max(0);
                                }
                            }
                        }
                    }"""

content = content.replace(search4, replace4)


search5 = """        let mut aggressor_occ_strain = 0.0;
        let mut defender_occ_strain = 0.0;
        let mut aggressor_total = 0;
        let mut aggressor_occupied = 0;
        let mut defender_total = 0;
        let mut defender_occupied = 0;

        for s in state.star_systems.values() {
            if let Some(sec) = state.sectors.get(&s.sector_id) {
                let is_occupied = state.occupied_systems.contains_key(&s.id);

                if is_occupied {
                    if aggressor_side.contains(&sec.empire_id) {
                        aggressor_occ_strain += 1.0;
                    } else if defender_side.contains(&sec.empire_id) {
                        defender_occ_strain += 1.0;
                    }
                }

                if sec.empire_id == aggressor_id {
                    aggressor_total += 1;
                    if is_occupied {
                        aggressor_occupied += 1;
                    }
                } else if sec.empire_id == defender_id {
                    defender_total += 1;
                    if is_occupied {
                        defender_occupied += 1;
                    }
                }
            }
        }"""

replace5 = """        let mut aggressor_occ_strain = 0.0;
        let mut defender_occ_strain = 0.0;

        // Bolt optimization: To avoid looping over all systems in the O(wars) loop,
        // we use a cached map of territories if wars are active.
        for &emp_id in &aggressor_side {
            aggressor_occ_strain += empire_occupied_systems.get(&emp_id).copied().unwrap_or(0) as f64;
        }
        for &emp_id in &defender_side {
            defender_occ_strain += empire_occupied_systems.get(&emp_id).copied().unwrap_or(0) as f64;
        }

        let aggressor_total = empire_total_systems.get(&aggressor_id).copied().unwrap_or(0);
        let aggressor_occupied = empire_occupied_systems.get(&aggressor_id).copied().unwrap_or(0);
        let defender_total = empire_total_systems.get(&defender_id).copied().unwrap_or(0);
        let defender_occupied = empire_occupied_systems.get(&defender_id).copied().unwrap_or(0);"""

content = content.replace(search5, replace5)

with open("src/sim/politics.rs", "w") as f:
    f.write(content)
