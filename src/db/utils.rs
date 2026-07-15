//! Database utility helpers.
//!
//! Provides functions for database management during development, such as resetting
//! the schema or clearing all simulated data.

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
///
/// # Examples
/// ```no_run
/// use sqlx::PgPool;
/// use galactic_market::db::utils::clear_database;
///
/// #[tokio::main]
/// async fn main() -> Result<(), sqlx::Error> {
///     let pool = PgPool::connect("postgres://postgres:password@localhost:5432/galactic_market").await?;
///     clear_database(&pool).await?;
///     Ok(())
/// }
/// ```
pub async fn clear_database(pool: &PgPool) -> Result<(), sqlx::Error> {
    info!("Clearing database: dropping and recreating public schema...");

    // Drop the public schema cascade to clean up all tables and views
    sqlx::query("DROP SCHEMA public CASCADE")
        .execute(pool)
        .await?;

    // Recreate the schema fresh
    sqlx::query("CREATE SCHEMA public").execute(pool).await?;

    info!("Database cleared. Schema is fresh.");
    Ok(())
}
