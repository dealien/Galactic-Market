use galactic_market::sim::SimState;

fn main() {
    // Run all registered benchmarks.
    divan::main();
}

#[divan::bench]
fn bench_sim_initialization() {
    let _state = SimState::new();
}
