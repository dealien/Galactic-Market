use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use std::env;
use tracing::info;

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

    /// Show detailed debug logs during simulation
    #[arg(long)]
    debug: bool,

    /// Random seed for reproducible runs
    #[arg(long)]
    random_seed: Option<u64>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // Initialize tracing. Default to INFO, or DEBUG if flag is set.
    let log_level = if args.debug {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env().add_directive(log_level.into()),
        )
        .init();

    // Load .env file configurations
    dotenvy::dotenv().ok();

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

    // Load event definitions from JSON
    let events_json = std::fs::read_to_string("data/events.json")?;
    let event_config: serde_json::Value = serde_json::from_str(&events_json)?;
    state.event_definitions = serde_json::from_value(event_config["events"].clone())?;
    info!(
        "Loaded {} event definitions.",
        state.event_definitions.len()
    );

    // Initialize RNG
    use rand::SeedableRng;
    let seed = args.random_seed.unwrap_or_else(rand::random);
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    info!("Simulation RNG seed: {}", seed);

    // Run tick loop
    for _ in 0..args.ticks {
        state.run_tick(&pool, &mut rng).await?;
    }

    info!("Simulation complete. Ran {} ticks.", args.ticks);

    Ok(())
}
