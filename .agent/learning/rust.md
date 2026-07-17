## 2023-11-20 - Finance Corporate Tax Coverage
**Learning:** Found a significant gap in coverage within the core simulation phase `src/sim/finance.rs` specifically for corporate tax deduction (`process_corporate_taxes`). Finance handles both typical operations (interest payments, deposit yields) and edge conditions (bankruptcy, liquidations). Testing boundary conditions like a company with zero or extremely low cash is critical since taxation skips those entities, and verifying the exact flow of funds (deduction from company and addition to empire treasury) is vital.
**Action:** When testing finance or similar economic/simulation systems, always write an isolated unit test for the happy path (where all constraints are met and funds correctly move from point A to B) and separate tests for the skipping/boundary conditions (bankrupt statuses, low values, disconnected entities).

## 2025-02-13 - Test Structure and Code Review Additions
**Learning:** Adding new unit tests requires strictly placing them within the existing `#[cfg(test)] mod tests` module at the bottom of the source file, not at the global file scope, to ensure they compile correctly and don't pollute the production namespace. We must also explicitly avoid appending garbage `.patch` files to the repository.
**Action:** Always parse the structure of the `#[cfg(test)] mod tests { ... }` block properly when appending new test cases to existing files. Always clean up intermediate `.patch` or scratch files before submitting.
## $(date +%Y-%m-%d) - Testing Private Core Simulation Functions
**Learning:** Private utility functions like `request_loan` in core simulation modules (e.g. `sim/decisions.rs`) are best tested by adding direct unit tests inside the same file's `mod tests` block. This allows bypassing privacy boundaries to thoroughly test edge cases (like debt ratio limits or missing banks) without having to construct the massive simulation context required by the public `run_decisions` entry point.
**Action:** When targeting uncovered private helpers in simulation modules, inject tests directly into the inline `#[cfg(test)] mod tests` block, utilizing localized mock states (e.g., `make_state_with_bank()`) tailored to the specific helper rather than full system integration tests.

## 2026-07-16 - SimState Component Coverage
**Learning:** Functions related to overall simulation state management (`sim/state.rs`) such as treasury components or summary calculation functions are important and easily testable without requiring complex mock setup of the database. Since state acts as the central datastore for the simulation phase, it's very important to thoroughly test components like `SimState::generate_summary` which combines multiple values.
**Action:** When working on generic components in the simulation state, write isolated unit tests that explicitly construct a targeted simulation state and assert changes directly.
## $(date +%Y-%m-%d) - Testing Finance Deposit Interest & Bankrupt Liquidation
**Learning:** In `src/sim/finance.rs`, edge cases such as a bank lacking sufficient cash to pay deposit interest (`test_deposit_interest_bank_insufficient_cash`) and a bankrupt company paying off its debt with no remaining inventory (`test_bankrupt_company_liquidation`) represent significant logic branches that are vital to test. These situations trigger distinct status changes (e.g. "liquidated") and non-standard mathematical results (e.g., partial interest yields) that must be verified against state.
**Action:** When working on simulation economic systems, ensure tests specifically construct minimal states forcing out-of-bounds or zero-cash edge cases to trigger and verify failure/fallback pathways.

## 2024-07-16 - Handling Nested Optional Relationships in Tests
**Learning:** When testing high-level simulation logic (like banking AI) that navigates deep relational chains in the in-memory state (e.g., Company -> City -> CelestialBody -> StarSystem -> Sector -> Empire), the test setup must populate the entire chain of entities. Missing even one link (like `StarSystem` or `Sector`) will cause the test logic to skip or panic depending on whether it uses `get()` or indexing `[]`.
**Action:** When mocking dependencies for a specific module, ensure all dependent sub-structures required for conditional branches (like evaluating prime rates tied to an empire) are initialized and inserted into `SimState`.

## 2026-07-17 - Mismatched Scale in Simulation Consumption and Fulfillment
**Learning:** In multi-phased economic simulations, different phases might use scaled values for performance or balancing. We discovered a 1000x mismatch where `run_consumption` posted buy orders scaled by `population / 1000`, but `update_population_dynamics` evaluated food fulfillment against the raw `population`. This created a perpetual starvation loop.
**Action:** When designing or refactoring multi-phase simulations, always verify that the mathematical scales, dimensions, and denominators are consistent across all phases.

## 2026-07-17 - Closed-Loop Cash and Production Debt Mechanisms
**Learning:** Under a closed-loop economy model, blocking production/refining when cash drops below labor costs (while allowing mining to run on debt) creates a deadlock. Because refineries/plantations cannot produce, miners cannot sell raw materials, leaving the entire economy permanently frozen at zero cash.
**Action:** Ensure that all productive actors in a closed-loop economy can run on similar debt/shortfall mechanisms (or credit facilities) to prevent permanent freezes.

## 2026-07-17 - Redirecting Sentinel Entity Transactions
**Learning:** Sentinel company IDs (negative integers) are useful for bypassing standard limits in simulation logic, but they trigger database foreign-key constraints if persisted to shared tables (like inventories). Moreover, relief goods purchased by sentinels must be redirected directly to local consumer company inventories so they can be consumed by citizens.
**Action:** For all sentinel-initiated transactions, resolve their destination to a valid positive actor (e.g., local consumer company) at the boundary of order matching to maintain database and simulation integrity.
