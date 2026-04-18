# Copilot Instructions for Galactic Market Simulator

## Quick Reference

**Technology Stack:** Rust (Edition 2024) + Tokio async runtime + PostgreSQL 16 + Clap CLI

**Key Commands:**
- `cargo run -- --ticks 1000` — Run simulation for 1000 ticks
- `cargo run -- --seed --ticks 100` — Seed the database then run 100 ticks
- `cargo test` — Run all tests
- `cargo test <test_name>` — Run a specific test
- `cargo clippy -- -D warnings` — Lint (warnings-as-errors)
- `cargo fmt` — Format code
- `cargo bench` — Run benchmarks
- `docker-compose up -d` — Start PostgreSQL database

## Project Overview

The Galactic Market Simulator is a **headless economic simulation engine** that models a complex, self-driving galactic economy without player input. The entire simulation state is kept in memory during tick execution for performance, then periodically persisted to PostgreSQL for durability and analytics.

### Core Architecture

**Research → Strategy → Execution Pattern:**
1. Load full simulation state into memory on startup (from PostgreSQL)
2. Execute 10-phase tick loop entirely in-memory
3. Periodically flush state back to PostgreSQL
4. Repeat

**The 10-Phase Tick Loop** (defined in `DESIGN.md` § 3.2):
1. **Resource Extraction** — Mine ore, harvest crops, etc.
2. **Refining** — Process raw materials into refined goods
3. **Production** — Manufacture finished goods from refined materials
4. **Logistics** — Move goods between locations (route calculation, cargo transit)
5. **Markets** — Price discovery and trading of goods
6. **Consumption** — Populations consume goods; demand shifts
7. **Finance** — Companies track costs, revenue, profit; decide on expansion/bankruptcy
8. **Events** — Random events and state transitions
9. **Politics** — Faction relations, taxes, diplomacy, warfare effects
10. **Cleanup** — End-of-tick bookkeeping (depreciation, record-keeping)

All 10 phases are defined in `src/sim/mod.rs` and called sequentially in `src/main.rs`.

### Universe Hierarchy

The simulated universe is a **6-level strict hierarchy**:
```
Empire/Faction (Level 1)
├── Sector (Level 2)
│   ├── Star System (Level 3)
│   │   ├── Planet/Station (Level 4)
│   │   │   ├── Continent/Region (Level 5)
│   │   │   │   ├── City/Settlement (Level 6)
```

Every entity in the database is part of this hierarchy (enforced by foreign keys). This structure drives taxation, transport routing, military logistics, and all geographic calculations.

### Key Modules

- **`src/db/`** — Database layer: migrations, seeding, and state persistence
  - `db::load` — Loads full simulation state from PostgreSQL into memory
  - `db::seed` — Procedural world generation (empires, sectors, systems, cities, resources)
  - `db::utils` — Utilities like `clear_database()`
  
- **`src/sim/`** — Core simulation logic, organized by phase
  - `sim::state` — In-memory state struct holding all simulation entities
  - `sim::resources` — Resource extraction and deposit management
  - `sim::production` — Factory production logic
  - `sim::markets` — Price discovery and order matching
  - `sim::logistics` — Cargo routing and transit
  - `sim::finance` — Company accounting and bankruptcy
  - `sim::events` — Random event engine and state transitions
  - `sim::consumption` — Population demand and consumption
  - `sim::decisions` — AI decision-making for companies and agents
  - `sim::namegen` — Procedural name generation

- **`migrations/`** — SQL schema files (run by `sqlx::migrate!` on startup)

- **`benches/sim_bench.rs`** — Performance benchmarks using Divan (CodSpeed)

## Performance Considerations

**Hot Path Performance:**
- The tick loop is the hot path; every query in the hot path affects total simulation speed
- Avoid database queries during tick execution (entire state is in memory)
- On startup, state is completely loaded from the database in one batch operation
- After ticks complete, state is flushed back to the database in batch writes

**Debugging with `--debug` Flag: Always Pipe to a File**

⚠️ **Critical:** Running with `--debug` flag directly to the terminal is **extremely slow** — terminal I/O adds minutes of overhead. Additionally, the output volume is typically too large for AI agents to parse effectively.

**Always follow this pattern:**
1. **Run with output redirected to a file:**
   ```bash
   cargo run -- --seed --ticks 1000 --debug > sim.log 2>&1
   ```
   This completes in reasonable time and allows post-hoc analysis.

2. **Analyze the log file afterward:**
   - PowerShell (Windows):
     ```powershell
     # Search for specific market activity
     Select-String -Path sim.log -Pattern "Match:" | Select-Object -First 20
     
     # Count events by type
     (Select-String -Path sim.log -Pattern "Event:").Length
     
     # Find errors
     Select-String -Path sim.log -Pattern "ERROR|panic"
     ```
   
   - Bash/Linux:
     ```bash
     # Search for specific market activity
     grep "Match:" sim.log | head -20
     
     # Count events by type
     grep -c "Event:" sim.log
     
     # Find errors
     grep -E "ERROR|panic" sim.log
     ```

3. **Filter to specific modules for targeted debugging:**
   ```bash
   RUST_LOG=galactic_market::sim::markets=debug cargo run -- --ticks 100 --debug > markets.log 2>&1
   ```
   Then analyze the smaller log file.

## Development Conventions

### Rust Standards

- **Error handling**: Use `Result<T, E>` and the `?` operator. Avoid `unwrap()` and `expect()` in production code.
- **Async/Await**: Use `tokio` for async runtime; `sqlx` for async database access.
- **Documentation**: All public items must have `///` doc comments with examples where applicable.
- **Testing**: Add unit tests in inline `#[cfg(test)]` modules; integration tests under `tests/`.
- **Formatting & Linting**: Always run `cargo fmt` and ensure `cargo clippy -- -D warnings` passes before committing.

### Simulation Principles

- **Deterministic where possible**: Use seeded RNG for reproducible runs (see `--random_seed` flag).
- **Phase separation**: Maintain the 10-phase tick loop as defined; don't mix logic across phases.
- **In-memory hot path**: Keep ticks as fast as possible — no database queries during tick execution.
- **Entity lifecycle**: Entities have clear creation/lifecycle/deletion states; track these in simulation state.

### Database & Migrations

- Use `sqlx::migrate!` to run migrations automatically on startup.
- Schema changes go in new migration files under `migrations/`.
- Leverage PostgreSQL features (foreign keys, constraints, triggers) for data integrity.
- Seed data goes in `src/db/seed.rs`; avoid hardcoding static data in code.

## Environment Setup

### Prerequisites

1. **PostgreSQL 16** (via Docker):
   ```bash
   docker-compose up -d
   ```
   This starts PostgreSQL on `localhost:5432`. Connection string is defined in `.env`.

2. **Rust toolchain** — Managed by `Cargo.toml` (Edition 2024). No special toolchain pinning required.

3. **Environment variables** — Copy `.env` template if needed:
   ```
   DATABASE_URL=postgres://postgres:password@localhost:5432/galactic_market
   ```

### First-Time Setup

```bash
# Install dependencies and build
cargo build

# Verify lint/format
cargo fmt && cargo clippy -- -D warnings

# Run migrations + seed + 10 ticks
cargo run -- --seed --ticks 10
```

## Testing Strategy

**Unit Tests:** Located inline in modules with `#[cfg(test)]`.
- Run specific test: `cargo test <module>::<test_name> -- --exact`
- Example: `cargo test sim::markets::tests::test_price_discovery -- --exact`

**Integration Tests:** Located in `tests/` directory.
- Run all integration tests: `cargo test --test '*'`
- Run specific integration test: `cargo test --test integration_test_name`

**Benchmarks:** Located in `benches/sim_bench.rs`, running via Divan + CodSpeed.
- Run: `cargo bench`
- Benchmarks help identify performance regressions in the hot path.

## Debugging Tips

### View Tracing Output

The project uses `tracing` for structured logging. By default, logs are at `INFO` level:
- **Full debug output:** `cargo run -- --debug ...` (very verbose)
- **Filter to specific modules:** Set `RUST_LOG` env var
  - Example: `RUST_LOG=galactic_market::sim::markets=debug cargo run -- --ticks 10`

### Inspecting Database State

Use PostgreSQL client to explore state:
```bash
psql postgres://postgres:password@localhost:5432/galactic_market

# List all tables
\dt

# View a specific table
SELECT * FROM cities LIMIT 10;

# Analyze market prices over ticks
SELECT tick, system_id, commodity_id, price FROM market_prices ORDER BY tick DESC LIMIT 50;
```

### Recording Learnings

When you discover a non-obvious pattern specific to this codebase (e.g., a borrow checker workaround, an async gotcha, a crate interaction quirk), record it in `.agent/learning/rust.md` so future Copilot sessions benefit. See `.agent/rules/rust-env.md` § 5 for the format and guidelines.

## Key Files to Know

- **`DESIGN.md`** — Complete project architecture, universe hierarchy, phase definitions, roadmap
- **`README.md`** — High-level project overview and status
- **`GEMINI.md`** — Research notes and design explorations (context for decision-making)
- **`.agent/rules/rust-env.md`** — Detailed Rust execution rules (toolchain, error handling, doc standards)
- **`Cargo.toml`** — Project manifest; lists all dependencies and build configuration
- **`docker-compose.yml`** — PostgreSQL 16 container setup

## Workflow Example: Adding a New Phase or Economic Feature

1. **Understand the existing architecture** — Read relevant sections of `DESIGN.md` and the phase definitions in `src/sim/mod.rs`.
2. **Design the feature** — Update `DESIGN.md` if adding a new system.
3. **Implement in memory** — Write the logic in a new module under `src/sim/` or extend an existing one.
4. **Update database schema** — Add migration files under `migrations/` if new tables/columns are needed.
5. **Update state loading** — Modify `src/db/load.rs` to load new data into the in-memory state struct.
6. **Integrate into tick loop** — Add a new phase call in `src/sim/mod.rs::tick()`.
7. **Test & benchmark** — Add unit tests and ensure no performance regression with `cargo bench`.
8. **Document** — Update `DESIGN.md` and add doc comments to public APIs.

## Notes

- **Windows path handling:** Use backslashes (`\`) in paths when working on Windows; tools in this environment expect Windows-style paths.
- **No manual Cargo.lock edits** — Let Cargo manage dependencies.
