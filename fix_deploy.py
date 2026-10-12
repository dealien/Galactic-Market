with open("src/sim/politics.rs", "r") as f:
    content = f.read()

search = """            let fleet_ids = stationed_fleets_by_empire.get(&empire_id).map(|v| v.as_slice()).unwrap_or(&[]);

            for &fleet_id in fleet_ids {
                let fleet_sys = match state.military_units.get(&fleet_id) {
                    Some(f) => f.system_id,
                    None => continue,
                };"""

replace = """            let fleet_ids = stationed_fleets_by_empire.get(&empire_id).map(|v| v.as_slice()).unwrap_or(&[]);

            for &fleet_id in fleet_ids {
                let fleet_sys = match state.military_units.get(&fleet_id) {
                    Some(f) if f.status == "stationed" => f.system_id,
                    _ => continue,
                };"""

content = content.replace(search, replace)

with open("src/sim/politics.rs", "w") as f:
    f.write(content)
