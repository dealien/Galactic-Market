-- Stage 3: Advanced Logistics and Geography

-- Connectivity between star systems
CREATE TABLE system_lanes (
    system_a_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    system_b_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    distance_ly DOUBLE PRECISION NOT NULL,
    lane_type VARCHAR(50) NOT NULL DEFAULT 'standard', -- standard, high_speed, unstable
    PRIMARY KEY (system_a_id, system_b_id)
);

-- Add throughput and cost modifiers to cities
ALTER TABLE cities ADD COLUMN port_fee_per_unit DOUBLE PRECISION NOT NULL DEFAULT 0.1;
ALTER TABLE cities ADD COLUMN port_max_throughput BIGINT NOT NULL DEFAULT 10000;
