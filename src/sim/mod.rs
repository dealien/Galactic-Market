use sqlx::PgPool;
use tracing::info;

pub struct SimState {
    pub tick: u64,
}

impl SimState {
    pub fn new() -> Self {
        Self { tick: 0 }
    }

    pub async fn run_tick(&mut self, _pool: &PgPool) -> Result<(), sqlx::Error> {
        self.tick += 1;
        info!("--- Tick {} ---", self.tick);

        // Phase 1-9 logic will go here

        // Phase 10: Flush
        // In the future: flush dirty states to db here

        Ok(())
    }
}
