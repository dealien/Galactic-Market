with open("src/sim/politics.rs", "r") as f:
    content = f.read()

# I need to match the actual code that's failing clippy.
search = """                    if owner_strength > 0.0 && occupier_strength <= 0.0 && state.occupied_systems.remove(&system_id).is_some() {
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
# Wait, the search block isn't working because in previous attempts `fix_clippy2.py` failed to match. Let me just use regex.

import re
content = re.sub(
    r'                    if owner_strength > 0\.0 && occupier_strength <= 0\.0 \{\s*if state\.occupied_systems\.remove\(&system_id\)\.is_some\(\) \{\s*if let Some\(sec\) = state\s*\.star_systems\s*\.get\(&system_id\)\s*\.and_then\(\|s\| state\.sectors\.get\(&s\.sector_id\)\)\s*\{\s*if let Some\(count\) = empire_occupied_systems\.get_mut\(&sec\.empire_id\) \{\s*\*count = \(\*count - 1\)\.max\(0\);\s*\}\s*\}\s*\}\s*\}',
    r'''                    if owner_strength > 0.0 && occupier_strength <= 0.0 && state.occupied_systems.remove(&system_id).is_some() {
                        if let Some(sec) = state.star_systems.get(&system_id).and_then(|s| state.sectors.get(&s.sector_id)) {
                            if let Some(count) = empire_occupied_systems.get_mut(&sec.empire_id) {
                                *count = (*count - 1).max(0);
                            }
                        }
                    }''',
    content
)

with open("src/sim/politics.rs", "w") as f:
    f.write(content)
