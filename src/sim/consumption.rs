//! Population consumption and food fulfillment dynamics.
//!
//! Simulates citizen demand for refined and consumer goods using accumulated city wages,
//! and updates population growth/decline depending on food fulfillment thresholds.

use std::collections::HashMap;
use tracing::debug;

use crate::sim::state::{MarketOrder, SimState};

/// Ingots demanded per 1,000 citizens per tick.
const DEMAND_PER_1K_POPULATION: i64 = 1;

/// Issue #10: Population fulfillment thresholds for growth/decline
/// - Above 95% food: population grows +0.05% per tick
/// - 70-95% food: population stable
/// - 40-70% food: population declines -0.1% per tick
/// - Below 40% food: famine starvation -0.5% per tick
const FOOD_FULFILLMENT_GROWTH_THRESHOLD: f64 = 0.95;
const FOOD_FULFILLMENT_STABLE_MIN: f64 = 0.70;
const FOOD_FULFILLMENT_DECLINE_MIN: f64 = 0.40;

const POPULATION_GROWTH_RATE: f64 = 0.0005; // +0.05% per tick
const POPULATION_STABLE_RATE: f64 = 0.0; // No change
const POPULATION_DECLINE_RATE: f64 = -0.001; // -0.1% per tick
const POPULATION_STARVATION_RATE: f64 = -0.005; // -0.5% per tick

/// Phase 6: Population consumption.
///
/// Issue #9/#10: Draw per-capita income from city wage pools (closed-loop economy).
/// Instead of manifesting credits, cities spend accumulated wages on goods.
/// Posts buy orders for all Refined and Consumer goods.
/// After consumption, updates population based on food fulfillment.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::consumption::run_consumption;
///
/// let mut state = SimState::new();
/// run_consumption(&mut state, 1);
/// ```
pub fn run_consumption(state: &mut SimState, current_tick: u64) {
    // Snapshot last known prices for budgeting
    let last_prices = state.price_cache.clone();

    // Identify target resource types (Refined and Consumer Goods)
    let target_resource_ids: Vec<i32> = state
        .resource_types
        .values()
        .filter(|r| r.category == "Refined Material" || r.category == "Consumer Good")
        .map(|r| r.id)
        .collect();

    if target_resource_ids.is_empty() {
        return;
    }

    // Collect (city_id, population, consumer_company_id)
    let consumers: Vec<(i32, i64, i32)> = state
        .city_consumer_ids
        .iter()
        .filter_map(|(&city_id, &company_id)| {
            let population = state.cities.get(&city_id)?.population;
            Some((city_id, population, company_id))
        })
        .collect();

    for (city_id, population, company_id) in consumers {
        // Check for active famine in this city
        let famine_severity = state
            .active_events
            .values()
            .filter(|e| e.event_type == "famine" && e.target_id == Some((city_id, 0)))
            .map(|e| e.severity)
            .sum::<f64>();

        // Issue #9: Draw income from city wage pool instead of manifesting credits
        let available_wages = state.get_wage_pool(city_id);
        let consumption_budget = available_wages * 0.8; // Use 80% of wages, keep 20% as buffer

        if consumption_budget < 0.01 && available_wages < 0.01 {
            // No wages available; skip consumption this tick
            // Population may decline due to lack of food (handled in population update)
            state
                .market_orders
                .retain(|_, order| order.company_id != company_id);
            continue;
        }

        // Deduct wages from pool (Issue #9: closed-loop economy)
        state.withdraw_from_wage_pool(city_id, consumption_budget);

        // Ensure company has cash for orders (fallback to manifested if needed, but log it)
        if let Some(company) = state.companies.get_mut(&company_id) {
            company.cash += consumption_budget;
            if company.cash < 1.0 {
                // Company still has minimal cash; skip
                state
                    .market_orders
                    .retain(|_, order| order.company_id != company_id);
                continue;
            }
        }

        let mut available_cash = 0.0;
        if let Some(company) = state.companies.get(&company_id) {
            available_cash = company.cash;
        }

        // Clear existing orders for this consumer to prevent leaking orders every tick
        state
            .market_orders
            .retain(|_, order| order.company_id != company_id);

        // 2. Budgeting: Split available cash among target resources
        let total_budget = available_cash * 0.8;
        let budget_per_resource = total_budget / target_resource_ids.len() as f64;

        for &res_id in &target_resource_ids {
            let res = &state.resource_types[&res_id];
            let market_price = last_prices.get(&(city_id, res_id)).copied().unwrap_or(20.0);

            // Consumers are willing to pay slightly above market to ensure fulfillment.
            // Spikes significantly during famine for vital resources (food, water).
            let mut bid_modifier = 1.1;
            let mut demand_modifier = 1.0;

            if res.is_vital && famine_severity > 0.0 {
                bid_modifier += famine_severity * 2.0; // Desperation price
                demand_modifier += famine_severity * 3.0; // Inelastic demand spike
            }

            let bid_price = (market_price * bid_modifier).min(1000.0);

            // Demand scales with population
            let ideal_demand =
                ((population / 1000).max(1) * DEMAND_PER_1K_POPULATION) as f64 * demand_modifier;
            let ideal_demand = ideal_demand as i64;

            // Can we afford the ideal demand?
            let affordable_qty = (budget_per_resource / bid_price) as i64;
            let final_qty = ideal_demand.min(affordable_qty);

            if final_qty > 0 {
                let order_id = state.next_order_id();
                state.market_orders.insert(
                    order_id,
                    MarketOrder {
                        id: order_id,
                        city_id,
                        company_id,
                        resource_type_id: res_id,
                        order_type: "buy".into(),
                        order_kind: "limit".into(),
                        price: bid_price,
                        quantity: final_qty,
                        created_tick: current_tick,
                    },
                );
            }
        }

        debug!(city_id, population, "Consumer demand updated for city");
    }

    // Issue #10: Update population based on food fulfillment
    update_population_dynamics(state);

    // Issue #10: Population migration between cities
    run_migration(state);
}

/// Issue #10: Update city populations based on food fulfillment.
///
/// Population growth/decline is based on the ratio of food consumed to food required:
/// - fulfillment > 95%: growth +0.05% per tick
/// - fulfillment 70-95%: stable (0% change)
/// - fulfillment 40-70%: decline -0.1% per tick
/// - fulfillment < 40%: starvation -0.5% per tick
fn update_population_dynamics(state: &mut SimState) {
    let mut city_updates: Vec<(i32, i64, f64)> = Vec::new();

    for (city_id, city) in state.cities.iter() {
        if city.population <= 0 {
            continue;
        }

        // Calculate food fulfillment: actual food consumed / required for full population
        let food_required = city.population as f64;
        let mut food_consumed = 0.0;

        // Find food resource ID
        let food_resource_id = state
            .resource_types
            .values()
            .find(|r| r.name.contains("Food") || r.name.contains("Ration"))
            .map(|r| r.id);

        if let Some(food_id) = food_resource_id {
            // Count food in consumer company inventory for this city
            // Inventories are keyed by (company_id, city_id, resource_type_id)
            if let Some(consumer_co_id) = state.city_consumer_ids.get(city_id)
                && let Some(inv) = state.inventories.get(&(*consumer_co_id, *city_id, food_id))
            {
                food_consumed = inv.quantity as f64;
            }
        }

        let food_fulfillment = if food_required > 0.0 {
            (food_consumed / food_required).min(2.0) // Cap at 200%
        } else {
            1.0
        };

        let growth_rate = if food_fulfillment >= FOOD_FULFILLMENT_GROWTH_THRESHOLD {
            POPULATION_GROWTH_RATE
        } else if food_fulfillment >= FOOD_FULFILLMENT_STABLE_MIN {
            POPULATION_STABLE_RATE
        } else if food_fulfillment >= FOOD_FULFILLMENT_DECLINE_MIN {
            // Linear interpolation between decline and starvation
            let t = (food_fulfillment - FOOD_FULFILLMENT_DECLINE_MIN)
                / (FOOD_FULFILLMENT_STABLE_MIN - FOOD_FULFILLMENT_DECLINE_MIN);
            POPULATION_STABLE_RATE * t + POPULATION_DECLINE_RATE * (1.0 - t)
        } else {
            POPULATION_STARVATION_RATE
        };

        let new_population_f64 = city.population as f64 * (1.0 + growth_rate);
        let new_population = new_population_f64.max(1.0) as i64; // Never go below 1

        if new_population != city.population {
            debug!(
                city_id,
                old_pop = city.population,
                new_pop = new_population,
                fulfillment = food_fulfillment,
                growth_rate,
                "Population updated"
            );
        }
        city_updates.push((*city_id, new_population, growth_rate));
    }

    // Apply population updates
    for (city_id, new_pop, growth_rate) in city_updates {
        if let Some(city) = state.cities.get_mut(&city_id) {
            city.population = new_pop;
            city.population_growth_rate = growth_rate;
        }
    }
}

/// Issue #10: How often migration runs (in ticks).
const MIGRATION_INTERVAL: u64 = 50;

/// Issue #10: Fraction of population that migrates per interval.
const MIGRATION_RATE: f64 = 0.01;

/// Issue #10: Population migration between cities within the same empire.
///
/// Every MIGRATION_INTERVAL ticks, population migrates from low-fulfillment
/// cities to high-fulfillment cities in the same empire. Migration is capped
/// to prevent sudden collapses.
///
/// # Examples
///
/// ```rust
/// use galactic_market::sim::SimState;
/// use galactic_market::sim::consumption::run_migration;
///
/// let mut state = SimState::new();
/// state.tick = 50;
/// run_migration(&mut state);
/// ```
pub fn run_migration(state: &mut SimState) {
    // Only run every MIGRATION_INTERVAL ticks
    if !state.tick.is_multiple_of(MIGRATION_INTERVAL) {
        return;
    }

    // Group cities by empire (via sector → empire mapping)
    let mut empire_cities: HashMap<i32, Vec<i32>> = HashMap::new();
    for (&city_id, city) in &state.cities {
        // Look up empire via: city → body → system → sector → empire
        let empire_id = state
            .celestial_bodies
            .get(&city.body_id)
            .and_then(|b| state.star_systems.get(&b.system_id))
            .and_then(|s| state.sectors.get(&s.sector_id))
            .map(|sec| sec.empire_id);

        if let Some(emp_id) = empire_id {
            empire_cities.entry(emp_id).or_default().push(city_id);
        }
    }

    // For each empire, migrate from low-fulfillment to high-fulfillment
    let mut migrations: Vec<(i32, i32, i64)> = Vec::new(); // (from, to, amount)

    for city_ids in empire_cities.values() {
        if city_ids.len() < 2 {
            continue;
        }

        // Score each city by fulfillment ratio
        let mut scored: Vec<(i32, f64, i64)> = city_ids
            .iter()
            .filter_map(|&cid| {
                let balance = state.city_food_balance.get(&cid)?;
                let pop = state.cities.get(&cid)?.population;
                Some((cid, balance.fulfillment_ratio, pop))
            })
            .collect();

        if scored.len() < 2 {
            continue;
        }

        scored.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

        // Migrate from worst to best (small trickle)
        if let (Some(worst), Some(best)) = (scored.first(), scored.last())
            && best.1 > worst.1 + 0.3
            && worst.2 > 100
        {
            // Migrate up to 1% of worst city's population
            let amount = (worst.2 as f64 * MIGRATION_RATE).max(1.0) as i64;
            migrations.push((worst.0, best.0, amount));
        }
    }

    for (from_id, to_id, amount) in migrations {
        if let Some(from_city) = state.cities.get_mut(&from_id) {
            from_city.population = (from_city.population - amount).max(1);
        }
        if let Some(to_city) = state.cities.get_mut(&to_id) {
            to_city.population += amount;
        }
        debug!(from = from_id, to = to_id, amount, "Population migrated");
    }
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{City, Company, ResourceType, SimState};

    fn make_consumer_state(population: i64, cash: f64) -> SimState {
        let mut s = SimState::new();

        s.resource_types.insert(
            1,
            ResourceType {
                id: 1,
                name: "Test Ingot".into(),
                category: "Refined Material".into(),
                is_vital: false,
            },
        );

        s.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Consumer City".into(),
                population,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        s.companies.insert(
            1,
            Company {
                id: 1,
                name: "Test Consumer".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        s.city_consumer_ids.insert(1, 1);
        s
    }

    #[test]
    fn consumer_draws_from_wage_pool() {
        let mut state = make_consumer_state(1_000_000, 0.0);

        // Set up a wage pool for the city
        let wage_amount = 1_000.0;
        state.add_to_wage_pool(1, wage_amount);

        run_consumption(&mut state, 1);

        // Should have drawn 80% of the wage pool
        let remaining_wage = state.get_wage_pool(1);
        assert!(
            (remaining_wage - (wage_amount * 0.2)).abs() < 0.001,
            "Wage pool should have 20% remaining"
        );
    }

    #[test]
    fn consumer_posts_buy_orders() {
        let mut state = make_consumer_state(1_000_000, 10_000.0);

        // Set up a wage pool for the city
        state.add_to_wage_pool(1, 10_000.0);

        run_consumption(&mut state, 1);
        let orders: Vec<_> = state.market_orders.values().collect();
        assert!(!orders.is_empty(), "Should have posted buy orders");
        assert_eq!(orders[0].order_type, "buy");
    }

    fn setup_population_dynamics_state(population: i64, food_amount: i64) -> SimState {
        let mut state = SimState::new();

        state.resource_types.insert(
            1,
            ResourceType {
                id: 1,
                name: "Food Ration".into(),
                category: "Consumer Good".into(),
                is_vital: true,
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "Test City".into(),
                population,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Consumer Co".into(),
                company_type: "consumer".into(),
                home_city_id: 1,
                cash: 0.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        state.city_consumer_ids.insert(1, 1);

        state.inventories.insert(
            crate::sim::state::Inventory::key(1, 1, 1),
            crate::sim::state::Inventory {
                company_id: 1,
                city_id: 1,
                resource_type_id: 1,
                quantity: food_amount,
            },
        );

        state
    }

    #[test]
    fn test_update_population_dynamics_growth() {
        // High food fulfillment (1.0 -> 100% -> growth)
        let pop = 1_000_000;
        let mut state = setup_population_dynamics_state(pop, pop);
        update_population_dynamics(&mut state);

        let city = state.cities.get(&1).unwrap();
        let expected_growth = pop as f64 * POPULATION_GROWTH_RATE;
        assert_eq!(city.population, pop + expected_growth as i64);
    }

    #[test]
    fn test_update_population_dynamics_decline() {
        // 50% fulfillment, between 40% (decline) and 70% (stable)
        let pop = 1_000_000;
        let food = 500_000;
        let mut state = setup_population_dynamics_state(pop, food);
        update_population_dynamics(&mut state);

        let t = (0.5 - FOOD_FULFILLMENT_DECLINE_MIN)
            / (FOOD_FULFILLMENT_STABLE_MIN - FOOD_FULFILLMENT_DECLINE_MIN);
        let expected_rate = POPULATION_STABLE_RATE * t + POPULATION_DECLINE_RATE * (1.0 - t);

        let city = state.cities.get(&1).unwrap();
        let expected_population = (pop as f64 * (1.0 + expected_rate)).max(1.0) as i64;
        assert_eq!(city.population, expected_population);
        assert!(city.population < pop); // Should decline
    }

    #[test]
    fn test_update_population_dynamics_starvation() {
        // 10% fulfillment -> starvation
        let pop = 1_000_000;
        let food = 100_000;
        let mut state = setup_population_dynamics_state(pop, food);
        update_population_dynamics(&mut state);

        let city = state.cities.get(&1).unwrap();
        let expected_population = pop + (pop as f64 * POPULATION_STARVATION_RATE) as i64;
        assert_eq!(city.population, expected_population);
    }

    #[test]
    fn test_run_migration_between_cities() {
        let mut state = SimState::new();
        state.tick = MIGRATION_INTERVAL;

        // Empire hierarchy
        state.sectors.insert(
            1,
            crate::sim::state::Sector {
                id: 1,
                empire_id: 1,
                name: "S1".into(),
            },
        );
        state.star_systems.insert(
            1,
            crate::sim::state::StarSystem {
                id: 1,
                sector_id: 1,
                name: "Sys1".into(),
            },
        );
        state.celestial_bodies.insert(
            1,
            crate::sim::state::CelestialBody {
                id: 1,
                system_id: 1,
                name: "B1".into(),
                fertility: 1.0,
            },
        );

        let pop1 = 100_000;
        let pop2 = 100_000;

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City 1".into(),
                population: pop1,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 1,
                name: "City 2".into(),
                population: pop2,
                infrastructure_lvl: 5,
                port_tier: 1,
                port_fee_per_unit: 0.1,
                port_max_throughput: 1000,
                tax_collected_this_tick: 0.0,
                population_growth_rate: 0.0,
            },
        );

        state.city_food_balance.insert(
            1,
            crate::sim::state::CityFoodBalance {
                city_id: 1,
                food_surplus: 0,
                fulfillment_ratio: 0.1,
                needs_relief: true,
                has_surplus: false,
            },
        );
        state.city_food_balance.insert(
            2,
            crate::sim::state::CityFoodBalance {
                city_id: 2,
                food_surplus: 0,
                fulfillment_ratio: 0.9,
                needs_relief: false,
                has_surplus: false,
            },
        );

        run_migration(&mut state);

        let c1 = state.cities.get(&1).unwrap();
        let c2 = state.cities.get(&2).unwrap();

        let expected_migration = (pop1 as f64 * MIGRATION_RATE).max(1.0) as i64;

        assert_eq!(c1.population, pop1 - expected_migration);
        assert_eq!(c2.population, pop2 + expected_migration);
    }
}
