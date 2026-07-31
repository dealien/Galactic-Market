sed -i 's/for &theater_sys in &theaters/for &theater_sys in theaters/g' src/sim/politics.rs
sed -i 's/for &system_id in &theaters/for &system_id in theaters/g' src/sim/politics.rs
cargo check
