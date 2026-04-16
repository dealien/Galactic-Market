use sqlx::PgPool;
use tracing::info;
use crate::sim::namegen::{self, LocationType};
use rand::thread_rng;

pub async fn run_seed(pool: &PgPool) -> Result<(), sqlx::Error> {
    info!("Seeding universe...");

    // Initialize name dictionary
    if let Err(e) = namegen::init_dictionary("data/names.json") {
        info!("Name dictionary already initialized or failed: {}", e);
    }

    let mut rng = thread_rng();

    // Check if empires exist; skip if already seeded
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM empires")
        .fetch_one(pool)
        .await?;

    if count.0 > 0 {
        info!("Database already seeded, skipping.");
        return Ok(());
    }

    let mut tx = pool.begin().await?;

    // 1. Resource Types
    let iron_ore_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Iron Ore").bind("Raw Material").bind(100.0).bind(true)
    .fetch_one(&mut *tx).await?.0;

    let copper_ore_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Copper Ore").bind("Raw Material").bind(120.0).bind(true)
    .fetch_one(&mut *tx).await?.0;

    let tin_ore_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Tin Ore").bind("Raw Material").bind(150.0).bind(true)
    .fetch_one(&mut *tx).await?.0;

    let iron_ingot_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Iron Ingot").bind("Refined Material").bind(150.0).bind(true)
    .fetch_one(&mut *tx).await?.0;

    let copper_ingot_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Copper Ingot").bind("Refined Material").bind(180.0).bind(true)
    .fetch_one(&mut *tx).await?.0;

    let tin_ingot_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Tin Ingot").bind("Refined Material").bind(220.0).bind(true)
    .fetch_one(&mut *tx).await?.0;

    sqlx::query(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4)"
    )
    .bind("Food Rations").bind("Consumer Good").bind(1.0).bind(true)
    .execute(&mut *tx).await?;

    info!("Seeded resource types.");

    // 2. Empires
    let empire_ids = [
        sqlx::query_as::<_, (i32,)>(
            "INSERT INTO empires (name, government_type, currency, tax_rate_base) VALUES ($1, $2, $3, $4) RETURNING id"
        )
        .bind("The Republic").bind("Democracy").bind("Credits").bind(0.15)
        .fetch_one(&mut *tx).await?.0,

        sqlx::query_as::<_, (i32,)>(
            "INSERT INTO empires (name, government_type, currency, tax_rate_base) VALUES ($1, $2, $3, $4) RETURNING id"
        )
        .bind("Frontier Syndicate").bind("Corporate").bind("Scrip").bind(0.05)
        .fetch_one(&mut *tx).await?.0,
    ];

    // 3. Sectors (1 per empire = 2)
    let sector_ids = [
        sqlx::query_as::<_, (i32,)>(
            "INSERT INTO sectors (empire_id, name, strategic_value) VALUES ($1, $2, $3) RETURNING id"
        )
        .bind(empire_ids[0]).bind("Core Sector Alpha").bind(10.0)
        .fetch_one(&mut *tx).await?.0,

        sqlx::query_as::<_, (i32,)>(
            "INSERT INTO sectors (empire_id, name, strategic_value) VALUES ($1, $2, $3) RETURNING id"
        )
        .bind(empire_ids[1]).bind("Outer Rim Gamma").bind(5.0)
        .fetch_one(&mut *tx).await?.0,
    ];

    // 4. Star Systems (2 per sector = 4)
    let mut system_ids = Vec::new();
    let mut system_names = Vec::new();
    for (i, sector_id) in sector_ids.iter().enumerate() {
        for _ in 1..=2 {
            let loc_type = if i == 0 { LocationType::Core } else { LocationType::Outpost };
            let name = namegen::generate_system_name(loc_type, &mut rng);
            let id = sqlx::query_as::<_, (i32,)>(
                "INSERT INTO star_systems (sector_id, name, star_type, resource_modifier) VALUES ($1, $2, $3, $4) RETURNING id"
            )
            .bind(sector_id).bind(&name).bind("G-Type").bind(1.0)
            .fetch_one(&mut *tx).await?.0;
            system_ids.push(id);
            system_names.push(name);
        }
    }

    // 5. Celestial Bodies (2 per system = 8)
    let mut body_ids = Vec::new();
    for (i, system_id) in system_ids.iter().enumerate() {
        let loc_type = if i < 2 { LocationType::Core } else { LocationType::Outpost };
        for _ in 1..=2 {
            let name = namegen::generate_planet_name(loc_type, &mut rng);
            let id = sqlx::query_as::<_, (i32,)>(
                "INSERT INTO celestial_bodies (system_id, name, body_type, mass, habitable, population_cap) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
            )
            .bind(system_id).bind(&name).bind("Terrestrial").bind(5.97e24).bind(true).bind(10_000_000_000_i64)
            .fetch_one(&mut *tx).await?.0;
            body_ids.push(id);
        }
    }

    // 6. Cities (4 per planet = 32); collect their IDs for company seeding
    let mut city_ids = Vec::new();
    let mut city_names = Vec::new();
    for (i, body_id) in body_ids.iter().enumerate() {
        let loc_type = if i < 4 { LocationType::Core } else { LocationType::Outpost };
        for j in 1..=4 {
            // Tiered ports: First city of each planet is a Hub (higher throughput, lower fee)
            let (fee, throughput, tier) = if j == 1 {
                (0.05, 50000, 3)
            } else {
                (0.15, 10000, 1)
            };

            let name = namegen::generate_city_name(loc_type, &mut rng);
            let city_id = sqlx::query_as::<_, (i32,)>(
                "INSERT INTO cities (body_id, name, population, infrastructure_lvl, port_tier, port_fee_per_unit, port_max_throughput) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id"
            )
            .bind(body_id).bind(&name).bind(1_000_000_i64).bind(1).bind(tier).bind(fee).bind(throughput)
            .fetch_one(&mut *tx).await?.0;
            city_ids.push(city_id);
            city_names.push(name);
        }
    }

    info!("Seeded geography: 2 empires, {} systems, {} planets, 32 cities.", system_ids.len(), body_ids.len());

    // 6.1 Seed System Lanes (Structured Ring Topology for Debugging)
    // 1 -> 2, 2 -> 3, 3 -> 4, 4 -> 1
    for i in 0..system_ids.len() {
        let sys_a = system_ids[i];
        let sys_b = system_ids[(i + 1) % system_ids.len()];
        sqlx::query(
            "INSERT INTO system_lanes (system_a_id, system_b_id, distance_ly) VALUES ($1, $2, 5.0)",
        )
        .bind(sys_a)
        .bind(sys_b)
        .execute(&mut *tx)
        .await?;
    }
    info!("Seeded structured jump lane network (Ring).");

    // 7. Recipes
    let iron_recipe_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO recipes (name, output_resource_id, output_qty, facility_type, time_ticks) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind("Iron Ingot Smelting").bind(iron_ingot_id).bind(1).bind("refinery").bind(1)
    .fetch_one(&mut *tx).await?.0;

    sqlx::query(
        "INSERT INTO recipe_inputs (recipe_id, resource_type_id, quantity) VALUES ($1, $2, $3)",
    )
    .bind(iron_recipe_id)
    .bind(iron_ore_id)
    .bind(3)
    .execute(&mut *tx)
    .await?;

    let copper_recipe_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO recipes (name, output_resource_id, output_qty, facility_type, time_ticks) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind("Copper Ingot Smelting").bind(copper_ingot_id).bind(1).bind("refinery").bind(1)
    .fetch_one(&mut *tx).await?.0;

    sqlx::query(
        "INSERT INTO recipe_inputs (recipe_id, resource_type_id, quantity) VALUES ($1, $2, $3)",
    )
    .bind(copper_recipe_id)
    .bind(copper_ore_id)
    .bind(3)
    .execute(&mut *tx)
    .await?;

    let tin_recipe_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO recipes (name, output_resource_id, output_qty, facility_type, time_ticks) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind("Tin Ingot Smelting").bind(tin_ingot_id).bind(1).bind("refinery").bind(1)
    .fetch_one(&mut *tx).await?.0;

    sqlx::query(
        "INSERT INTO recipe_inputs (recipe_id, resource_type_id, quantity) VALUES ($1, $2, $3)",
    )
    .bind(tin_recipe_id)
    .bind(tin_ore_id)
    .bind(3)
    .execute(&mut *tx)
    .await?;

    // 8. Seed one freelancer mining company per city + startup loan + mine + deposit
    //    Sector capital cities (first city of first planet per system) also get a refinery.
    let startup_loan_amount = 50_000.0_f64;
    let loan_interest_rate = 0.05_f64;

    // The "sector capital" is the first city of the first planet in each system (index 0, 4, 8, 12)
    let sector_capital_indices: Vec<usize> = vec![0, 4, 8, 12];

    for (idx, &city_id) in city_ids.iter().enumerate() {
        // Which planet does this city belong to?
        let body_id = body_ids[idx / 4];
        let loc_type = if idx < 16 { LocationType::Core } else { LocationType::Outpost };

        // Create the mining company
        let company_name = namegen::generate_company_name(loc_type, &mut rng);
        let company_id = sqlx::query_as::<_, (i32,)>(
            "INSERT INTO companies (name, company_type, home_city_id, cash, debt, credit_rating, next_eval_tick, status, last_trade_tick) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id"
        )
        .bind(&company_name).bind("freelancer").bind(city_id)
        .bind(0.0_f64)   // cash = 0; funded by loan
        .bind(0.0_f64)   // debt = 0; they start clean
        .bind("B")
        .bind(1_i64)
        .bind("active")
        .bind(0_i64)
        .fetch_one(&mut *tx).await?.0;

        // Give the company its startup loan; initial cash float equals loan principal
        sqlx::query(
            "INSERT INTO loans (company_id, principal, interest_rate, balance) VALUES ($1, $2, $3, $4)"
        )
        .bind(company_id).bind(startup_loan_amount).bind(loan_interest_rate).bind(startup_loan_amount)
        .execute(&mut *tx).await?;

        // Seed the loan proceeds into cash
        sqlx::query("UPDATE companies SET cash = $1 WHERE id = $2")
            .bind(startup_loan_amount)
            .bind(company_id)
            .execute(&mut *tx)
            .await?;

        // Create an Iron Ore, Copper Ore, and Tin Ore deposit on this planet
        // Only create once per planet (on the first company of each planet group)
        if idx % 4 == 0 {
            sqlx::query(
                "INSERT INTO deposits (body_id, resource_type_id, size_total, size_remaining, extraction_cost_per_unit, discovered) VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(body_id).bind(iron_ore_id).bind(1_000_000_i64).bind(1_000_000_i64).bind(2.0_f64).bind(true)
            .execute(&mut *tx).await?;

            sqlx::query(
                "INSERT INTO deposits (body_id, resource_type_id, size_total, size_remaining, extraction_cost_per_unit, discovered) VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(body_id).bind(copper_ore_id).bind(750_000_i64).bind(750_000_i64).bind(2.5_f64).bind(true)
            .execute(&mut *tx).await?;

            sqlx::query(
                "INSERT INTO deposits (body_id, resource_type_id, size_total, size_remaining, extraction_cost_per_unit, discovered) VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(body_id).bind(tin_ore_id).bind(500_000_i64).bind(500_000_i64).bind(3.0_f64).bind(true)
            .execute(&mut *tx).await?;
        }

        // Create a mine facility for each resource type for this company in its home city.
        sqlx::query(
            "INSERT INTO facilities (city_id, company_id, facility_type, capacity, target_resource_id) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(city_id).bind(company_id).bind("mine").bind(10).bind(iron_ore_id)
        .execute(&mut *tx).await?;

        sqlx::query(
            "INSERT INTO facilities (city_id, company_id, facility_type, capacity, target_resource_id) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(city_id).bind(company_id).bind("mine").bind(10).bind(copper_ore_id)
        .execute(&mut *tx).await?;

        sqlx::query(
            "INSERT INTO facilities (city_id, company_id, facility_type, capacity, target_resource_id) VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(city_id).bind(company_id).bind("mine").bind(10).bind(tin_ore_id)
        .execute(&mut *tx).await?;

        // Sector capitals also get a refinery (owned by the local company in this seed)
        if sector_capital_indices.contains(&idx) {
            let initial_ratios = serde_json::json!({
                iron_recipe_id.to_string(): 0.34,
                copper_recipe_id.to_string(): 0.33,
                tin_recipe_id.to_string(): 0.33
            });
            sqlx::query(
                "INSERT INTO facilities (city_id, company_id, facility_type, capacity, production_ratios) VALUES ($1, $2, $3, $4, $5)"
            )
            .bind(city_id).bind(company_id).bind("refinery").bind(15).bind(initial_ratios)
            .execute(&mut *tx).await?;
        }
    }

    info!(
        "Seeded 32 freelancer companies with startup loans, mine facilities, and Ore deposits (Iron, Copper, Tin)."
    );

    // 9. Seed one consumer company per city representing local population demand.
    //    Each consumer is funded by a per-capita city treasury (population × 10 credits).
    //    They don't receive loans — they represent collective purchasing power.
    for (idx, &city_id) in city_ids.iter().enumerate() {
        // Fetch city population for this city_id
        let (pop,): (i64,) = sqlx::query_as("SELECT population FROM cities WHERE id = $1")
            .bind(city_id)
            .fetch_one(&mut *tx)
            .await?;

        let treasury = pop as f64 * 10.0; // starting credits = population × 10
        let company_name = format!("{} Consumers", city_names[idx]);

        sqlx::query(
            "INSERT INTO companies (name, company_type, home_city_id, cash, debt, credit_rating, next_eval_tick, status, last_trade_tick)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(&company_name)
        .bind("consumer")
        .bind(city_id)
        .bind(treasury)
        .bind(0.0_f64)
        .bind("A") // consumers are always good for their purchases
        .bind(1_i64)
        .bind("active")
        .bind(0_i64)
        .execute(&mut *tx)
        .await?;
    }

    info!("Seeded 32 consumer companies (one per city).");

    // 9. Seed 4 Merchant companies (Arbitrageurs) - one per star system
    for (i, &_system_id) in system_ids.iter().enumerate() {
        // Find the first city in this system to place the merchant's home office
        let city_id = city_ids[i * 8];

        sqlx::query(
            "INSERT INTO companies (name, company_type, home_city_id, cash, debt, credit_rating, next_eval_tick, status, last_trade_tick)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(format!("{} Merchant", system_names[i]))
        .bind("merchant")
        .bind(city_id)
        .bind(100_000.0_f64) // High initial capital for arbitrage
        .bind(0.0_f64)
        .bind("A")
        .bind(1_i64)
        .bind("active")
        .bind(0_i64)
        .execute(&mut *tx)
        .await?;
    }
    info!("Seeded 4 merchant arbitrageurs.");

    // 10. Prime the market with initial prices to prevent discovery deadlock
    for &city_id in &city_ids {
        for &(res_id, base_price) in &[
            (iron_ore_id, 3.0),
            (copper_ore_id, 3.5),
            (tin_ore_id, 4.0),
            (iron_ingot_id, 45.0),
            (copper_ingot_id, 55.0),
            (tin_ingot_id, 65.0),
        ] {
            sqlx::query(
                "INSERT INTO market_history (city_id, resource_type_id, tick, open, high, low, close, volume)
                 VALUES ($1, $2, 0, $3, $3, $3, $3, 100)"
            )
            .bind(city_id)
            .bind(res_id)
            .bind(base_price)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    info!("Seeding complete! Universe is ready.");

    Ok(())
}
