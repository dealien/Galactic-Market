use sqlx::PgPool;
use tracing::info;

pub async fn run_seed(pool: &PgPool) -> Result<(), sqlx::Error> {
    info!("Seeding universe...");

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

    let iron_ingot_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO resource_types (name, category, base_mass_kg, stackable) VALUES ($1, $2, $3, $4) RETURNING id"
    )
    .bind("Iron Ingot").bind("Refined Material").bind(150.0).bind(true)
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
    for (i, sector_id) in sector_ids.iter().enumerate() {
        for j in 1..=2 {
            let id = sqlx::query_as::<_, (i32,)>(
                "INSERT INTO star_systems (sector_id, name, star_type, resource_modifier) VALUES ($1, $2, $3, $4) RETURNING id"
            )
            .bind(sector_id).bind(format!("System {}-{}", i + 1, j)).bind("G-Type").bind(1.0)
            .fetch_one(&mut *tx).await?.0;
            system_ids.push(id);
        }
    }

    // 5. Celestial Bodies (2 per system = 8)
    let mut body_ids = Vec::new();
    for (i, system_id) in system_ids.iter().enumerate() {
        for j in 1..=2 {
            let id = sqlx::query_as::<_, (i32,)>(
                "INSERT INTO celestial_bodies (system_id, name, body_type, mass, habitable, population_cap) VALUES ($1, $2, $3, $4, $5, $6) RETURNING id"
            )
            .bind(system_id).bind(format!("Planet {}-{}", i + 1, j)).bind("Terrestrial").bind(5.97e24).bind(true).bind(10_000_000_000_i64)
            .fetch_one(&mut *tx).await?.0;
            body_ids.push(id);
        }
    }

    // 6. Cities (4 per planet = 32); collect their IDs for company seeding
    let mut city_ids = Vec::new();
    for (i, body_id) in body_ids.iter().enumerate() {
        for j in 1..=4 {
            let city_id = sqlx::query_as::<_, (i32,)>(
                "INSERT INTO cities (body_id, name, population, infrastructure_lvl, port_tier) VALUES ($1, $2, $3, $4, $5) RETURNING id"
            )
            .bind(body_id).bind(format!("City {}-{}", i + 1, j)).bind(1_000_000_i64).bind(1).bind(1)
            .fetch_one(&mut *tx).await?.0;
            city_ids.push(city_id);
        }
    }

    info!("Seeded geography: 2 empires, 4 systems, 8 planets, 32 cities.");

    // 7. Iron Ingot recipe: 3 Iron Ore → 1 Iron Ingot
    let recipe_id = sqlx::query_as::<_, (i32,)>(
        "INSERT INTO recipes (name, output_resource_id, output_qty, facility_type, time_ticks) VALUES ($1, $2, $3, $4, $5) RETURNING id"
    )
    .bind("Iron Ingot Smelting").bind(iron_ingot_id).bind(1).bind("refinery").bind(1)
    .fetch_one(&mut *tx).await?.0;

    sqlx::query(
        "INSERT INTO recipe_inputs (recipe_id, resource_type_id, quantity) VALUES ($1, $2, $3)"
    )
    .bind(recipe_id).bind(iron_ore_id).bind(3)
    .execute(&mut *tx).await?;

    // 8. Seed one freelancer mining company per city + startup loan + mine + deposit
    //    Sector capital cities (first city of first planet per system) also get a refinery.
    let startup_loan_amount = 10_000.0_f64;
    let loan_interest_rate = 0.05_f64;

    // The "sector capital" is the first city of the first planet in each system (index 0, 4, 8, 12)
    let sector_capital_indices: Vec<usize> = vec![0, 4, 8, 12];

    for (idx, &city_id) in city_ids.iter().enumerate() {
        // Which planet does this city belong to?
        let body_id = body_ids[idx / 4];

        // Create the mining company
        let company_name = format!("Freelancer Mining Co. #{}", idx + 1);
        let company_id = sqlx::query_as::<_, (i32,)>(
            "INSERT INTO companies (name, company_type, home_city_id, cash, debt, credit_rating, next_eval_tick) VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id"
        )
        .bind(&company_name).bind("freelancer").bind(city_id)
        .bind(0.0_f64)   // cash = 0; funded by loan
        .bind(startup_loan_amount)
        .bind("B")
        .bind(1_i64)
        .fetch_one(&mut *tx).await?.0;

        // Give the company its startup loan; initial cash float equals loan principal
        sqlx::query(
            "INSERT INTO loans (company_id, principal, interest_rate, balance) VALUES ($1, $2, $3, $4)"
        )
        .bind(company_id).bind(startup_loan_amount).bind(loan_interest_rate).bind(startup_loan_amount)
        .execute(&mut *tx).await?;

        // Seed the loan proceeds into cash
        sqlx::query("UPDATE companies SET cash = $1 WHERE id = $2")
            .bind(startup_loan_amount).bind(company_id)
            .execute(&mut *tx).await?;

        // Create an Iron Ore deposit on this planet (shared per planet, 1M units each)
        // Only create once per planet (on the first company of each planet group)
        if idx % 4 == 0 {
            sqlx::query(
                "INSERT INTO deposits (body_id, resource_type_id, size_total, size_remaining, extraction_cost_per_unit, discovered) VALUES ($1, $2, $3, $4, $5, $6)"
            )
            .bind(body_id).bind(iron_ore_id).bind(1_000_000_i64).bind(1_000_000_i64).bind(2.0_f64).bind(true)
            .execute(&mut *tx).await?;
        }

        // Create a mine facility for this company in its home city
        sqlx::query(
            "INSERT INTO facilities (city_id, company_id, facility_type, capacity) VALUES ($1, $2, $3, $4)"
        )
        .bind(city_id).bind(company_id).bind("mine").bind(10)
        .execute(&mut *tx).await?;

        // Sector capitals also get a refinery (owned by the local company in this seed)
        if sector_capital_indices.contains(&idx) {
            sqlx::query(
                "INSERT INTO facilities (city_id, company_id, facility_type, capacity) VALUES ($1, $2, $3, $4)"
            )
            .bind(city_id).bind(company_id).bind("refinery").bind(5)
            .execute(&mut *tx).await?;
        }
    }

    info!("Seeded 32 freelancer companies with startup loans, mine facilities, and Iron Ore deposits.");

    // 9. Seed one consumer company per city representing local population demand.
    //    Each consumer is funded by a per-capita city treasury (population × 10 credits).
    //    They don't receive loans — they represent collective purchasing power.
    for &city_id in &city_ids {
        // Fetch city population for this city_id
        let (pop,): (i64,) = sqlx::query_as("SELECT population FROM cities WHERE id = $1")
            .bind(city_id)
            .fetch_one(&mut *tx)
            .await?;

        let treasury = pop as f64 * 10.0; // starting credits = population × 10
        let company_name = format!("City {} Consumers", city_id);

        sqlx::query(
            "INSERT INTO companies (name, company_type, home_city_id, cash, debt, credit_rating, next_eval_tick)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(&company_name)
        .bind("consumer")
        .bind(city_id)
        .bind(treasury)
        .bind(0.0_f64)
        .bind("A") // consumers are always good for their purchases
        .bind(1_i64)
        .execute(&mut *tx)
        .await?;
    }

    info!("Seeded 32 consumer companies (one per city).");

    tx.commit().await?;
    info!("Seeding complete! Universe is ready.");

    Ok(())
}
