pub mod consumption;
pub mod decisions;
pub mod events;
pub mod finance;
pub mod logistics;
pub mod markets;
pub mod namegen;
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
    pub async fn run_tick(
        &mut self,
        pool: &PgPool,
        rng: &mut impl rand::Rng,
    ) -> Result<(), sqlx::Error> {
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

        // ── Phase 9: Random Events ───────────────────────────────────────────
        events::run_events(self, rng);

        // ── Phase 10: Periodic DB flush ───────────────────────────────────────
        if self.tick.is_multiple_of(FLUSH_INTERVAL) {
            let summary = self.generate_summary();

            // ═══════════════════════════════════════════════════════════════════
            // Build a comprehensive multi-column Economic Pulse display
            // ═══════════════════════════════════════════════════════════════════

            // --- Table 1: Financial Summary ---
            let mut financial_table = Table::new();
            financial_table.load_preset(UTF8_FULL_CONDENSED);
            financial_table.set_header(vec!["Financial Metric", "Value"]);
            financial_table.add_row(vec!["Total Cash", &format!("{:.0}", summary.total_cash)]);
            financial_table.add_row(vec!["Total Debt", &format!("{:.0}", summary.total_debt)]);
            financial_table.add_row(vec![
                "Debt-to-Cash Ratio",
                &format!("{:.2}", summary.avg_debt_to_cash),
            ]);
            financial_table.add_row(vec![
                "Total Inventory",
                &summary.total_inventory.to_string(),
            ]);
            financial_table.add_row(vec![
                "Trade Volume (This Flush)",
                &summary.trade_volume.to_string(),
            ]);

            // --- Table 2: Market Activity ---
            let mut market_table = Table::new();
            market_table.load_preset(UTF8_FULL_CONDENSED);
            market_table.set_header(vec!["Market Metric", "Value"]);
            market_table.add_row(vec!["Active Orders", &summary.active_orders.to_string()]);
            market_table.add_row(vec!["  ├─ Buy Orders", &summary.buy_orders.to_string()]);
            market_table.add_row(vec!["  └─ Sell Orders", &summary.sell_orders.to_string()]);
            market_table.add_row(vec![
                "Avg Ore Price",
                &format!("{:.2}", summary.avg_ore_price),
            ]);

            // Add refined material prices
            let mut ingot_prices: Vec<_> = summary.ingot_prices.iter().collect();
            ingot_prices.sort_by_key(|(name, _)| *name);
            for (name, price) in ingot_prices {
                market_table.add_row(vec![
                    &format!("  Price: {}", name),
                    &format!("{:.2}", price),
                ]);
            }

            // --- Table 3: Economy & Population ---
            let mut economy_table = Table::new();
            economy_table.load_preset(UTF8_FULL_CONDENSED);
            economy_table.set_header(vec!["Economy Metric", "Value"]);
            economy_table.add_row(vec![
                "Total Population",
                &summary.total_population.to_string(),
            ]);
            economy_table.add_row(vec![
                "Food in Circulation",
                &summary.total_food_inventory.to_string(),
            ]);
            economy_table.add_row(vec![
                "Active Plantations",
                &summary.active_plantations.to_string(),
            ]);
            economy_table.add_row(vec![
                "Active Events",
                &summary.total_active_events.to_string(),
            ]);

            // --- Table 4: Companies ---
            let mut company_table = Table::new();
            company_table.load_preset(UTF8_FULL_CONDENSED);
            company_table.set_header(vec!["Company Type", "Count"]);
            company_table.add_row(vec![
                "Total Companies",
                &summary.total_companies.to_string(),
            ]);

            let mut company_types: Vec<_> = summary.company_breakdown.iter().collect();
            company_types.sort_by_key(|(name, _)| *name);
            for (company_type, count) in company_types {
                company_table.add_row(vec![&format!("  ├─ {}", company_type), &count.to_string()]);
            }

            info!("\n╔════════════════════════════════════════════════════════════════════════╗");
            info!(
                "║                        ECONOMIC PULSE - TICK {}                         ║",
                summary.tick
            );
            info!("╚════════════════════════════════════════════════════════════════════════╝");
            info!("\n📊 FINANCIAL SUMMARY\n{financial_table}");
            info!("\n💱 MARKET ACTIVITY\n{market_table}");
            info!("\n🌍 ECONOMY & POPULATION\n{economy_table}");
            info!("\n🏢 COMPANY BREAKDOWN\n{company_table}");

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
                "UPDATE companies SET cash = $1, debt = $2, next_eval_tick = $3, status = $4, last_trade_tick = $5 WHERE id = $6",
            )
            .bind(company.cash)
            .bind(company.debt)
            .bind(company.next_eval_tick as i64)
            .bind(&company.status)
            .bind(company.last_trade_tick as i64)
            .bind(company.id)
            .execute(&mut *tx)
            .await?;
        }

        // ── Loans ─────────────────────────────────────────────────────────────
        // Use UPSERT so that loans created in-memory during the tick are also persisted
        // and mutable columns (balance, interest_rate) are kept up to date.
        for loan in self.loans.values() {
            sqlx::query(
                "INSERT INTO loans (id, company_id, lender_company_id, principal, interest_rate, balance)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (id) DO UPDATE SET
                     interest_rate = EXCLUDED.interest_rate,
                     balance = EXCLUDED.balance",
            )
            .bind(loan.id)
            .bind(loan.company_id)
            .bind(loan.lender_company_id)
            .bind(loan.principal)
            .bind(loan.interest_rate)
            .bind(loan.balance)
            .execute(&mut *tx)
            .await?;
        }

        // ── Bank Accounts ─────────────────────────────────────────────────────
        for account in self.bank_accounts.values() {
            sqlx::query("UPDATE bank_accounts SET balance = $1, interest_rate = $2 WHERE id = $3")
                .bind(account.balance)
                .bind(account.interest_rate)
                .bind(account.id)
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
                "INSERT INTO facilities (id, city_id, company_id, facility_type, capacity, setup_ticks_remaining, target_resource_id, production_ratios)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT (id) DO UPDATE SET
                    capacity = EXCLUDED.capacity,
                    setup_ticks_remaining = EXCLUDED.setup_ticks_remaining,
                    target_resource_id = EXCLUDED.target_resource_id,
                    production_ratios = EXCLUDED.production_ratios",
            )
            .bind(facility.id)
            .bind(facility.city_id)
            .bind(facility.company_id)
            .bind(&facility.facility_type)
            .bind(facility.capacity)
            .bind(facility.setup_ticks_remaining as i32)
            .bind(facility.target_resource_id)
            .bind(ratios_json)
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

        // ── Diplomatic Relations ──────────────────────────────────────────────
        for rel in self.diplomatic_relations.values() {
            sqlx::query(
                "INSERT INTO diplomatic_relations (empire_a_id, empire_b_id, tension, status)
                 VALUES ($1, $2, $3, $4)
                 ON CONFLICT (empire_a_id, empire_b_id)
                 DO UPDATE SET tension = EXCLUDED.tension, status = EXCLUDED.status",
            )
            .bind(rel.empire_a_id)
            .bind(rel.empire_b_id)
            .bind(rel.tension)
            .bind(&rel.status)
            .execute(&mut *tx)
            .await?;
        }

        // ── Active Events ─────────────────────────────────────────────────────
        sqlx::query("DELETE FROM active_events")
            .execute(&mut *tx)
            .await?;

        for event in self.active_events.values() {
            // Persist both target_id components so paired targets (e.g. blockade lanes
            // identified by two system IDs) can be reconstructed after reload.
            let encoded_target_id = event.target_id.map(|(a, _)| a);
            let encoded_target_id_b = event.target_id.map(|(_, b)| b);

            sqlx::query(
                "INSERT INTO active_events (id, event_type, target_id, target_id_b, severity, start_tick, end_tick, flavor_text)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            )
            .bind(event.id)
            .bind(&event.event_type)
            .bind(encoded_target_id)
            .bind(encoded_target_id_b)
            .bind(event.severity)
            .bind(event.start_tick as i64)
            .bind(event.end_tick as i64)
            .bind(&event.flavor_text)
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
