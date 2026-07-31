sed -i 's/let active_wars: Vec<(i32, Vec<i32>, Vec<i32>)> = state/let active_wars: Vec<(i32, \&Vec<i32>, Vec<i32>)> = state/g' src/sim/politics.rs
sed -i 's/(w.id, w.theaters.clone(), participant_ids)/(w.id, \&w.theaters, participant_ids)/g' src/sim/politics.rs

sed -i 's/let active_wars: Vec<(i32, i32, i32, Vec<i32>)> = state/let active_wars: Vec<(i32, i32, i32, \&Vec<i32>)> = state/g' src/sim/politics.rs
sed -i 's/(w.id, w.aggressor_id, w.defender_id, w.theaters.clone())/(w.id, w.aggressor_id, w.defender_id, \&w.theaters)/g' src/sim/politics.rs

cargo check
