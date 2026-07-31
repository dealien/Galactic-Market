sed -i 's/w.theaters.clone()/w.theaters.as_slice()/g' src/sim/politics.rs
cargo check
