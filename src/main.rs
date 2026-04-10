use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use std::env;
use tracing::{Level, info};

use galactic_market::db;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Number of ticks to simulate
    #[arg(short, long, default_value_t = 10000)]
    ticks: u64,

    /// Seed the initial galaxy before running
    #[arg(short, long)]
    seed: bool,

    /// Wipe the database before running (drops and recreates the public schema)
    #[arg(long)]
    clear: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing from RUST_LOG environment variable, defaulting to INFO
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .init();

    // Load .env file configurations
    dotenvy::dotenv().ok();

    let args = Args::parse();

    info!("Starting Galactic Market Simulator (Stage 1)");

    let database_url = env::var("DATABASE_URL")?;

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    info!("Connected to PostgreSQL.");

    // Optionally wipe the DB before running migrations
    if args.clear {
        db::utils::clear_database(&pool).await?;
    }

    // Run migrations
    info!("Running migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;

    if args.seed {
        db::seed::run_seed(&pool).await?;
    }

    // Load full simulation state from DB into memory
    info!("Loading simulation state from database...");
    let mut state = db::load::load(&pool).await?;

    // Run tick loop
    for _ in 0..args.ticks {
        state.run_tick(&pool).await?;
    }

    info!("Simulation complete. Ran {} ticks.", args.ticks);

    Ok(())
}
