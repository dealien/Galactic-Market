import re

with open('tests/sim_integration_test.rs', 'r') as f:
    content = f.read()

# Make it fully dynamic to follow the instruction perfectly.
new_test = """async fn test_database_seeding_creates_records() -> Result<(), anyhow::Error> {
    let _ = dotenvy::dotenv().ok();
    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL not set");
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    // Clear db
    galactic_market::db::utils::clear_database(&pool).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;

    let empires_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM empires").fetch_one(&pool).await?;
    let cities_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cities").fetch_one(&pool).await?;
    let companies_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM companies").fetch_one(&pool).await?;
    let resources_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM resource_types").fetch_one(&pool).await?;
    let facilities_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM facilities").fetch_one(&pool).await?;
    let deposits_before: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM deposits").fetch_one(&pool).await?;

    // Run seed
    galactic_market::db::seed::run_seed(&pool).await?;

    let empires_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM empires").fetch_one(&pool).await?;
    let cities_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM cities").fetch_one(&pool).await?;
    let companies_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM companies").fetch_one(&pool).await?;
    let resources_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM resource_types").fetch_one(&pool).await?;
    let facilities_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM facilities").fetch_one(&pool).await?;
    let deposits_after: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM deposits").fetch_one(&pool).await?;

    assert!(empires_after.0 > empires_before.0, "Should have seeded empires");
    assert!(cities_after.0 > cities_before.0, "Should have seeded cities");
    assert!(companies_after.0 > companies_before.0, "Should have seeded companies");
    assert!(resources_after.0 > resources_before.0, "Should have seeded resource types");
    assert!(facilities_after.0 > facilities_before.0, "Should have seeded facilities");
    assert!(deposits_after.0 > deposits_before.0, "Should have seeded deposits");

    Ok(())
}"""

content = re.sub(r'async fn test_database_seeding_creates_records\(\) -> Result<\(\), anyhow::Error> \{.*?\n\}', new_test, content, flags=re.DOTALL)

with open('tests/sim_integration_test.rs', 'w') as f:
    f.write(content)
print("done")
