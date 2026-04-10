pub mod consumption;
pub mod decisions;
pub mod markets;
pub mod production;
pub mod resources;
pub mod state;

use sqlx::PgPool;
use tracing::info;

// Re-export the primary state type for ergonomic use by callers
pub use state::SimState;

/// Flush interval: write dirty state to the database every N ticks.
const FLUSH_INTERVAL: u64 = 100;

impl SimState {
    /// Advance the simulation by one tick, running all active phases in order.
    pub async fn run_tick(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        self.tick += 1;

        // Only log every 100 ticks to avoid spamming the log
        if self.tick.is_multiple_of(100) || self.tick == 1 {
            info!("--- Tick {} ---", self.tick);
        }

        // ── Phase 1: Resource extraction ─────────────────────────────────────
        resources::run_extraction(self);

        // ── Phase 6: Company AI decisions ─────────────────────────────────────
        decisions::run_decisions(self, self.tick);

        // ── Phase 2: Production / refining ───────────────────────────────────
        production::run_production(self);

        // ── Phase 3: Population consumption ───────────────────────────────
        consumption::run_consumption(self, self.tick);

        // ── Phase 4: Market clearing ────────────────────────────────────
        markets::clear_orders(self, self.tick);

        // ── Phase 10: Periodic DB flush ───────────────────────────────────────
        if self.tick.is_multiple_of(FLUSH_INTERVAL) {
            let summary = self.generate_summary();
            let ingots_fmt = summary
                .ingot_prices
                .iter()
                .map(|(name, price)| format!("{}: {:.2}", name, price))
                .collect::<Vec<_>>()
                .join(", ");

            info!(
                tick = summary.tick,
                cash = %format!("{:.0}", summary.total_cash),
                debt = %format!("{:.0}", summary.total_debt),
                inventory = summary.total_inventory,
                orders = summary.active_orders,
                volume = summary.trade_volume,
                ore_price = %format!("{:.2}", summary.avg_ore_price),
                ingots = %ingots_fmt,
                "=== Economic Pulse ==="
            );
            self.flush(pool).await?;
        }

        Ok(())
    }

    /// Flush in-memory state to the database.
    ///
    /// Upserts deposits, inventories, company cash/debt, and appends market
    /// history rows accumulated since the last flush. All writes are batched
    /// inside a single transaction so a crash between flushes recovers cleanly.
    async fn flush(&mut self, pool: &PgPool) -> Result<(), sqlx::Error> {
        let mut tx = pool.begin().await?;

        // ── Deposits ──────────────────────────────────────────────────────────
        for deposit in self.deposits.values() {
            sqlx::query("UPDATE deposits SET size_remaining = $1 WHERE id = $2")
                .bind(deposit.size_remaining)
                .bind(deposit.id)
                .execute(&mut *tx)
                .await?;
        }

        // ── Inventories ───────────────────────────────────────────────────────
        for inv in self.inventories.values() {
            sqlx::query(
                "INSERT INTO inventory (company_id, city_id, resource_type_id, quantity)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (company_id, city_id, resource_type_id)
                 DO UPDATE SET quantity = EXCLUDED.quantity",
            )
            .bind(inv.company_id)
            .bind(inv.city_id)
            .bind(inv.resource_type_id)
            .bind(inv.quantity)
            .execute(&mut *tx)
            .await?;
        }

        // ── Company financials ────────────────────────────────────────────────
        for company in self.companies.values() {
            sqlx::query(
                "UPDATE companies SET cash = $1, debt = $2, next_eval_tick = $3 WHERE id = $4",
            )
            .bind(company.cash)
            .bind(company.debt)
            .bind(company.next_eval_tick as i64)
            .bind(company.id)
            .execute(&mut *tx)
            .await?;
        }

        // ── Facilities ────────────────────────────────────────────────────────
        for facility in self.facilities.values() {
            let ratios_json = facility
                .production_ratios
                .as_ref()
                .map(|r| sqlx::types::Json(r.clone()));
            sqlx::query(
                "UPDATE facilities SET setup_ticks_remaining = $1, target_resource_id = $2, production_ratios = $3 WHERE id = $4",
            )
            .bind(facility.setup_ticks_remaining as i32)
            .bind(facility.target_resource_id)
            .bind(ratios_json)
            .bind(facility.id)
            .execute(&mut *tx)
            .await?;
        }

        // ── Market history ────────────────────────────────────────────────────
        for h in &self.market_history_buffer {
            sqlx::query(
                "INSERT INTO market_history (city_id, resource_type_id, tick, open, high, low, close, volume)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (city_id, resource_type_id, tick) DO NOTHING",
            )
            .bind(h.city_id)
            .bind(h.resource_type_id)
            .bind(h.tick as i64)
            .bind(h.open)
            .bind(h.high)
            .bind(h.low)
            .bind(h.close)
            .bind(h.volume)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        self.market_history_buffer.clear();

        info!(
            tick = self.tick,
            deposits = self.deposits.len(),
            inventories = self.inventories.len(),
            companies = self.companies.len(),
            "DB flush complete."
        );
        Ok(())
    }
}
