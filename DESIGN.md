# Galactic Market Simulator

## Project Design & Development Guide

Galactic Market Simulator — Project Guide
v0.2 — Draft

**Stack:** Rust · SQL/PostgreSQL · WebAssembly/SvelteKit

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
| `celestial_bodies` | `id, system_id, body_type, mass, habitable, population_cap` | Planets, moons, stations |
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

The simulation advances in discrete ticks (e.g., 1 tick = 1 simulated day or week). Each tick the engine performs a fixed sequence of phases:

| Phase | Operations | Key Tables Written |
| --- | --- | --- |
| 1. Resource | Advance extraction jobs; deplete deposits; trigger discovery checks | `deposits`, `production_jobs` |
| 2. Production | Advance all production_jobs; consume inputs; create output inventory | `production_jobs`, `inventory` |
| 3. Logistics | Advance in-transit shipments; deliver cargo at destination | `trade_routes`, `inventory` |
| 4. Markets | Match buy/sell orders; compute clearing prices; record history | `market_orders`, `market_history` |
| 5. Finance | Pay wages, loan interest, dividends; update cash/debt balances | `companies`, `employees` |
| 6. Decisions | Each company AI evaluates state and queues new jobs/orders/routes | `production_jobs`, `market_orders` |
| 7. Population | Update city populations; shift consumer demand; migrate labor | `cities`, `employees` |
| 8. Politics | Advance diplomatic tension; resolve treaties; apply tax changes | `diplomatic_relations`, `treaties` |
| 9. Events | Roll random events; apply effects; log to events_log | `events_log`, *(affected tables)* |
| 10. Flush | Periodically write dirty state to DB; always append `events_log` and `market_history` deltas; update tick counter | `simulation_meta`, `events_log`, `market_history`, *(dirty tables)* |

> **Note:** Each phase should be implementable as a separate Rust module. The tick loop runs entirely in memory; the periodic DB flush uses an atomic transaction so a crash between flushes can recover to the last clean checkpoint rather than landing in a partially-advanced state.

---

## 5. Rust Implementation Architecture

Rust is an excellent choice for this project. The simulation is CPU-bound, handles large volumes of data per tick, and needs to be fast enough to simulate many ticks in reasonable time. Rust's ownership model also prevents entire classes of bugs that would be catastrophic in a long-running simulation.

### 5.1 Recommended Crates

| Crate | Purpose | Notes |
| --- | --- | --- |
| `sqlx` | Async SQL queries with compile-time checking | Supports PostgreSQL; queries checked against DB schema at compile time |
| `tokio` | Async runtime | sqlx and most IO crates are built on tokio; use `tokio::main` |
| `serde` / `serde_json` | Serialization | Used for config files, `effects_json` in events, API responses |
| `rand` / `rand_distr` | Random number generation | For event rolls, procedural generation, market noise |
| `petgraph` | Graph data structures | Model the star system jump network; run Dijkstra for routing |
| `clap` | CLI argument parsing | Control tick speed, load scenarios, etc. |
| `tracing` / `tracing-subscriber` | Structured logging | Far better than `println!` for a complex simulation; filterable by module |
| `anyhow` / `thiserror` | Error handling | `anyhow` for application errors; `thiserror` for library error types |
| `rayon` | Data parallelism | Parallelize per-company decision phase across CPU cores |
| `axum` | HTTP server *(future)* | For the REST API layer that will serve the web UI |

### 5.2 Project Structure

```text
galactic-sim/
  Cargo.toml
  src/
    main.rs              # CLI entry point, tick loop
    db/                  # Database connection pool, migrations
      mod.rs
      migrations/        # SQL migration files
    sim/                 # Core simulation modules (one per tick phase)
      mod.rs
      resources.rs       # Phase 1: extraction & depletion
      production.rs      # Phase 2: manufacturing
      logistics.rs       # Phase 3: cargo transit
      markets.rs         # Phase 4: order matching, price history
      finance.rs         # Phase 5: wages, interest, dividends
      decisions.rs       # Phase 6: company AI
      population.rs      # Phase 7: demographics
      politics.rs        # Phase 8: diplomacy, war
      events.rs          # Phase 9: random events
    models/              # Rust structs mirroring DB tables
      company.rs
      market.rs
      geography.rs
      ...
    config.rs            # Simulation parameters (tick speed, etc.)
    worldgen/            # Optional: procedural universe generation
```

### 5.3 Company Decision AI

Each company runs a lightweight decision algorithm each tick. Rule-based heuristics produce surprisingly rich emergent behavior:

- **Miners:** check market price for their extractable resources; extract if `price > cost + margin_threshold`; idle or diversify otherwise
- **Manufacturers:** check recipe profitability (`output_price − input_costs − labor − facility_depreciation`); queue production if profitable
- **Traders:** scan all reachable market pairs for arbitrage opportunities (`buy_price_here + transport_cost < sell_price_there`); execute if margin > threshold
- **Investors:** if `cash_balance > capex_threshold`, evaluate ROI of expanding facilities vs. acquiring a competitor vs. paying dividends

> **Note:** The richness of the simulation scales with the sophistication of these heuristics. Start simple (greedy, local information only) and add complexity once the basic loop works.

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

## 6. Political Simulation

Politics is the second simulation layer sitting above economics. It does not replace economic logic — it modifies it. Wars raise taxes and disrupt trade; alliances open new markets; political instability increases risk premiums.

### 6.1 Diplomatic States

| State | Economic Effect | Trigger Conditions |
| --- | --- | --- |
| Allied | Open borders; shared intelligence; possible joint taxation | Treaty signed; mutual defense activated |
| Neutral | Normal trade; standard tariffs | Default state; post-war cool-down |
| Cold War | Embargoes possible; higher tariffs; espionage events active | Tension > threshold without declaration |
| War | Blockades; territorial seizures; supply chain disruption; defense spending spike | Formal declaration or border incident |
| Occupation | Occupied territories taxed at higher rate; resistance events | War victory condition met |

### 6.2 Political Event Types

- **Leadership change** — new ruler may change tax policy, alliances, or trigger military buildup
- **Election / Coup** — faction government type may shift; economic policy uncertainty spike
- **Trade deal signed** — tariff reduction between factions; new trade route profitability
- **Blockade declared** — specific jump lane(s) blocked; prices diverge between systems
- **Sanction imposed** — specific goods cannot cross faction borders; new smuggling opportunity
- **Rebellion** — city or sector breaks away; creates a new mini-faction or joins neighbor

### 6.3 War Mechanics (Simplified)

Full military simulation is out of scope for v1. A simplified war model:

- Wars have a **theater** (set of contested systems/sectors)
- Each tick, `military_strength` scores are compared with dice rolls + terrain + supply line modifiers
- Outcomes: territory changes, infrastructure damage to random cities in theater, economic disruption events
- War ends when one side's territory falls below `capitulation_threshold` or a peace treaty is accepted
- War cost model: defense spending rises; consumer goods production capacity falls; debt rises

---

## 7. Random Event Engine

Events are the third simulation layer. They inject irreducible uncertainty — even a perfectly-managed company can be disrupted by a supernova remnant gas cloud blocking a trade lane, or a charismatic leader dying at a critical moment.

### 7.1 Event Definition Structure

Events are defined in a configuration file (TOML or JSON), not hard-coded. Each event definition has:

- `id` and `name`
- `scope` — what entities can be targeted (city, planet, system, sector, empire, company, person)
- `trigger_weight` — base probability per tick per eligible entity
- `trigger_conditions` — optional filter (e.g., only fires if `city.infrastructure_lvl < 3`)
- `effects` — a list of modifiers applied on firing (parameterized with ranges for variability with the strength of the event)
- `duration_ticks` — how long the effect persists (`0` = instant)

### 7.2 Sample Event Catalog

| Event | Scope | Effect |
| --- | --- | --- |
| Asteroid Strike | City | Infrastructure damage; population loss; resource crater (new deposit) possible |
| Disease Outbreak | City / Planet | Population decline; labor shortage; demand spike for medical goods |
| Tech Breakthrough | Company / Empire | Recipe efficiency +N%; possible new recipe unlocked |
| Megacorp Scandal | Company | Credit rating drop; stock price crash; acquisition vulnerability |
| Resource Discovery | Continent | New deposit revealed; local price crash for that resource type |
| Pirate Surge | Star System | Trade route risk increases; insurance costs rise; possible cargo seizures |
| Political Assassination | Person | Diplomatic tension +N; possible government type change; treaty renegotiation |
| Solar Flare | Star System | Communications disrupted; in-system transit slowed for N ticks |
| Infrastructure Boom | Sector | Construction costs temporarily reduced; rapid city development |
| Famine | Planet | Food demand spikes; imports surge; population growth halted |

---

## 8. Development Roadmap

This is a large project. Tackling it in stages prevents overengineering and ensures there is always a working (if simple) simulation to experiment with.

### Stage 0 — Foundation

**Goal:** Rust project boots, connects to Postgres, creates schema, seeds a tiny universe, runs a tick loop.

- [ ] Set up Cargo workspace; add `sqlx`, `tokio`, `tracing`, `clap`
- [ ] Write initial SQL migration: `empires`, `sectors`, `star_systems`, `cities`, `resource_types`
- [ ] Write world-gen seed script: 2 empires, 4 systems, 8 planets, 32 cities, 3 resource types
- [ ] Implement tick loop: increment tick counter and log — nothing else yet
- [ ] **Verify:** `cargo run -- --ticks 100` completes without error

### Stage 1 — Basic Economy

**Goal:** Resources flow through a single production chain; prices change.

- [ ] Implement `deposits` table and extraction phase (miners consume deposit, produce inventory)
- [ ] Implement markets and order book (companies post sell orders; simple price averaging)
- [ ] Implement one production recipe (ore → ingots)
- [ ] Implement trade routes (instant delivery for now; model timing later)
- [ ] Implement basic company AI: mine if `price > cost`; sell output
- [ ] **Verify:** `market_history` fills with price data; `deposit.size_remaining` declines

### Stage 2 — Company Lifecycle

**Goal:** Companies grow, hire, invest, go bankrupt.

- [ ] Add finance phase: wages, loan interest, cash balance tracking
- [ ] Implement freelancer → company promotion logic
- [ ] Implement facility construction (company spends cash, facility appears after N ticks)
- [ ] Implement bankruptcy detection and asset liquidation
- [ ] Implement acquisition logic (simple version: cash offer accepted if > book value)
- [ ] **Verify:** observe a company growing from freelancer to corp over a long run

### Stage 3 — Geography & Logistics

**Goal:** Transport costs and times matter; arbitrage opportunities exist.

- [ ] Add transit time to trade routes (ETA based on distance and mode)
- [ ] Implement jump lane graph; use Dijkstra for cheapest path routing
- [ ] Add transport cost to market order pricing (prices differ between cities)
- [ ] Implement arbitrage AI for trading companies
- [ ] **Verify:** prices in isolated systems diverge from connected ones; traders equalize them

### Stage 4 — Politics & Events

**Goal:** Wars and events disrupt the economy in visible ways.

- [ ] Implement `diplomatic_relations` table and tension mechanic
- [ ] Implement blockade event (lane blocked; prices diverge)
- [ ] Implement war phase (territory rolls; infrastructure damage)
- [ ] Implement event engine and first 10 event definitions
- [ ] Implement random seed configuration for reproducible runs
- [ ] Implement lore seed pipeline: a script that parses markdown/JSON exports from a world-building tool (e.g., Obsidian) and generates Postgres `COPY`-compatible seed files for planets, factions, and resources — write lore naturally, not as hand-typed `INSERT` statements
- [ ] **Verify:** declare war between two empires; observe trade collapse and price spikes

### Stage 5 — Web UI

**Goal:** A browser-based dashboard to watch the simulation in real time.

- [ ] Add `axum` HTTP server to the Rust binary (separate thread from tick loop)
- [ ] Expose REST endpoints: `GET /systems`, `GET /markets/{id}/history`, `GET /companies`, etc.
- [ ] Build SvelteKit frontend; use D3.js or deck.gl for galactic map
- [ ] Add charts for price history (recharts or Chart.js)
- [ ] Add company explorer: net worth, production, trade routes
- [ ] **Verify:** run simulation for 1000 ticks; explore results in browser

### Stage 6 — God Mode

**Goal:** Player can observe and intervene.

- [ ] `POST /events/trigger` — manually fire any event definition
- [ ] `POST /companies` — create a player-controlled company
- [ ] `PATCH /companies/{id}` — edit company cash, strategy, facilities
- [ ] `POST /diplomacy/declare-war` — force a conflict
- [ ] WebSocket feed: live event stream to the UI

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
