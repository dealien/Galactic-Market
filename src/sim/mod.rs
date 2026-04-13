pub mod consumption;
pub mod decisions;
pub mod finance;
pub mod logistics;
pub mod markets;
pub mod production;
pub mod resources;
pub mod state;

use comfy_table::Table;
use comfy_table::presets::UTF8_FULL_CONDENSED;
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

        // ── Phase 2: Production / refining ───────────────────────────────────
        production::run_production(self);

        // ── Phase 3: Logistics ───────────────────────────────────────────────
        logistics::run_logistics(self, self.tick);

        // ── Phase 6: Company AI decisions ─────────────────────────────────────
        decisions::run_decisions(self, self.tick);

        // ── Phase 3: Population consumption ───────────────────────────────
        consumption::run_consumption(self, self.tick);

        // ── Phase 4: Market clearing ────────────────────────────────────
        markets::clear_orders(self, self.tick);

        // ── Phase 5: Finance ───────────────────────────────────────────────
        finance::run_finance(self);

        // ── Phase 10: Periodic DB flush ───────────────────────────────────────
        if self.tick.is_multiple_of(FLUSH_INTERVAL) {
            let summary = self.generate_summary();

            let mut table = Table::new();
            table.load_preset(UTF8_FULL_CONDENSED);
            table.set_header(vec!["Metric", "Value"]);
            table.add_row(vec!["Tick", &summary.tick.to_string()]);
            table.add_row(vec!["Total Cash", &format!("{:.0}", summary.total_cash)]);
            table.add_row(vec!["Total Debt", &format!("{:.0}", summary.total_debt)]);
            table.add_row(vec![
                "Total Inventory",
                &summary.total_inventory.to_string(),
            ]);
            table.add_row(vec!["Active Orders", &summary.active_orders.to_string()]);
            table.add_row(vec!["Trade Volume", &summary.trade_volume.to_string()]);
            table.add_row(vec![
                "Avg Ore Price",
                &format!("{:.2}", summary.avg_ore_price),
            ]);

            let mut ingot_prices: Vec<_> = summary.ingot_prices.iter().collect();
            ingot_prices.sort_by_key(|(name, _)| *name);

            for (name, price) in ingot_prices {
                table.add_row(vec![&format!("Price: {}", name), &format!("{:.2}", price)]);
            }

            info!("\n=== Economic Pulse ===\n{table}");
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

        // ── Loans ─────────────────────────────────────────────────────────────
        for loan in self.loans.values() {
            sqlx::query("UPDATE loans SET balance = $1 WHERE id = $2")
                .bind(loan.balance)
                .bind(loan.id)
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

        // ── Trade Routes (In-Transit) ──────────────────────────────────────────
        // Wipe and rewrite active trade routes for simplicity in Stage 1
        sqlx::query("DELETE FROM trade_routes")
            .execute(&mut *tx)
            .await?;

        for route in self.trade_routes.values() {
            sqlx::query(
                "INSERT INTO trade_routes (id, company_id, origin_city_id, dest_city_id, resource_type_id, quantity, arrival_tick)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(route.id)
            .bind(route.company_id)
            .bind(route.origin_city_id)
            .bind(route.dest_city_id)
            .bind(route.resource_type_id)
            .bind(route.quantity)
            .bind(route.arrival_tick as i64)
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
