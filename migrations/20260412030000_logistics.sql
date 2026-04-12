-- Stage 1 Logistics: Simple Trade Routes
CREATE TABLE trade_routes (
    id SERIAL PRIMARY KEY,
    company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    origin_city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    dest_city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    resource_type_id INTEGER NOT NULL REFERENCES resource_types(id),
    quantity BIGINT NOT NULL,
    arrival_tick BIGINT NOT NULL
);

-- Optional: System lanes for future pathfinding, even if unused now
CREATE TABLE system_lanes (
    system_a_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    system_b_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    distance_ly DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (system_a_id, system_b_id)
);
