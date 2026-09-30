with open("src/sim/politics.rs", "r") as f:
    content = f.read()

# I will find the exact lines to replace.
search = """                    if owner_strength > 0.0 && occupier_strength <= 0.0 {
                        if state.occupied_systems.remove(&system_id).is_some() {
                            if let Some(sec) = state
                                .star_systems
                                .get(&system_id)
                                .and_then(|s| state.sectors.get(&s.sector_id))
                            {
                                if let Some(count) = empire_occupied_systems.get_mut(&sec.empire_id) {
                                    *count = (*count - 1).max(0);
                                }
                            }
                        }
                    }"""

replace = """                    if owner_strength > 0.0
                        && occupier_strength <= 0.0
                        && state.occupied_systems.remove(&system_id).is_some()
                    {
                        if let Some(sec) = state
                            .star_systems
                            .get(&system_id)
                            .and_then(|s| state.sectors.get(&s.sector_id))
                        {
                            if let Some(count) = empire_occupied_systems.get_mut(&sec.empire_id) {
                                *count = (*count - 1).max(0);
                            }
                        }
                    }"""

content = content.replace(search, replace)
with open("src/sim/politics.rs", "w") as f:
    f.write(content)
