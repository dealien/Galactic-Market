use crate::sim::state::{Inventory, SimState};
use tracing::debug;

/// Phase 3: Logistics.
///
/// Advance in-transit shipments and deliver cargo at destination.
/// In Stage 1, we support "instant" delivery by checking arrival_tick.
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

// ─── Unit Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{SimState, TradeRoute};

    #[test]
    fn logistics_delivers_arrived_shipments() {
        let mut state = SimState::new();
        state.trade_routes.insert(
            1,
            TradeRoute {
                id: 1,
                company_id: 1,
                origin_city_id: 1,
                dest_city_id: 2,
                resource_type_id: 1,
                quantity: 100,
                arrival_tick: 1,
            },
        );

        run_logistics(&mut state, 1);

        let key = Inventory::key(1, 2, 1);
        assert_eq!(state.inventories[&key].quantity, 100);
        assert!(state.trade_routes.is_empty());
    }

    #[test]
    fn logistics_skips_future_shipments() {
        let mut state = SimState::new();
        state.trade_routes.insert(
            1,
            TradeRoute {
                id: 1,
                company_id: 1,
                origin_city_id: 1,
                dest_city_id: 2,
                resource_type_id: 1,
                quantity: 100,
                arrival_tick: 10,
            },
        );

        run_logistics(&mut state, 1);

        assert!(state.inventories.is_empty());
        assert_eq!(state.trade_routes.len(), 1);
    }
}
