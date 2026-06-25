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

### Debugging & Performance

Running the simulation with the `--debug` flag generates high-volume tracing logs. When running for more than 100 ticks, console I/O becomes a significant bottleneck. 

To maintain performance during debugging:
1. **Pipe to File:** Redirect output to a log file instead of the terminal.
2. **Post-Analysis:** Use `grep` (Linux/WSL) or `Select-String` (PowerShell) to analyze the resulting log.

**PowerShell Example:**
```powershell
# Run 1000 ticks with full debug logs saved to a file
cargo run -- --seed --ticks 1000 --debug > sim_debug.log 2>&1

# Search for specific market activity in the log
Select-String -Path sim_debug.log -Pattern "Match:" | Select-Object -First 20
```

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

# Lore Protocol -- Agent Instructions

Copy this into your AI agent's system prompt or instruction file.

---

## What Is Lore

Lore embeds structured decision context (constraints, rejected alternatives, directives) into git commit trailers. It is queryable via the `lore` CLI. Protocol version: 1.0.

## Before Modifying Any File

Run these commands for every file or directory you are about to change:

```sh
lore constraints <path> --json
lore rejected <path> --json
lore directives <path> --json
```

- **Constraint** = hard requirement. Do not violate.
- **Rejected** = approach tried and abandoned (`alternative | reason`). Do not re-explore.
- **Directive** = standing instruction. Follow it.

If constraints exist, verify your changes comply. If a rejected alternative matches your plan, choose differently.

## When Committing

Stage changes with `git add`, then pipe JSON to `lore commit`:

```sh
echo '{
  "intent": "fix: handle null user in auth middleware",
  "body": "Previously threw 500 on null user. Now returns 401.",
  "trailers": {
    "Constraint": ["must not throw -- return 401 instead"],
    "Rejected": ["silent redirect to login | breaks API clients"],
    "Confidence": "high",
    "Scope-risk": "narrow",
    "Tested": ["null user returns 401", "valid user still works"],
    "Not-tested": ["concurrent request race condition"]
  }
}' | lore commit
```

### JSON Schema

```json
{
  "intent": "string (REQUIRED) -- max 72 chars",
  "body": "string (optional)",
  "trailers": {
    "Constraint": ["string array"],
    "Rejected": ["format: 'alternative | reason'"],
    "Confidence": "'low' | 'medium' | 'high'",
    "Scope-risk": "'narrow' | 'moderate' | 'wide'",
    "Reversibility": "'clean' | 'migration-needed' | 'irreversible'",
    "Directive": ["string array"],
    "Tested": ["string array"],
    "Not-tested": ["string array"],
    "Supersedes": ["8-char hex Lore-id"],
    "Depends-on": ["8-char hex Lore-id"],
    "Related": ["8-char hex Lore-id"]
  }
}
```

Only `intent` is required. `Lore-id` is auto-generated.

### When to Add Trailers

| Situation | Trailer |
|-----------|---------|
| Chose A over B | `Rejected: ["B \| reason"]` |
| Rule must hold | `Constraint: ["the rule"]` |
| Future instruction | `Directive: ["the instruction"]` |
| Unsure | `Confidence: "low"` |
| Hard to undo | `Reversibility: "migration-needed"` |
| Known gap | `Not-tested: ["the gap"]` |

## Other Commands

| Command | Purpose |
|---------|---------|
| `lore context <path> --json` | Full context for a file/directory |
| `lore why <file>:<line> --json` | Line-level blame with Lore context |
| `lore search --text "q" --json` | Search across all lore |
| `lore stale <path> --json` | Check for outdated decisions |
| `lore trace <lore-id> --json` | Trace a decision chain |
