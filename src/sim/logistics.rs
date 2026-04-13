use crate::sim::state::{Inventory, SimState};
use tracing::debug;

/// Phase 3: Logistics.
///
/// Advance in-transit shipments and deliver cargo at destination.
pub fn run_logistics(state: &mut SimState, current_tick: u64) {
    let mut to_deliver = Vec::new();

    // Identify shipments that have arrived
    for route in state.trade_routes.values() {
        if route.arrival_tick <= current_tick {
            to_deliver.push(route.id);
        }
    }

    for id in to_deliver {
        let route = state.trade_routes.remove(&id).unwrap();

        // Inventory is already removed from origin when the route is created (Phase 6).
        // Here we just add it to the destination.
        let key = Inventory::key(route.company_id, route.dest_city_id, route.resource_type_id);
        let entry = state.inventories.entry(key).or_insert(Inventory {
            company_id: route.company_id,
            city_id: route.dest_city_id,
            resource_type_id: route.resource_type_id,
            quantity: 0,
        });
        entry.quantity += route.quantity;

        debug!(
            company_id = route.company_id,
            resource_id = route.resource_type_id,
            qty = route.quantity,
            from = route.origin_city_id,
            to = route.dest_city_id,
            "Shipment delivered"
        );
    }
}

/// Metadata about a potential transport route.
#[derive(Debug, Clone, Copy)]
pub struct TransportInfo {
    /// Total ticks until arrival.
    pub ticks: u64,
    /// Cost in currency per unit of resource.
    pub cost_per_unit: f64,
}

/// Calculate time and cost for moving goods between two cities.
pub fn get_transport_info(
    state: &SimState,
    origin_city_id: i32,
    dest_city_id: i32,
) -> TransportInfo {
    if origin_city_id == dest_city_id {
        return TransportInfo {
            ticks: 0,
            cost_per_unit: 0.0,
        };
    }

    let origin_city = state
        .cities
        .get(&origin_city_id)
        .expect("Origin city not found");
    let dest_city = state
        .cities
        .get(&dest_city_id)
        .expect("Dest city not found");

    // 1. Same Celestial Body (Planet/Moon)
    if origin_city.body_id == dest_city.body_id {
        return TransportInfo {
            ticks: 1,
            cost_per_unit: 0.1,
        };
    }

    // 2. Same Star System
    let origin_body = state
        .celestial_bodies
        .get(&origin_city.body_id)
        .expect("Origin body not found");
    let dest_body = state
        .celestial_bodies
        .get(&dest_city.body_id)
        .expect("Dest body not found");

    if origin_body.system_id == dest_body.system_id {
        return TransportInfo {
            ticks: 3,
            cost_per_unit: 0.5,
        };
    }

    // 3. Same Sector
    let origin_system = state
        .star_systems
        .get(&origin_body.system_id)
        .expect("Origin system not found");
    let dest_system = state
        .star_systems
        .get(&dest_body.system_id)
        .expect("Dest system not found");

    if origin_system.sector_id == dest_system.sector_id {
        return TransportInfo {
            ticks: 7,
            cost_per_unit: 2.0,
        };
    }

    // 4. Inter-Sector / Inter-Empire
    TransportInfo {
        ticks: 15,
        cost_per_unit: 5.0,
    }
}

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{CelestialBody, City, Sector, SimState, StarSystem};

    fn setup_hierarchy(state: &mut SimState) {
        state.sectors.insert(
            1,
            Sector {
                id: 1,
                empire_id: 1,
                name: "Sector 1".into(),
            },
        );
        state.sectors.insert(
            2,
            Sector {
                id: 2,
                empire_id: 1,
                name: "Sector 2".into(),
            },
        );

        state.star_systems.insert(
            1,
            StarSystem {
                id: 1,
                sector_id: 1,
                name: "System 1".into(),
            },
        );
        state.star_systems.insert(
            2,
            StarSystem {
                id: 2,
                sector_id: 1,
                name: "System 2".into(),
            },
        );

        state.celestial_bodies.insert(
            1,
            CelestialBody {
                id: 1,
                system_id: 1,
                name: "Body 1".into(),
            },
        );
        state.celestial_bodies.insert(
            2,
            CelestialBody {
                id: 2,
                system_id: 1,
                name: "Body 2".into(),
            },
        );

        state.cities.insert(
            1,
            City {
                id: 1,
                body_id: 1,
                name: "City 1".into(),
                population: 0,
            },
        );
        state.cities.insert(
            2,
            City {
                id: 2,
                body_id: 1,
                name: "City 2".into(),
                population: 0,
            },
        );
        state.cities.insert(
            3,
            City {
                id: 3,
                body_id: 2,
                name: "City 3".into(),
                population: 0,
            },
        );
        state.cities.insert(
            4,
            City {
                id: 4,
                body_id: 2,
                name: "City 4".into(),
                population: 0,
            },
        );

        state.star_systems.insert(
            3,
            StarSystem {
                id: 3,
                sector_id: 2,
                name: "System 3".into(),
            },
        );
        state.celestial_bodies.insert(
            3,
            CelestialBody {
                id: 3,
                system_id: 3,
                name: "Body 3".into(),
            },
        );
        state.cities.insert(
            5,
            City {
                id: 5,
                body_id: 3,
                name: "City 5".into(),
                population: 0,
            },
        );
    }

    #[test]
    fn transport_info_calculates_correctly() {
        let mut state = SimState::new();
        setup_hierarchy(&mut state);

        // Same city
        let info = get_transport_info(&state, 1, 1);
        assert_eq!(info.ticks, 0);

        // Same planet
        let info = get_transport_info(&state, 1, 2);
        assert_eq!(info.ticks, 1);
        assert_eq!(info.cost_per_unit, 0.1);

        // Same system, different planet
        let info = get_transport_info(&state, 1, 3);
        assert_eq!(info.ticks, 3);
        assert_eq!(info.cost_per_unit, 0.5);

        // Same sector, different system
        state.celestial_bodies.insert(
            4,
            CelestialBody {
                id: 4,
                system_id: 2,
                name: "Body 4".into(),
            },
        );
        state.cities.insert(
            6,
            City {
                id: 6,
                body_id: 4,
                name: "City 6".into(),
                population: 0,
            },
        );
        let info = get_transport_info(&state, 1, 6);
        assert_eq!(info.ticks, 7);
        assert_eq!(info.cost_per_unit, 2.0);

        // Different sector
        let info = get_transport_info(&state, 1, 5);
        assert_eq!(info.ticks, 15);
        assert_eq!(info.cost_per_unit, 5.0);
    }
}
