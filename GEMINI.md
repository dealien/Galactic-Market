# Galactic Market Simulator - Context & Guidelines

This document provides essential context and instructions for the **Galactic Market Simulator**, a headless economic simulation engine built in Rust and PostgreSQL.

## Project Overview

The Galactic Market Simulator is a high-performance simulation of a living, breathing galactic economy. It features a hierarchical universe structure and a 10-phase tick loop that models resource extraction, production, logistics, markets, finance, and politics.

### Core Technologies

- **Language:** Rust (Edition 2024)
- **Runtime:** Tokio (Async)
- **Database:** PostgreSQL 16 (running via Docker)
- **CLI:** Clap
- **Benchmarking:** Divan (CodSpeed)
- **Logging:** Tracing

### Architecture

The simulation follows a **Research -> Strategy -> Execution** pattern with a focus on in-memory state for performance.

1. **In-Memory State:** The full simulation state is loaded into memory on startup.
2. **Tick Loop:** A 10-phase tick loop advances the simulation entirely in-memory.
3. **Persistence:** State is periodically flushed back to PostgreSQL, which acts as a persistence and analytics layer.

## Project Structure

- `src/main.rs`: CLI entry point and tick loop runner.
- `src/lib.rs`: Library entry point.
- `src/db/`: Database logic, migrations, and seeding.
- `src/sim/`: Core simulation logic (divided into phases).
- `migrations/`: SQL schema definitions.
- `tests/`: Integration tests.
- `benches/`: Benchmarks for performance-critical components.

## Building and Running

### Environment Setup

- **Database:** The database runs in Docker. Start it with:

  ```bash
  docker-compose up -d
  ```

- **Environment Variables:** Create a `.env` file (or use existing) with:

  ```text
  DATABASE_URL=postgres://postgres:password@localhost:5432/galactic_market
  ```

### Commands

- **Run Simulation:** `cargo run -- --ticks 1000`
- **Seed & Run:** `cargo run -- --seed --ticks 100`
- **Build:** `cargo build`
- **Test:** `cargo test`
- **Bench:** `cargo bench`
- **Lint:** `cargo clippy -- -D warnings`
- **Format:** `cargo fmt`

## Development Conventions

### Rust Standards

- **Cargo First:** Use `cargo` for all development tasks.
- **Async/Await:** Use `tokio` for concurrency and `sqlx` for database interactions.
- **Error Handling:** Prefer `Result` and the `?` operator. Avoid `unwrap()` or `expect()` in production code.
- **Formatting:** Code must be formatted with `cargo fmt` and pass `cargo clippy -- -D warnings`. Ensure compliance before completion.

### Simulation Principles

- **In-Memory Hot Path:** Avoid database queries during the hot path of the tick loop.
- **Deterministic Ticks:** Ticks should be deterministic where possible (using seeded RNG).
- **Phased Execution:** Maintain the separation of the 10 phases as defined in `DESIGN.md`.

### Documentation & Reporting

- **Public API:** Document all public items with `///` doc comments, including examples.
- **Learnings:** Record non-obvious Rust patterns or codebase-specific discoveries in `.agent/learning/rust.md`.
- **System Instructions:** Instructions in `.agent/rules/rust-env.md` must be followed for all Rust work.

### Database

- **Migrations:** Use `sqlx-cli` or `sqlx::migrate!` for schema changes.
- **Seeding:** The `seed` module handles initial world generation.

---
*Refer to `DESIGN.md` for the full project roadmap and architectural details.*
