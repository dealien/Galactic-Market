# Rust Learning Journal

This journal tracks specific, architectural, and systemic learnings from working with this codebase.

## 2026-06-15 - State Serialization and Continuous Operation
**Learning:** The simulation requires that all mutable state must be correctly saved to the database on shutdown, and correctly loaded back into memory on initialization. New fields added to `SimState` must have corresponding explicit mapping in `src/db/load.rs` and `src/sim/state.rs` (`flush` method) to ensure continuity.
**Action:** Always check `src/db/load.rs` and the `flush()` method in `src/sim/state.rs` when adding new state tracking vectors, hashmaps, or fields to `SimState`.

## 2026-07-02 - Empire Sentinel IDs in Market Operations
**Learning:** The simulation uses negative `company_id` values as "sentinel" IDs for special entities participating in the market, such as Empire Relief (which buys food for starving cities). Market sorting, clearing, and verification logic must handle these negative IDs correctly and not assume all `company_id`s refer to standard corporate entities in the `state.companies` hashmap.
**Action:** When implementing new market logic, ensure negative `company_id`s do not trigger errors (e.g. panics on missing companies) and correctly map to empire-level reserves or consumer inventories.

## 2026-07-10 - Voiding Orders During Matching
**Learning:** During the market clearing phase (`src/sim/markets.rs`), participants can run out of cash before fulfilling an order. If an order cannot be processed due to insufficient funds, the matching engine explicitly sets `actual_buyer_cash` logic to limit `affordable_by_buyer`. However, if cash is fully depleted mid-loop, `affordable_by_buyer` becomes 0, meaning `qty` is 0. This must not cause an infinite loop, and there is explicit logic to check for this and `break` or adjust limits.
**Action:** When updating order matching algorithms, always include explicit tests for the edge case where an order matches on price but the participant cannot afford it, verifying the correct state cleanup via voiding mechanisms.

## 2026-07-19 - Simulating Graph Disconnections and Time Events
**Learning:** In the `sim::logistics` module, the simulation caches all-pairs shortest paths for star systems using `build_system_distances`. Testing distance recalibrations due to events (like a `blockade_lane` active event) requires correctly formatting the event tuple `target_id: Some((1, 2))` and incrementing `state.blockade_version` to trigger a recompute. Time-based logic in logistics relies purely on the `arrival_tick` comparison against `current_tick` being `<=` to pop out of `state.trade_routes`.
**Action:** When testing time-based delivery logic, always set `arrival_tick` exactly, explicitly pass smaller/equal ticks to `run_logistics`, and assert that the expected structures (`trade_routes`, `inventories`) mutate accordingly. When testing cached pathfinding variations, mock `state.system_lanes` and increment cache versioning variables.

## 2026-07-21 - Testing Population Dynamics and Migration
**Learning:** In the `sim::consumption` module, simulating population dynamics (`update_population_dynamics`) involves tracking food fulfillment based on an explicit mathematical relationship with `POPULATION_GROWTH_RATE`, `POPULATION_DECLINE_RATE`, and `POPULATION_STARVATION_RATE`. Tests must set up specific mock `SimState` instances containing a City, Consumer Company, and Inventory with carefully calculated resource quantities to hit the various interpolation thresholds (e.g. `FOOD_FULFILLMENT_DECLINE_MIN`). For migration, testing must mock an entire empire hierarchy (Sector, StarSystem, CelestialBody) containing multiple cities with divergent `CityFoodBalance` ratios, ensuring `state.tick` correctly hits the `MIGRATION_INTERVAL` check.
**Action:** When testing simulation modules relying on complex hierarchical dependencies or time-interval checks, explicitly mock the entire required chain in `SimState` and carefully manipulate `state.tick`.

## $(date +%Y-%m-%d) - SimState Setup and Missing Entity Handling
**Learning:** Functions that iterate over multiple cross-referenced entities within `SimState` (e.g. `analyze_city_food_balance` scanning cities, consumer ids, and inventories) must gracefully handle missing references since the simulation state is highly dynamic. For instance, when analyzing food balances, `food_resource_id` and `consumer_co_id` could be `None`. The logic rightly defaults `food_in_inventory` to 0.0 in these cases.
**Action:** When testing simulation components that rely on interrelated entities, actively construct test cases where specific relationships (like a missing resource type) are broken. These missing-data edge cases are common in dynamic simulations and provide crucial coverage for fallback and default behaviors.

## $(date +%Y-%m-%d) - Appending Tests hygiene and File cleanup
**Learning:** Adding unit tests at the end of files without verifying if the file already ends with a `}` (e.g. `mod tests { ... }`) can result in compilation errors due to misplaced or missing braces. Also, generating temporary text and python scripts directly in the repo root without cleaning up leads to code review failures due to poor repository hygiene.
**Action:** Always parse the file to correctly insert new tests inside the `#[cfg(test)] mod tests` block rather than appending blindly. Delete any scratchpad text files, `lcov.info`, or helper scripts immediately after use.

## $(date +%Y-%m-%d) - Overcoming Coverage Gaps in Market Sorting Logic
**Learning:** In systems like the market engine where `market_orders` are matched, the use of isolated "single-buyer vs. single-seller" test cases can inadvertently bypass critical sorting closures designed to handle competition. When sorting logic acts on collections, testing with one element results in the sorting logic skipping branch comparisons entirely, leading to gaps in coverage.
**Action:** When testing matching engines or systems that process a collection of inputs (e.g. orders, production queues), explicitly create test scenarios that seed *multiple competing inputs* per category (e.g., several buyers at different prices/kinds competing for a single seller) to ensure the sorting priority and matching precedence loops are actively executed and validated.

## 2024-07-27 - Commercial Bank Lender of Last Resort Testing
**Learning:** Testing logic dependent on implicit rates (like `prime_rate`) can be brittle if the state setup doesn't match the environment conditions precisely. The Central Bank's AI evaluation runs first and mutates the `prime_rate` based on the ratio of debt to cash. If a test simulates an environment with high cash but zero debt, the `prime_rate` adjusts down, altering expected penalty interest rates for downstream evaluations (like Commercial Banks taking emergency loans).
**Action:** Always verify other AI evaluation paths that execute *before* the target path within `run_decisions`, and explicitly assert against values resulting from those prior steps rather than static initial conditions.

## 2026-07-29 - Default trait usage inside of SimState nested objects
**Learning:** Instantiating structs within `src/sim/state.rs` often requires fully-qualifying member types or using explicit constructors since they might lack generic `Default` derives or standard parameters (e.g. `City`, `Company`, `Facility`). Tests that need nested `SimState` elements should manually set every necessary field (and watch out for type mismatches, e.g., using `100.0` instead of `100` for an `i32` capacity) rather than trying to default unneeded data. Also, watch out for missing fields inside constructors for structs like `Company` which have new runtime fields added over time.
**Action:** When creating tests, define nested objects deliberately and check `src/sim/state.rs` for exactly what fields must be set to initialize objects.

## 2025-03-05 - Performance patterns for sorting
**Learning:** In the market matching tick loop, `sort_by` was being used for sorting orders which guarantees stable sorting (maintaining FIFO for matching prices). An attempt to use `sort_unstable_by` caused a test failure because order matching became non-deterministic for limit orders with the same price, violating the exact preservation of functionality rule.
**Action:** Do not use `sort_unstable_by` when stable sorting (FIFO behavior) is required, such as in order book matching algorithms.

## 2025-02-18 - SimState Instantiation and Implicit Struct Initialization
**Learning:** When writing tests that directly manipulate the `SimState` structs (like `Occupation` or `SectorControl`), the structs may have fewer fields than assumed if one attempts to populate them exhaustively based on older knowledge or generalized assumptions. Specifically, `StarSystem` has no `x`, `y`, or `status` fields; `Occupation` requires `system_id`, `occupier_empire_id`, and `since_tick`; and `SectorControl` uses an `empire_system_counts` HashMap and `total_systems` instead of a flat `controlling_empires` vector.
**Action:** When manually mocking `SimState` entities for a test, always carefully inspect the `SimState` struct definition (or rely on compiler errors and adapt) to construct valid structs, especially complex nested state like `SectorControl`.

## 2026-08-01 - Optimizing String Allocations in Market Clearing Tick Loop
**Learning:** In the highly sensitive market order tick loop (`src/sim/markets.rs`), using `.clone()` on strings (`o.order_kind.clone()`) on every single tick for every order introduces substantial unnecessary heap allocation overhead.
**Action:** Replace string cloning with strict boolean flags derived once by reference (`o.order_kind == "market"`) when parsing elements within the tick loop.

## 2025-02-27 - SimState HashMap iteration order test flakiness
**Learning:** Testing logic that iterates over non-deterministic collections like `HashMap` (e.g. tracking payment exhaustions across multiple loans) can lead to flaky assertions if the test assumes a specific iteration order.
**Action:** When asserting against side-effects of iterating over `state.loans` or similar `HashMap`s, write assertions that are invariant to the order of operations, such as checking that *one of* a set of expected states is true for specific entries, rather than hardcoding exact values that depend on a specific iteration sequence.

## 2026-08-03 - Pre-allocated tuples for market matching
**Learning:** Sorting multiple `MarketOrder` objects in the market clearing loop involves many string comparisons and boolean logic. Pre-calculating the sort criteria as a tuple when filling the initial Vec (e.g. `buys.push((id, is_market, order.price))`) avoids repeated calculations during sorting, which optimizes the tick loop hot path.
**Action:** When sorting complex objects in hot loops, consider the Schwartzian transform (caching the sort keys in a tuple alongside the original ID or data) to minimize recalculations and string comparisons.

## 2024-08-04 - Unstable sorting requires deterministic tie-breakers
**Learning:** When switching from `sort_by` to `sort_unstable_by` for performance optimization in a tick loop (like market order sorting), it's crucial to explicitly break ties (e.g. by `created_tick` and then `id`). `sort_unstable_by` does not preserve original order, and if ties exist, it may randomly re-order orders with the same price, violating deterministic simulation ticks across different seeds or architectures.
**Action:** Always include deterministic fallback comparisons (`.then_with(|| a_tick.cmp(&b_tick)).then_with(|| a_id.cmp(&b_id))`) when using `sort_unstable_by` on collections of structs that might share primary sorting keys, especially in tick-loop hot paths.

## 2024-05-18 - Missing Docstrings Discovered in Review
**Learning:** When adding new test functions, docstrings (`///`) must be strictly applied above the `#[test]` attribute for every function to satisfy testing conventions and review standards.
**Action:** When creating new tests in the future, always include a behavior-explaining docstring immediately before `#[test]`.

## 2025-02-27 - SimState HashMap iteration order test flakiness
**Learning:** Testing logic that iterates over non-deterministic collections like `HashMap` (e.g. tracking payment exhaustions across multiple loans) can lead to flaky assertions if the test assumes a specific iteration order.
**Action:** When asserting against side-effects of iterating over `state.loans` or similar `HashMap`s, write assertions that are invariant to the order of operations, such as checking that *one of* a set of expected states is true for specific entries, rather than hardcoding exact values that depend on a specific iteration sequence.

## 2026-08-03 - Pre-allocated tuples for market matching
**Learning:** Sorting multiple `MarketOrder` objects in the market clearing loop involves many string comparisons and boolean logic. Pre-calculating the sort criteria as a tuple when filling the initial Vec (e.g. `buys.push((id, is_market, order.price))`) avoids repeated calculations during sorting, which optimizes the tick loop hot path.
**Action:** When sorting complex objects in hot loops, consider the Schwartzian transform (caching the sort keys in a tuple alongside the original ID or data) to minimize recalculations and string comparisons.

## 2024-08-04 - Unstable sorting requires deterministic tie-breakers
**Learning:** When switching from `sort_by` to `sort_unstable_by` for performance optimization in a tick loop (like market order sorting), it's crucial to explicitly break ties (e.g. by `created_tick` and then `id`). `sort_unstable_by` does not preserve original order, and if ties exist, it may randomly re-order orders with the same price, violating deterministic simulation ticks across different seeds or architectures.
**Action:** Always include deterministic fallback comparisons (`.then_with(|| a_tick.cmp(&b_tick)).then_with(|| a_id.cmp(&b_id))`) when using `sort_unstable_by` on collections of structs that might share primary sorting keys, especially in tick-loop hot paths.

## $(date +%Y-%m-%d) - Optimizing Simulation Hot Paths via Direct Partitioning
**Learning:** To optimize simulation hot paths, avoid creating intermediate vectors of IDs for grouping. Instead, directly partition cached data tuples into their final target collections (e.g., `HashMap<Key, (Vec<Tuple>, Vec<Tuple>)>`) to reduce allocation overhead and prevent redundant map lookups during iteration.
**Action:** When gathering entities for paired processing (like buys/sells or attackers/defenders), build a struct or tuple containing all needed properties and distribute them directly into partitioned vectors within a single pass over the source map.

## 2024-05-18 - Finance Phase Loan Interest Structure Learning
**Learning:** `Loan` state models interest payments flowing dynamically from the borrower's cash explicitly into the lender company's cash (`Company::cash`) *only if* the `lender_company_id` is set to a valid, active company (e.g. a `commercial_bank`).
**Action:** When mocking state to test loan flows, ensure the lender is created in `state.companies` and its ID is mapped on `loan.lender_company_id`. Otherwise, the interest acts purely as an economic sink (money destroyed).

## 2025-02-20 - Testing Random Events Based on Probabilities
**Learning:** Testing logic that relies on `rng.gen_bool(prob)` can be difficult to hit consistently with a single test run without injecting a custom RNG interface.
**Action:** When testing code containing probabilistic branches (e.g. `rng.gen_bool(0.05)` in event loops), instead of mocking the RNG or guessing a magic seed, it is effective to run the target function in a deterministic loop (e.g. 100 iterations) with a standard seeded RNG until the condition is met, verifying that the branch is eventually reached and handles state correctly.

## 2024-03-24 - Exhaustive Edge Case Testing via Manual State Construction
**Learning:** Simulation events logic relies on multiple disparate systems (e.g. relations, traits, treaties) interconnected within `SimState`. Because these state structs may be sparsely populated unless carefully seeded, achieving 100% path coverage in complex nested logic (like `has_conflicting_alliances`) necessitates precise state instantiation mimicking legacy behavior constraints instead of merely exercising the happy paths.
**Action:** When writing state-dependent tests for deeply nested logic, prioritize constructing minimal, exact `SimState` fixtures directly in test functions mapping edge case invariants rather than relying on generic shared setups.

## 2024-05-15 - Structure Imports in Sim
**Learning:** Core simulation data structures like `MarketOrder`, `ActiveEvent`, and `City` are defined in `crate::sim::state`, not in a separate `models` module as is common in some other frameworks.
**Action:** When creating setup data for tests (e.g. inserting into `state.market_orders`), always import or qualify with `crate::sim::state::` rather than `crate::models::` or `crate::sim::models::`.

## 2024-08-13 - Eliminate Hashmap Lookups from Market Clearing Tick Loop
**Learning:** In the core simulation hot loop (`src/sim/markets.rs`), repetitively looking up orders by ID in `state.market_orders` via `.get_mut()` inside the `while` match loop, and enthusiastically removing them via `.remove()`, caused a major performance bottleneck due to hash collisions and re-allocation overhead.
**Action:** Extend the initial sorting `OrderKey` tuple to hold the `quantity`. Mutate this local value during the loop `O(1)`, and write back all modified quantities in bulk outside the loop. Use `order.quantity = 0` as a tombstone for voided orders instead of inline `.remove()`, deferring cleanup to the existing `.retain()` sweep at the end of the phase. This prevents state leaks while dramatically dropping CPU cycles.

## 2026-08-16 - Extracting Loop-Invariant Lookups
**Learning:** In simulation hot paths (like `clear_orders` in `src/sim/markets.rs`), ensure loop-invariant `HashMap` lookups (e.g., retrieving constant attributes for a `city_id` during a trading matching loop) are extracted and cached outside inner loops to avoid redundant O(1) map lookup overheads.
**Action:** When iterating over combinations in an outer loop (e.g., `for ((city_id, _), ...)`), always pre-calculate properties dependent solely on the outer keys before entering the inner matching loop.

## 2026-08-15 - Optimizing Merchant Inventory Lookup
**Learning:** In `src/sim/decisions.rs`, the function `compute_merchant_opportunities` checked for available inventory using an `iter().any(...)` over the entire `state.inventories` HashMap. Because this lookup occurred inside a nested loop over resources and cities, the O(N) scan created a significant performance bottleneck.
**Action:** Replaced the O(N) `iter().any(...)` scan with an O(1) `HashMap::get()` lookup using the exact composite tuple key `(merchant_id, origin_city_id, res_id)`. This reduced the opportunity scan time considerably and eliminated redundant iterations.

## 2024-05-18 - Pre-allocating HashMap Entry Collections
**Learning:** In the `markets.rs` tick loop, using `.or_default()` on a `HashMap::entry` allocates default vectors with a capacity of 0. When pushing items to these vectors immediately after, it forces dynamic resizing and heap allocations during the hot loop.
**Action:** Replace `.or_default()` with `.or_insert_with(|| (Vec::with_capacity(N), Vec::with_capacity(M)))` when the approximate size is known, to eliminate dynamic resizing overhead in hot loops.

## 2024-10-24 - Serializing Database Integration Tests
**Learning:** When writing database-dependent integration tests that use `clear_database` (which resets the public schema), tests must be executed serially to prevent concurrent execution from causing race conditions and schema conflicts like 'schema public already exists'. Ensure you use the `serial_test` crate with the `#[serial]` macro on conflicting tests, and use `IF NOT EXISTS` for schema creation in the reset logic.
**Action:** Always add `#[serial]` to `#[tokio::test]` functions in integration tests that mutate or reset the shared test database.

## 2024-05-18 - Optimizing the merchant opportunity scan hot loop
**Learning:** O(R * C^2) loops (like computing arbitrage routes across resources, origin cities, and destination cities) can be dramatically sped up by short-circuiting expensive map lookups (like transport distance costs) when `sell_price <= buy_price`. Furthermore, hoisting loop-invariant hashmap lookups (like fetching the merchant's home city ID) outside the innermost loops prevents redundant O(1) allocation overheads.
**Action:** When working with nested loops scanning large cross-products of IDs, evaluate condition checks in order of computational expense. Put simple mathematical comparisons (`sell_price <= buy_price`) *before* expensive state lookups (`get_transport_info`) to short-circuit the loop early and realize massive micro-optimization speedups (e.g. ~10x). Always pre-fetch constant attributes (like the evaluating merchant's home city) before entering the loop.

## $(date +%Y-%m-%d) - Testing Merchant Logistics and Trade Routes
**Learning:** When testing logic that evaluates trade routes or arbitrage opportunities (like merchant shipping decisions in `src/sim/decisions.rs`), the `SimState` setup must explicitly initialize multiple distinct cities (and related metadata like `ema_prices`) to satisfy internal destination loops (`for &dest_city_id in state.cities.keys()`) and correctly trigger cross-city evaluation logic.
**Action:** When testing merchant or logistical logic, always seed `SimState` with at least two cities (an origin and a destination) and set appropriate varying EMA prices to trigger the expected trading behavior (shipping vs. local selling).
