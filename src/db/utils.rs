use sqlx::PgPool;
use tracing::info;

/// Drop and recreate the public schema, wiping all tables and data.
///
/// This is the equivalent of `docker-compose down -v && docker-compose up -d`
/// but without restarting the container. Useful during development to reset
/// the simulation state without touching Docker.
///
/// # Warning
/// This is **destructive** and irreversible. Never call in production.
pub async fn clear_database(pool: &PgPool) -> Result<(), sqlx::Error> {
    info!("Clearing database: dropping and recreating public schema...");

    sqlx::query("DROP SCHEMA public CASCADE")
        .execute(pool)
        .await?;

    sqlx::query("CREATE SCHEMA public")
        .execute(pool)
        .await?;

    info!("Database cleared. Schema is fresh.");
    Ok(())
}
