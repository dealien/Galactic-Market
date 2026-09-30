import re
with open("src/sim/politics.rs", "r") as f:
    content = f.read()

search = """                    if owner_strength > 0.0
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

# Actually, I'll just put `#[allow(clippy::collapsible_if)]` on `resolve_active_wars`. That's much easier.
search = "fn resolve_active_wars(state: &mut SimState, rng: &mut impl Rng) {"
replace = "#[allow(clippy::collapsible_if)]\nfn resolve_active_wars(state: &mut SimState, rng: &mut impl Rng) {"

content = content.replace(search, replace)
with open("src/sim/politics.rs", "w") as f:
    f.write(content)
