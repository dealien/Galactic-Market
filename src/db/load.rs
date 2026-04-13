use sqlx::PgPool;
use tracing::info;

use crate::sim::state::{
    City, Company, Deposit, Facility, Inventory, Recipe, RecipeInput, SimState,
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
    let rows = sqlx::query_as::<_, (i32, i32, String, i64)>(
        "SELECT c.id, cb.id AS body_id, c.name, c.population
         FROM cities c
         JOIN celestial_bodies cb ON cb.id = c.body_id",
    )
    .fetch_all(pool)
    .await?;

    for (id, body_id, name, population) in rows {
        state.cities.insert(
            id,
            City {
                id,
                body_id,
                name,
                population,
            },
        );
    }

    info!(count = state.cities.len(), "Loaded cities.");

    // ── Companies ─────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, String, String, i32, f64, f64, i64)>(
        "SELECT id, name, company_type, home_city_id, cash, debt, next_eval_tick FROM companies",
    )
    .fetch_all(pool)
    .await?;

    for (id, name, company_type, home_city_id, cash, debt, next_eval_tick) in rows {
        state.companies.insert(
            id,
            Company {
                id,
                name,
                company_type,
                home_city_id,
                cash,
                debt,
                next_eval_tick: next_eval_tick as u64,
            },
        );
    }

    info!(count = state.companies.len(), "Loaded companies.");

    // ── Loans ─────────────────────────────────────────────────────────────────
    let rows = sqlx::query_as::<_, (i32, i32, f64, f64, f64)>(
        "SELECT id, company_id, principal, interest_rate, balance FROM loans",
    )
    .fetch_all(pool)
    .await?;

    for (id, company_id, principal, interest_rate, balance) in rows {
        state.loans.insert(
            id,
            crate::sim::state::Loan {
                id,
                company_id,
                principal,
                interest_rate,
                balance,
            },
        );
    }

    info!(count = state.loans.len(), "Loaded loans.");

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

    // ── Inventories ───────────────────────────────────────────────────────────
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

    Ok(state)
}
