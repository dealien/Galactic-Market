use sqlx::PgPool;
use tracing::info;

use crate::sim::state::{
    ActiveEvent, BankAccount, City, Company, Deposit, DiplomaticRelation, Empire, Facility, Inventory,
    Recipe, RecipeInput, SimState,
};

/// Load the full simulation state from the database into memory.
///
/// This is called once at startup after migrations have run. The returned
/// `SimState` is the authoritative in-memory world for the tick loop.
///
/// # Errors
/// Returns a `sqlx::Error` if any query fails.
pub async fn load(pool: &PgPool) -> Result<SimState, sqlx::Error> {
    let mut state = SimState::new();

    // ── Cities ────────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, String, i64, i32, f64, i64)>(
        "SELECT id, body_id, name, population, port_tier, port_fee_per_unit, port_max_throughput FROM cities",
    )
    .fetch_all(pool)
    .await?;

    for (id, body_id, name, population, port_tier, port_fee_per_unit, port_max_throughput) in rows {
        state.cities.insert(
            id,
            City {
                id,
                body_id,
                name,
                population,
                port_tier,
                port_fee_per_unit,
                port_max_throughput,
            },
        );
    }

    info!(count = state.cities.len(), "Loaded cities.");

    // ── System Lanes ──────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, f64, String)>(
        "SELECT system_a_id, system_b_id, distance_ly, lane_type FROM system_lanes",
    )
    .fetch_all(pool)
    .await?;

    for (sys_a, sys_b, dist, lane_type) in rows {
        state.system_lanes.insert(
            (sys_a, sys_b),
            crate::sim::state::SystemLane {
                system_a_id: sys_a,
                system_b_id: sys_b,
                distance_ly: dist,
                lane_type,
            },
        );
    }

    crate::sim::logistics::build_system_distances(&mut state);

    info!(count = state.system_lanes.len(), "Loaded system lanes.");

    // ── Celestial Bodies ──────────────────────────────────────────────────────
    let rows =
        sqlx::query_as::<_, (i32, i32, String)>("SELECT id, system_id, name FROM celestial_bodies")
            .fetch_all(pool)
            .await?;

    for (id, system_id, name) in rows {
        state.celestial_bodies.insert(
            id,
            crate::sim::state::CelestialBody {
                id,
                system_id,
                name,
            },
        );
    }

    info!(
        count = state.celestial_bodies.len(),
        "Loaded celestial bodies."
    );

    // ── Star Systems ──────────────────────────────────────────────────────────
    let rows =
        sqlx::query_as::<_, (i32, i32, String)>("SELECT id, sector_id, name FROM star_systems")
            .fetch_all(pool)
            .await?;

    for (id, sector_id, name) in rows {
        state.star_systems.insert(
            id,
            crate::sim::state::StarSystem {
                id,
                sector_id,
                name,
            },
        );
    }

    info!(count = state.star_systems.len(), "Loaded star systems.");

    // ── Sectors ───────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, String)>("SELECT id, empire_id, name FROM sectors")
        .fetch_all(pool)
        .await?;

    for (id, empire_id, name) in rows {
        state.sectors.insert(
            id,
            crate::sim::state::Sector {
                id,
                empire_id,
                name,
            },
        );
    }

    info!(count = state.sectors.len(), "Loaded sectors.");

    // ── Empires ──────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, String, String, f64)>(
        "SELECT id, name, government_type, tax_rate_base FROM empires",
    )
    .fetch_all(pool)
    .await?;

    for (id, name, government_type, tax_rate_base) in rows {
        state.empires.insert(
            id,
            Empire {
                id,
                name,
                government_type,
                tax_rate_base,
            },
        );
    }

    info!(count = state.empires.len(), "Loaded empires.");

    // ── Diplomatic Relations ─────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, f64, String)>(
        "SELECT empire_a_id, empire_b_id, tension, status FROM diplomatic_relations",
    )
    .fetch_all(pool)
    .await?;

    for (a, b, tension, status) in rows {
        state.diplomatic_relations.insert(
            (a, b),
            DiplomaticRelation {
                empire_a_id: a,
                empire_b_id: b,
                tension,
                status,
            },
        );
    }

    info!(
        count = state.diplomatic_relations.len(),
        "Loaded diplomatic relations."
    );

    // ── Active Events ────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, String, Option<i32>, f64, i64, i64, Option<String>)>(
        "SELECT id, event_type, target_id, severity, start_tick, end_tick, flavor_text FROM active_events",
    )
    .fetch_all(pool)
    .await?;

    for (id, event_type, target_id, severity, start_tick, end_tick, flavor_text) in rows {
        state.active_events.insert(
            id,
            ActiveEvent {
                id,
                event_type,
                target_id,
                severity,
                start_tick: start_tick as u64,
                end_tick: end_tick as u64,
                flavor_text,
            },
        );
    }

    // Set next_event_id
    let max_event_id: (Option<i32>,) = sqlx::query_as("SELECT MAX(id) FROM active_events")
        .fetch_one(pool)
        .await?;
    state.next_event_id = max_event_id.0.unwrap_or(0) + 1;

    info!(count = state.active_events.len(), "Loaded active events.");

    // ── Companies ─────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, String, String, i32, f64, f64, i64, String, i64)>(
        "SELECT id, name, company_type, home_city_id, cash, debt, next_eval_tick, status, last_trade_tick FROM companies",
    )
    .fetch_all(pool)
    .await?;

    for (
        id,
        name,
        company_type,
        home_city_id,
        cash,
        debt,
        next_eval_tick,
        status,
        last_trade_tick,
    ) in rows
    {
        state.companies.insert(
            id,
            Company {
                id,
                name,
                company_type: company_type.clone(),
                home_city_id,
                cash,
                debt,
                next_eval_tick: next_eval_tick as u64,
                status,
                last_trade_tick: last_trade_tick as u64,
            },
        );
    }

    info!(count = state.companies.len(), "Loaded companies.");

    // ── Loans ─────────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, Option<i32>, f64, f64, f64)>(
        "SELECT id, company_id, lender_company_id, principal, interest_rate, balance FROM loans",
    )
    .fetch_all(pool)
    .await?;

    for (id, company_id, lender_company_id, principal, interest_rate, balance) in rows {
        state.loans.insert(
            id,
            crate::sim::state::Loan {
                id,
                company_id,
                lender_company_id,
                principal,
                interest_rate,
                balance,
            },
        );
    }

    info!(count = state.loans.len(), "Loaded loans.");

    // ── Bank Accounts ────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, i32, f64, f64)>(
        "SELECT id, company_id, bank_company_id, balance, interest_rate FROM bank_accounts",
    )
    .fetch_all(pool)
    .await?;

    for (id, company_id, bank_company_id, balance, interest_rate) in rows {
        state.bank_accounts.insert(
            id,
            BankAccount {
                id,
                company_id,
                bank_company_id,
                balance,
                interest_rate,
            },
        );
    }

    info!(count = state.bank_accounts.len(), "Loaded bank accounts.");

    // ── Deposits ──────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, i32, i64, i64, f64)>(
        "SELECT id, body_id, resource_type_id, size_total, size_remaining, extraction_cost_per_unit
         FROM deposits WHERE discovered = true",
    )
    .fetch_all(pool)
    .await?;

    for (id, body_id, resource_type_id, size_total, size_remaining, extraction_cost_per_unit) in
        rows
    {
        state.deposits.insert(
            id,
            Deposit {
                id,
                body_id,
                resource_type_id,
                size_total,
                size_remaining,
                extraction_cost_per_unit,
            },
        );
    }

    info!(count = state.deposits.len(), "Loaded deposits.");

    // ── Facilities ────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, i32, String, i32, i32, Option<i32>, Option<sqlx::types::Json<std::collections::HashMap<String, f64>>>)>(
        "SELECT id, city_id, company_id, facility_type, capacity, setup_ticks_remaining, target_resource_id, production_ratios FROM facilities",
    )
    .fetch_all(pool)
    .await?;

    for (
        id,
        city_id,
        company_id,
        facility_type,
        capacity,
        setup_ticks_remaining,
        target_resource_id,
        production_ratios,
    ) in rows
    {
        let ratios = production_ratios.map(|json| json.0);
        state.facilities.insert(
            id,
            Facility {
                id,
                city_id,
                company_id,
                facility_type,
                capacity,
                setup_ticks_remaining: setup_ticks_remaining as u32,
                target_resource_id,
                production_ratios: ratios,
            },
        );
    }

    info!(count = state.facilities.len(), "Loaded facilities.");

    // Set next_facility_id based on current max
    let max_facility_id: (Option<i32>,) = sqlx::query_as("SELECT MAX(id) FROM facilities")
        .fetch_one(pool)
        .await?;
    state.next_facility_id = max_facility_id.0.unwrap_or(0) + 1;

    // ─── Recipes ─────────────────────────────────────────────────────────────

    let rows = sqlx::query_as::<_, (i32, i32, i32, i64)>(
        "SELECT company_id, city_id, resource_type_id, quantity FROM inventory WHERE quantity > 0",
    )
    .fetch_all(pool)
    .await?;

    for (company_id, city_id, resource_type_id, quantity) in rows {
        let key = Inventory::key(company_id, city_id, resource_type_id);
        state.inventories.insert(
            key,
            Inventory {
                company_id,
                city_id,
                resource_type_id,
                quantity,
            },
        );
    }

    info!(count = state.inventories.len(), "Loaded inventories.");

    // ── Resource Types ────────────────────────────────────────────────────────
    let rows =
        sqlx::query_as::<_, (i32, String, String)>("SELECT id, name, category FROM resource_types")
            .fetch_all(pool)
            .await?;

    for (id, name, category) in rows {
        state
            .resource_types
            .insert(id, crate::sim::state::ResourceType { id, name, category });
    }

    info!(count = state.resource_types.len(), "Loaded resource types.");

    // ── Recipes ───────────────────────────────────────────────────────────────
    let recipe_rows = sqlx::query_as::<_, (i32, String, i32, i32, String)>(
        "SELECT id, name, output_resource_id, output_qty, facility_type FROM recipes",
    )
    .fetch_all(pool)
    .await?;

    for (id, name, output_resource_id, output_qty, facility_type) in recipe_rows {
        let input_rows = sqlx::query_as::<_, (i32, i32)>(
            "SELECT resource_type_id, quantity FROM recipe_inputs WHERE recipe_id = $1",
        )
        .bind(id)
        .fetch_all(pool)
        .await?;

        let inputs = input_rows
            .into_iter()
            .map(|(resource_type_id, quantity)| RecipeInput {
                resource_type_id,
                quantity,
            })
            .collect();

        state.recipes.insert(
            id,
            Recipe {
                id,
                name,
                output_resource_id,
                output_qty,
                facility_type,
                inputs,
            },
        );
    }

    info!(count = state.recipes.len(), "Loaded recipes.");

    // ── Trade Routes ──────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, i32, i32, i32, i64, i64)>(
        "SELECT id, company_id, origin_city_id, dest_city_id, resource_type_id, quantity, arrival_tick FROM trade_routes",
    )
    .fetch_all(pool)
    .await?;

    for (id, company_id, origin_city_id, dest_city_id, resource_type_id, quantity, arrival_tick) in
        rows
    {
        state.trade_routes.insert(
            id,
            crate::sim::state::TradeRoute {
                id,
                company_id,
                origin_city_id,
                dest_city_id,
                resource_type_id,
                quantity,
                arrival_tick: arrival_tick as u64,
            },
        );
    }

    info!(count = state.trade_routes.len(), "Loaded trade routes.");

    // ── Consumer company index ────────────────────────────────────────────────
    // Build a fast city_id → company_id map for the consumption phase.
    for (id, company) in &state.companies {
        if company.company_type == "consumer" {
            state.city_consumer_ids.insert(company.home_city_id, *id);
        }
    }

    info!(
        count = state.city_consumer_ids.len(),
        "Indexed consumer companies."
    );

    // ── Market Price Priming ──────────────────────────────────────────────────
    // Load the latest close price for each (city, resource) to prime the cache.
    let rows = sqlx::query_as::<_, (i32, i32, f64)>(
        "SELECT DISTINCT ON (city_id, resource_type_id) city_id, resource_type_id, close 
         FROM market_history 
         ORDER BY city_id, resource_type_id, tick DESC",
    )
    .fetch_all(pool)
    .await?;

    for (city_id, res_id, price) in rows {
        state.price_cache.insert((city_id, res_id), price);
        state.ema_prices.insert((city_id, res_id), price);
    }

    info!(
        count = state.price_cache.len(),
        "Primed market price cache."
    );

    Ok(state)
}
