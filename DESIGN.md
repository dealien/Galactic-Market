# Galactic Market Simulator

## Project Design & Development Guide

Galactic Market Simulator — Project Guide
v0.2 — Draft

**Stack:** Rust · SQL/PostgreSQL · WebAssembly/SvelteKit

**Updated:** April 2026. This document reflects the current implemented state. Political simulation and web UI are still in development.

---

## Table of Contents

1. [Project Vision](#1-project-vision)
2. [Universe Structure & Geography](#2-universe-structure--geography)
3. [Economic Simulation](#3-economic-simulation)
4. [Database Design](#4-database-design)
5. [Rust Implementation Architecture](#5-rust-implementation-architecture)
6. [Political Simulation](#6-political-simulation)
7. [Random Event Engine](#7-random-event-engine)
8. [Development Roadmap](#8-development-roadmap)
9. [Learning: SQL & Database Management](#9-learning-sql--database-management)
10. [Open Design Questions](#10-open-design-questions)

---

## 1. Project Vision

Galactic Market Simulator (working title) is a fully-automated, headless economic simulation engine inspired by the depth of Eve Online and the scale of Elite Dangerous. There is no player in the loop — the simulation runs on its own, producing a living, breathing galactic economy that can be observed, analyzed, and eventually influenced through a god-mode interface.

The core proposition is emergent complexity from simple rules: individual freelance miners decide what to extract based on prices; refineries buy ore and sell metals; manufacturers produce goods; merchants ship cargo between systems; populations grow or shrink; wars erupt; corporations rise to megacorp status and collapse. None of this is scripted — it falls out of the simulation.

### 1.1 Inspiration Sources

- **Eve Online** — deep player-driven markets, production chains, corporation mechanics, sovereignty warfare
- **Elite Dangerous** — galactic scale, background simulation (BGS), faction states, commodity flows between station types
- **Victoria 3 / Dwarf Fortress** — fully simulated populations with needs, jobs, and economic participation
- **Crusader Kings / EU4** — political simulation layered on top of economics; characters and events matter

### 1.2 Distinguishing Goals

- Everything simulated: no hard-coded price tables; prices emerge from supply, demand, and transport costs
- Full vertical integration: raw resource → refined material → intermediate good → finished product → consumer
- Geographic realism: transport time and cost matter; a cheap resource far away may be more expensive than a pricier local one
- Company lifecycle: freelancers → SMEs → corporations → megacorps, with mergers, acquisitions, and bankruptcies
- Political layer: factions, diplomacy, taxation, war, blockades, sanctions affect all economic activity
- Random event engine: disasters, discoveries, deaths, tech breakthroughs inject stochasticity
- Data-first: the entire simulation state lives in a SQL database; queries are first-class citizens
- Visualization layer *(future)*: web-based maps, charts, and dashboards for exploring the running simulation
- God mode *(future)*: player can inject events, create companies, edit markets, and observe cascading effects

---

## 2. Universe Structure & Geography

The simulated universe is organized as a strict six-level hierarchy. Each level is a row in the database; parent-child relationships are foreign keys. This hierarchy drives everything from taxation to transport routing to military logistics.

### 2.1 Hierarchy Levels

| Level | Entity Type | Examples / Notes |
| --- | --- | --- |
| 1 (Top) | Empire / Faction | The Republic, Frontier Syndicate — has government type, currency, military power, tax policy |
| 2 | Sector | A grouping of star systems under one faction's influence — analogous to a province or region |
| 3 | Star System | Sol, Alpha Centauri — has a star type, resource richness modifier, jump connectivity to other systems |
| 4 | Planet / Station | Rocky planet, gas giant, L4 trade station — has gravity, climate, habitability, docking capacity |
| 5 | Continent / Region | Landmass or station zone — groups cities; has terrain type affecting resource availability |
| 6 (Bottom) | City / Settlement | The atomic population/economic unit — has population, infrastructure level, consumer demand, port status |

> **Design note:** Jump lanes between star systems are stored as a separate adjacency table (`system_id_a`, `system_id_b`, `distance_ly`, `gate_type`). This models Eve-style stargates or Elite-style hyperspace jumps.

### 2.2 Resource Geography

Resources are distributed procedurally (or by hand during world-gen) across planets and continents. Key properties:

- **Deposit size** — total extractable units; finite; depletion is tracked per tick
- **Extraction difficulty** — affects cost per unit and time to extract
- **Discovery state** — deposits can be unknown until a survey operation finds them
- **Regeneration rate** — some resources regrow (food/water); others are non-renewable (ore, fossil fuels)
- **Richness decay** — as a deposit depletes, extraction cost rises (declining ore grades)

### 2.3 Transport & Logistics

Movement of goods is not instantaneous. Every cargo shipment has a route, a duration, and a cost.

| Mode | Scope | Speed | Capacity | Cost Driver |
| --- | --- | --- | --- | --- |
| Ground convoy | City → City (same continent) | Slow | Medium | Distance, terrain, road quality |
| Orbital shuttle | Continent → Planet orbit | Medium | Low | Fuel, port fees |
| In-system freighter | Planet → Planet / Station | Medium | High | Fuel, transit time |
| Jump freighter | System → System | Fast | High | Jump fuel, gate tolls, faction access |
| Sector hauler | Sector → Sector (long haul) | Slow (relative) | Very High | Multi-jump path, insurance |

> **Note:** Transport lanes can be disrupted by war, piracy events, or infrastructure damage. Routing algorithms must find the cheapest path, not just the shortest.

---

## 3. Economic Simulation

### 3.1 Production Chains

The economy runs on multi-stage production chains. A simplified example:

| Stage | Input | Output | Actor Type |
| --- | --- | --- | --- |
| Mining | Iron Ore Deposit | Raw Iron Ore | Miner / Mining Corp |
| Refining | 10 Raw Iron Ore | 6 Iron Ingots | Refinery |
| Component Mfg | 4 Iron Ingots + 1 Carbon | 10 Steel Frames | Manufacturer |
| Assembly | 20 Steel Frames + electronics + power cells | 1 Starship Hull | Shipyard |
| Retail / Distribution | Starship Hull + fitting | Fitted Frigate (saleable) | Merchant / Broker |

Production chains are defined in a `recipes` table. Any actor can execute any recipe if they own the inputs, have the facility, and it is profitable to do so. This makes the production chain data-driven, not hard-coded.

#### 3.1a Food Production & Plantations

Unlike mining (which depletes finite deposits), food production is renewable and driven by **planet fertility**. Each planet has a fertility rating (0.0–3.0x multiplier) that determines base productivity. Plantations are facilities that produce Food Rations at a rate proportional to fertility:

- **Plantation capacity** = Base capacity × (1.0 + fertility multiplier)
- **Example:** A planet with 1.5x fertility has plantations that produce 50% more than a 1.0x baseline planet
- **No inputs required:** Plantations convert environmental fertility directly into Food Rations each tick
- **Seeding:** All cities start with at least one plantation; capacity is set during world generation based on planet fertility
- **Expansion:** Companies can build additional plantations if cash and profitability justify the cost

This system ensures:
1. All planets naturally produce food (no planet is barren of sustenance)
2. High-fertility planets become agricultural hubs, attracting population and trade
3. Famine events (which consume food stocks) drive up prices, making plantations profitable
4. Geographic diversity: Core worlds typically have higher fertility; Rim worlds are more variable

**Future enhancement:** Continents will have individual terrain types (grassland, desert, tundra, etc.) that override planet-level fertility, allowing fine-grained geographic specialization.

### 3.2 Price Discovery

There are no global price tables. Prices exist per market (city or station) and are determined by local supply and demand:

- Each market has an order book: open buy orders and sell orders
- Price trends toward the clearing price each tick based on order volume
- Prices propagate between markets through arbitrage: agents will buy cheap and sell expensive, equalizing prices minus transport costs
- Price signals drive production decisions: if iron ore prices rise, miners invest more
- Elastic demand: populations buy more of goods that become cheaper and substitute when prices rise

> **Design note — avoiding oscillation:** With fully reactive AI, all miners will simultaneously pivot to the highest-priced resource in the same tick, flood the market, crash the price to zero, then all pivot away — producing pendulum swings rather than equilibrium. Mitigate this by giving company AI **imperfect information and decision stickiness**: not every company re-evaluates strategy every tick; companies should have a configurable re-evaluation interval (e.g., every 5–20 ticks, jittered), and switching production should carry a **retooling cost** (time + capital) that makes short-cycle pivots unprofitable. This is implemented in the Decisions phase (Phase 6) of the tick loop.

### 3.3 Supply & Demand Model

| Factor | Effect on Demand | Effect on Supply |
| --- | --- | --- |
| Population growth | Increases base demand for all consumer goods | Increases labor availability |
| Rising income/wealth | Shifts demand to higher-tier goods | Attracts more investment capital |
| Price increase | Reduces quantity demanded (price elasticity) | Increases quantity supplied |
| War / blockade | Disrupts import supply; raises prices locally | May destroy production infrastructure |
| Tech advancement | May create new demand categories | Lowers production cost (supply shift) |
| Natural disaster | Can spike demand for relief goods | Destroys local supply capacity |

### 3.4 Company Lifecycle

Economic actors evolve along a maturity axis:

- **Freelancer** — single individual or small crew, no formal structure, low capital, opportunistic decisions
- **Small Company** — formal entity, has a home base, can employ workers, takes on debt, starts to specialize
- **Corporation** — multi-location, has departments (mining, logistics, R&D), can issue equity, lobbies government
- **Megacorp** — galaxy-spanning, political influence, private fleets, can trigger wars, too big to fail

Progression is driven by profitability and capital accumulation. Regression (downsizing, bankruptcy) is triggered by sustained losses, debt default, or competitor action.

| Event | Trigger Condition | Outcome |
| --- | --- | --- |
| Incorporation | Freelancer profits exceed threshold for N ticks | New company entity created; assets transferred |
| Expansion | Free cash flow > capex threshold | New facility opened in adjacent market |
| Acquisition | Company A has cash > Company B market cap + premium | Merger; assets consolidated |
| Bankruptcy | Debt service > revenue for N consecutive ticks | Assets liquidated; workers unemployed |
| Spin-off | Division exceeds size threshold | New subsidiary company created |
| Nationalization | Strategic company in war zone | Faction takes majority stake |

---

## 4. Database Design

The canonical simulation state — universe geography, economic actors, market orders, production queues, political relations — is defined in a PostgreSQL schema. However, the Rust engine does **not** read from and write to Postgres every tick. Instead, it loads the full state into memory on startup and runs the tick loop entirely in-memory. Postgres is treated as a **persistence and analytics layer**, not the hot-path memory for the tick loop.

The engine flushes state back to the database periodically (e.g., every 100 ticks) and always appends to `events_log` and `market_history` on every tick. This separation means the simulation can be paused, inspected, replayed, and queried at any point without instrumenting the code — while keeping the tick loop fast enough to hit the 1,000 ticks/second target.

### 4.1 Why SQL / PostgreSQL

- **Structured, relational data** with enforced referential integrity (foreign keys prevent orphaned records)
- **Rich query language:** aggregations, window functions, and CTEs are perfect for economic analytics
- **Transactions:** an entire simulation tick can be committed atomically, preventing half-applied state
- **Indexing:** spatial and partial indexes accelerate the hot queries (find all markets within range, find all buy orders above price X)
- **Tooling:** PgAdmin, psql, DBeaver, and dozens of BI tools can connect directly for ad-hoc analysis
- **Future UI** can query Postgres directly via a REST API layer, no special export needed

### 4.2 Core Table Groups

#### Geography Tables

| Table | Key Columns | Notes |
| --- | --- | --- |
| `empires` | `id, name, government_type, currency, tax_rate_base` | Top-level factions |
| `sectors` | `id, empire_id, name, strategic_value` | FK → empires |
| `star_systems` | `id, sector_id, name, star_type, resource_modifier` | FK → sectors |
| `system_lanes` | `system_a_id, system_b_id, distance_ly, gate_status` | Adjacency graph for routing |
| `celestial_bodies` | `id, system_id, body_type, mass, habitable, population_cap, fertility` | Planets, moons, stations; fertility (0.0–3.0x) affects plantation production |
| `continents` | `id, body_id, name, terrain_type, area_km2` | Sub-divisions of planets |
| `cities` | `id, continent_id, name, population, infrastructure_lvl, port_tier` | Atomic economic unit |

#### Resource & Production Tables

| Table | Key Columns | Notes |
| --- | --- | --- |
| `resource_types` | `id, name, category, base_mass_kg, stackable` | Master list of all goods/materials |
| `deposits` | `id, continent_id, resource_type_id, size_total, size_remaining, difficulty, discovered` | Physical deposits |
| `recipes` | `id, name, output_resource_id, output_qty, facility_type, time_ticks` | Production blueprints |
| `recipe_inputs` | `recipe_id, resource_type_id, quantity` | Many-to-many inputs per recipe |
| `facilities` | `id, city_id, company_id, facility_type, capacity, condition` | Physical production assets |

#### Economic Actor Tables

| Table | Key Columns | Notes |
| --- | --- | --- |
| `companies` | `id, name, type, home_city_id, cash, debt, credit_rating` | All economic actors; type = freelancer/corp/megacorp |
| `company_holdings` | `company_id, facility_id, stake_pct` | Ownership of facilities |
| `employees` | `id, company_id, city_id, role, wage, skill_level` | Labor (can be simplified to aggregate counts) |
| `production_jobs` | `id, facility_id, recipe_id, started_tick, finish_tick, status` | In-progress production |

#### Market Tables

| Table | Key Columns | Notes |
| --- | --- | --- |
| `markets` | `id, city_id, name, market_type` | One per city minimum; market_type = commodity/futures/labor |
| `market_orders` | `id, market_id, company_id, order_type, resource_id, qty, price, created_tick` | Live order book; order_type = buy/sell |
| `market_history` | `market_id, resource_id, tick, open, high, low, close, volume` | OHLCV price history — append only |
| `trade_routes` | `id, company_id, origin_city_id, dest_city_id, cargo_manifest, status, eta_tick` | In-transit shipments |

#### Political Tables

| Table | Key Columns | Notes |
| --- | --- | --- |
| `diplomatic_relations` | `empire_a_id, empire_b_id, status, tension_score` | Symmetric relation; status = war/neutral/allied |
| `treaties` | `id, empire_a_id, empire_b_id, type, expires_tick` | Formal agreements; type = trade/non-aggression/vassal |
| `conflicts` | `id, aggressor_id, defender_id, start_tick, end_tick, outcome` | War records |
| `political_figures` | `id, empire_id, role, influence_score, alive` | Named characters with event hooks |
| `events_log` | `id, tick, event_type, scope_type, scope_id, description, effects_json` | Immutable event ledger |

### 4.3 Tick Architecture

The simulation advances in discrete ticks (e.g., 1 tick = 1 simulated day or week). Each tick the engine performs a fixed sequence of phases **entirely in memory**, then periodically flushes state to the database:

| Phase | Operations | Module | State Modified |
| --- | --- | --- | --- |
| 1. Resource Extraction | Advance extraction jobs; deplete deposits | `resources::run_extraction()` | deposits, companies, inventory |
| 2. Production | Advance production jobs; consume inputs; create output | `production::run_production()` | production jobs, inventory |
| 3. Logistics | Advance in-transit shipments; deliver cargo at destination | `logistics::run_logistics()` | trade routes, inventory |
| 4. Company AI Decisions | Each company AI evaluates profitability and queues new actions | `decisions::run_decisions()` | market orders, production jobs |
| 5. Population Consumption | Populations consume goods; update demand and food shortages | `consumption::run_consumption()` | companies, inventory, populations |
| 6. Market Clearing | Match buy/sell orders; compute clearing prices | `markets::clear_orders()` | market orders, inventory |
| 7. Finance | Pay wages, loan interest; update company cash/debt | `finance::run_finance()` | companies, bank accounts |
| 8. Random Events | Roll random events; trigger blockades, disasters, tech breakthroughs | `events::run_events()` | active events, various (event-dependent) |

**Periodic Flush (every 100 ticks):**
- All dirty state written to database in a single transaction
- `events_log` and `market_history` appended; `simulation_meta.current_tick` updated
- Crash recovery: can restart from last clean checkpoint (see §5.4)

> **Note:** Each phase is a separate Rust module. The tick loop runs entirely in memory for performance. Politics phase (diplomacy, faction relations, taxation) is currently integrated into the Events phase but is planned as a separate phase in future development.

---

## 5. Rust Implementation Architecture

Rust is an excellent choice for this project. The simulation is CPU-bound, handles large volumes of data per tick, and needs to be fast enough to simulate many ticks in reasonable time. Rust's ownership model also prevents entire classes of bugs that would be catastrophic in a long-running simulation.

### 5.1 Dependency Stack

| Crate | Version | Purpose | Status |
| --- | --- | --- | --- |
| `sqlx` | 0.7 | Async SQL with compile-time checking; PostgreSQL support | ✅ Active |
| `tokio` | 1.x | Async runtime (full features enabled) | ✅ Active |
| `tracing` / `tracing-subscriber` | 0.1 / 0.3 | Structured logging with environment filtering | ✅ Active |
| `clap` | 4.x | CLI argument parsing (derive macros) | ✅ Active |
| `dotenvy` | 0.15 | Load `.env` files for configuration | ✅ Active |
| `rand` | 0.8 | Random number generation for events, AI decisions | ✅ Active |
| `anyhow` | 1.0 | Ergonomic error handling | ✅ Active |
| `serde` / `serde_json` | 1.0 | Serialization for config, effects, API responses | ✅ Active |
| `comfy-table` | 7.1 | Terminal tables for Economic Pulse display at flush time | ✅ Active |
| `petgraph` | 0.8.3 | Graph structures for jump lane routing (Dijkstra) | ✅ Active |
| `divan` (codspeed-divan-compat) | 4.4.1 | Performance benchmarking (CodSpeed integration) | ✅ Dev |

**Not yet integrated (planned):**
- `rayon` — Data parallelism for decision phase across companies
- `axum` — HTTP server for REST API / web UI
- `thiserror` — Custom error types (use `anyhow` for now)

### 5.2 Project Structure

```text
galactic-market/
  Cargo.toml
  src/
    main.rs                     # CLI entry point; handles args and tick loop runner
    lib.rs                      # Library entry point
    db/
      mod.rs                    # Database module exports
      load.rs                   # Load full simulation state from Postgres into memory
      seed.rs                   # Procedural world generation (empires, sectors, systems, cities, etc.)
      utils.rs                  # Utilities (clear_database, etc.)
      migrations/               # SQL migration files (run by sqlx::migrate!() on startup)
    sim/
      mod.rs                    # Tick loop implementation; coordinates all phases
      state.rs                  # In-memory SimState struct; mirrors DB schema
      resources.rs              # Phase 1: resource extraction & deposit management
      production.rs             # Phase 2: production job execution
      logistics.rs              # Phase 3: cargo routing & transit
      decisions.rs              # Phase 4: company AI decision-making
      consumption.rs            # Phase 5: population consumption & demand updates
      markets.rs                # Phase 6: order matching & price discovery
      finance.rs                # Phase 7: wages, interest, cash/debt tracking
      events.rs                 # Phase 8: random events & political mechanics
      namegen.rs                # Procedural name generation for entities
  benches/
    sim_bench.rs                # Performance benchmarks (Divan + CodSpeed)
  migrations/                   # SQL schema definitions
  tests/                        # Integration tests
  docker-compose.yml            # PostgreSQL 16 container definition
  .env                          # Environment variables (DATABASE_URL, etc.)
```

**Key Design Principle:** All simulation state is loaded into memory (`SimState` struct) on startup. The tick loop mutates this struct entirely in-memory. Every 100 ticks, dirty state is flushed back to Postgres in a single atomic transaction. This separation keeps the hot path fast and Postgres as a durability/analytics layer (see §5.4).

### 5.3 Company Decision AI

The simulation implements adaptive company AI driven by **re-evaluation intervals** — not all companies make decisions every tick. Companies re-evaluate periodically (based on type) and make greedy, locally-informed decisions.

**Re-evaluation Intervals by Type:**
- **Freelancers / Merchants:** 1–5 ticks (respond quickly to price changes)
- **Small Companies:** 5–20 ticks (moderate planning horizon)
- **Corporations:** 20–60 ticks (slower, more strategic)
- **Megacorps:** 60–200 ticks (long-term planning)
- **Commercial Banks:** 5–20 ticks
- **Central Banks:** 50–100 ticks

**Decision Types (implemented in Phase 4 — Decisions):**

1. **Liquidation (Bankrupt Companies):** Post fire-sale orders at 50% market price to quickly convert inventory to cash.

2. **Corporate Treasury:** Manage deposits/withdrawals from commercial banks; request loans if cash is low and Debt-to-Asset ratio is sustainable (<0.8).

3. **Resource Extraction:** Evaluate ore prices; queue extraction jobs if `price > extraction_cost * (1 + profit_margin)`.

4. **Production:** Evaluate recipe profitability; queue production jobs if inputs are available and output price exceeds all costs.

5. **Trading & Arbitrage:** Scan all reachable city pairs for price differences; execute trade routes if `buy_price + transport_cost < sell_price − trading_margin`.

6. **Market Orders:** Post buy/sell orders on local markets at prices informed by EMA prices and inventory levels.

> **Note:** Future sophistication could include multi-step planning (e.g., pathfinding to distant high-profit markets), learning algorithms that tune decision thresholds, or faction-aligned behavior. Current implementation prioritizes speed and determinism.

### 5.4 State Management & Database Access Pattern

The simulation maintains a `SimState` struct in memory that mirrors the database. On startup the engine hydrates it with a full `SELECT` of all tables. The tick loop then reads and mutates this struct exclusively — no DB queries during computation.

**Startup:**

1. Load all relevant rows from Postgres into `SimState` (bulk `SELECT`, not N+1 queries)
2. Build any auxiliary in-memory structures (e.g., `petgraph` graph for jump lanes)

**Per-tick (hot path — memory only):**
3. Execute all tick phases against `SimState`
4. Append new `events_log` and `market_history` entries to an in-memory delta buffer

**Periodic flush (every N ticks):**
5. Open a single DB transaction
6. Write all dirty rows from `SimState` + drain the delta buffer
7. Commit; update `simulation_meta.current_tick`

This minimizes round-trips, keeps tick duration predictable, and ensures Postgres remains a fast analytics target rather than a bottleneck on the hot path.

> **Performance note:** Profile early. The markets phase (order matching) and decisions phase (per-company AI) are likely bottlenecks within the in-memory loop. `rayon` parallelism can help with decisions if companies are independent within a tick.

---

## 6. Political Simulation *(Planned / Partial Implementation)*

Politics is the second simulation layer sitting above economics. It does not replace economic logic — it modifies it. Wars raise taxes and disrupt trade; alliances open new markets; political instability increases risk premiums.

> **Status:** This section describes the intended design. Current implementation includes basic `DiplomaticRelation` structures and blockade event mechanics, but full diplomatic states, faction politics, and war mechanics are **not yet implemented**. Political effects are currently integrated into the Events phase. A dedicated Politics phase is planned for future development.

### 6.1 Diplomatic States *(Planned)*

| State | Economic Effect | Trigger Conditions |
| --- | --- | --- |
| Allied | Open borders; shared intelligence; possible joint taxation | Treaty signed; mutual defense activated |
| Neutral | Normal trade; standard tariffs | Default state; post-war cool-down |
| Cold War | Embargoes possible; higher tariffs; espionage events active | Tension > threshold without declaration |
| War | Blockades; territorial seizures; supply chain disruption; defense spending spike | Formal declaration or border incident |
| Occupation | Occupied territories taxed at higher rate; resistance events | War victory condition met |

### 6.2 Political Event Types *(Planned)*

- **Leadership change** — new ruler may change tax policy, alliances, or trigger military buildup
- **Election / Coup** — faction government type may shift; economic policy uncertainty spike
- **Trade deal signed** — tariff reduction between factions; new trade route profitability
- **Blockade declared** — specific jump lane(s) blocked; prices diverge between systems *(currently implemented as event type)*
- **Sanction imposed** — specific goods cannot cross faction borders; new smuggling opportunity
- **Rebellion** — city or sector breaks away; creates a new mini-faction or joins neighbor

### 6.3 War Mechanics *(Planned - Simplified Model)*

Full military simulation is out of scope for v1. A simplified war model:

- Wars have a **theater** (set of contested systems/sectors)
- Each tick, `military_strength` scores are compared with dice rolls + terrain + supply line modifiers
- Outcomes: territory changes, infrastructure damage to random cities in theater, economic disruption events
- War ends when one side's territory falls below `capitulation_threshold` or a peace treaty is accepted
- War cost model: defense spending rises; consumer goods production capacity falls; debt rises

---

## 7. Random Event Engine

Events are the third simulation layer (Phase 8 of the tick loop). They inject irreducible uncertainty — even a perfectly-managed company can be disrupted by blockade events or market crashes.

### 7.1 Event System *(Current Implementation)*

Events are stored in memory as `EventDefinition` structs loaded from the database. Each definition has:

- `id` and `name` — unique identifier and display name
- `weight` — base probability (0–100) relative to other events; weighted sampling per tick
- `severity_range` — [min, max] severity multiplier applied to effects
- `effects` — array of effect definitions, each with:
  - `effect_type` — string identifier (e.g., "blockade_lane", "price_spike")
  - `duration_range` — [min, max] ticks the effect persists
  - Additional effect-specific parameters

**Tick 8 Event Flow:**
1. **Expiration:** Remove events whose `end_tick < current_tick`; increment `blockade_version` if any blockade expires (triggers rerouting logic)
2. **New Events:** 5% chance per tick to fire a random event (weighted by event definition weights)
3. **Politics:** Process diplomatic tensions and war status (integrated into events phase; planned as separate phase)

### 7.2 Implemented Event Types

| Event Type | Effect | Target | Duration |
| --- | --- | --- | --- |
| `blockade_lane` | Blocks specific star system jump lane; disrupts trade routes; prices diverge | Jump lane (sys_a, sys_b) | 10–100 ticks |
| `price_spike` | Multiplies resource price in a city | (city_id, resource_type_id) | 5–50 ticks |
| `population_surge` | Increases city population by percentage | city_id | 1–20 ticks |
| `food_shortage` | Reduces food availability; triggers population crisis if severe | city_id | 5–30 ticks |

### 7.3 Planned Event Types *(Future)*

- **Asteroid Strike** — City infrastructure damage; population loss; potential new ore deposit
- **Disease Outbreak** — Population decline; labor shortage; demand spike for medical goods
- **Tech Breakthrough** — Recipe efficiency bonus; possible new recipe unlocked
- **Megacorp Scandal** — Company credit rating drop; acquisition vulnerability
- **Resource Discovery** — New deposit revealed; local price crash
- **Pirate Surge** — Trade route risk increases; insurance costs rise; cargo seizure events
- **Solar Flare** — In-system transit slowed for duration
- **Infrastructure Boom** — Construction costs temporarily reduced

---

## 8. Development Roadmap & Current Status

### Current Implementation Status (April 2026)

The project is in **active development**, past the foundation stages and into core economic simulation refinement. Below is the actual status vs. the roadmap.

#### ✅ Completed (Stage 0–2)

**Stage 0 — Foundation:**
- ✅ Rust project with sqlx, tokio, tracing, clap configured
- ✅ PostgreSQL schema with migrations (empires, sectors, systems, cities, resources, etc.)
- ✅ Procedural world generation (seeding ~1000+ entities)
- ✅ Tick loop framework with all 8 phases
- ✅ CLI arguments: `--ticks`, `--seed`, `--clear`, `--debug`, `--random-seed`

**Stage 1 — Basic Economy:**
- ✅ Resource extraction with deposits and depletion
- ✅ Production system with job queues
- ✅ Market order books with buy/sell matching
- ✅ Basic company AI (limited re-evaluation, greedy heuristics)
- ✅ Price history (`market_history` table; OHLCV data)
- ✅ Trade routes and cargo logistics
- ✅ Basic consumption model (population demands food)

**Stage 2 — Company Lifecycle:**
- ✅ Finance phase: wages, debt tracking, cash management
- ✅ Loan system: companies can borrow from commercial banks
- ✅ Bankruptcy detection: companies with negative cash and unpayable debt
- ✅ Fire-sale liquidation: bankrupt companies post discounted inventory

#### 🔶 In Progress (Stage 2.5)

- 🔶 **Consumption & Population:** Food demand is basic; missing complex demand curves and population growth
- 🔶 **Event System:** Basic blockade/price spike events; missing most event types
- 🔶 **Company AI:** Re-evaluation intervals working; needs more sophisticated trading logic

#### ⏳ Planned (Stage 3+)

**Stage 3 — Geography & Logistics:**
- ⏳ Transit time modeling (currently instant in some paths)
- ⏳ Realistic shipping costs and route optimization (Dijkstra partially done)
- ⏳ System lane disruptions based on blockade events
- ⏳ Advanced trading AI exploiting arbitrage

**Stage 4 — Politics & Events:**
- ⏳ Full event definition system with JSON/TOML configs
- ⏳ Diplomatic tension and faction relations
- ⏳ Blockade effects on trade (partially done; routes respect blockades)
- ⏳ War mechanics and territory control
- ⏳ Random seed reproducibility

**Stage 5 — Web UI:**
- ⏳ REST API with `axum` HTTP server
- ⏳ Live market charts and company dashboards
- ⏳ Real-time event feed
- ⏳ SvelteKit frontend + D3.js galactic map

**Stage 6 — God Mode:**
- ⏳ Manual event triggering
- ⏳ Company editor (create/modify via API)
- ⏳ Diplomacy controls
- ⏳ WebSocket live simulation stream

### Immediate Priorities

1. **Stabilize consumption model** — Population growth curves, demand elasticity, food crisis mechanics
   - *Tracked in GitHub issue #10 (Dynamic Population, Migration, Empire Relief System) and #9 (Balanced Economic Cycle)*

2. **Complete event system** — Load event definitions from JSON; implement all event types
   - *Related to issue #11 (Dynamic Resource Loading via JSON)*

3. **Improve trading AI** — Multi-hop routing, arbitrage detection, margin calculations

4. **Performance benchmarking** — Profile hot paths; optimize market matching and decision AI

5. **Roadmap refinement** — Determine UI priorities (god mode vs. analytics vs. playback)

> **Note:** See GitHub repository for active issues: https://github.com/dealien/Galactic-Market/issues
> Key blockers: Issues #9 (Economic Cycle) and #10 (Population) are prerequisites for meaningful war mechanics and should be prioritized before Stage 4 political simulation.

### GitHub Issues Cross-Reference

This table maps open GitHub issues to relevant DESIGN.md sections and roadmap stages:

| Issue # | Title | Design Section | Stage | Priority | Blocker | Status |
|---------|-------|---|-------|----------|---------|--------|
| **#9** | [Balanced Economic Cycle](https://github.com/dealien/Galactic-Market/issues/9) | §3.2, §5.4 | 2→3 | 🔴 Critical | — | Foundation for other issues |
| **#10** | [Dynamic Population & Migration](https://github.com/dealien/Galactic-Market/issues/10) | §3.3, §6.2 | 2.5→3 | 🔴 Critical | #9 | Prerequisite for war mechanics |
| **#5** | [Banks & Banking](https://github.com/dealien/Galactic-Market/issues/5) | §5.3 (AI), §7 (Finance) | 3 | 🟡 High | #9 | Parallel work; depends on #9 |
| **#11** | [Dynamic Resource Loading (JSON)](https://github.com/dealien/Galactic-Market/issues/11) | §4.1 (Seeding) | 3 | 🟡 High | — | Infrastructure; can be done in parallel |
| **#12** | [Terrain-based Fertility System](https://github.com/dealien/Galactic-Market/issues/12) | §3.1a (Production) | 3 | 🟢 Medium | #11 | Geographic specialization |
| **#2** | [Procedural Name Generator](https://github.com/dealien/Galactic-Market/issues/2) | §5.2 (modules), §8.1 | 1 | 🟢 Low | — | Quality-of-life enhancement |

**Recommendation:** Address issues in this priority order:
1. **#9** — Unblocks all others; critical foundation
2. **#10** — Depends on #9; unlocks war mechanics
3. **#5** — Parallel work; depends on #9
4. **#11, #12** — Infrastructure/polish; can overlap
5. **#2** — Polish; no dependencies

---

## 9. Learning: SQL & Database Management

### 9.1 Core Concepts to Use

| Concept | Why It Matters for This Project |
| --- | --- |
| `SELECT`, `WHERE`, `JOIN` | Every tick query reads joined tables (e.g., companies + facilities + cities) |
| `INSERT`, `UPDATE`, `DELETE` | Writing simulation results back to DB each tick |
| Transactions (`BEGIN`/`COMMIT`/`ROLLBACK`) | Atomically apply an entire tick; never leave partial state |
| Indexes (B-tree, partial) | Without indexes, queries over 100k+ rows will be slow |
| Foreign Keys & Constraints | Enforce data integrity — prevent orphaned facilities, invalid company refs |
| Window Functions (`ROW_NUMBER`, `RANK`, `LAG`) | Compute price moving averages, rank companies by profit, detect trends |
| CTEs (`WITH` clauses) | Break complex tick queries into readable, composable steps |
| `EXPLAIN ANALYZE` | Profile slow queries — essential once the DB is large |

### 9.2 PostgreSQL-Specific Features to Use

- **JSONB columns** — store variable event effects and production modifiers without schema changes
- **Array columns** — e.g., `trade_route.cargo_manifest` as `resource_id[] + qty[]`
- **`COPY` command** — bulk-insert world-gen data orders of magnitude faster than `INSERT` loops
- **Partial indexes** — e.g., `CREATE INDEX ON market_orders(resource_id) WHERE status = 'open'`
- **Table partitioning** — partition `market_history` by tick range for fast historical queries

### 9.3 Using sqlx in Rust

- Use `sqlx::query_as!` macro to map query results directly to Rust structs
- Use `sqlx::query!` for `INSERT`/`UPDATE` with compile-time parameter checking
- Manage schema with `sqlx migrate` — migration files live in the repo; run on startup
- Use a connection pool (`PgPool`) — the tick loop will fire many parallel queries

---

## 10. Open Design Questions

These are deliberate open questions — decisions to revisit as the simulation matures. Premature decisions here can over-constrain the design.

| Question | Options / Notes |
| --- | --- |
| **Tick granularity** | 1 tick = 1 day vs. 1 week. Finer granularity = more realism but more ticks to simulate centuries. Start with 1 week. |
| **Population as aggregate vs. individuals** | Aggregate (`city.population: u64`) is feasible. Individual agents (Victoria 3 style) is realistic but computationally expensive. Start aggregate. |
| **Company AI sophistication** | Greedy local heuristics vs. simple planning vs. full MCTS. Greedy is fine for v1; add planning later. |
| **Currency & exchange rates** | Single galactic currency (simplest) vs. faction currencies with exchange rates (richer but complex). Decide before Stage 1. |
| **Procedural world-gen vs. hand-crafted universe** | Hand-crafted gives more control; procedural enables multiple playthroughs. Start with a procedural world-gen, then add hand-crafted elements later. |
| **Simulation speed target** | How many ticks/second? 1000 ticks/s would simulate ~19 simulated years/second at 1-week ticks. Benchmark after Stage 1. |
| **Save / replay system** | Full tick snapshots are expensive. Consider event-sourcing: log all changes; replay from tick 0 to reconstruct any state. |
