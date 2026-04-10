use galactic_market::sim::SimState;

#[tokio::test]
async fn test_sim_state_initialization() {
    let state = SimState::new();
    assert_eq!(state.tick, 0, "Initial tick should be 0");
}
