-- Stage 3: Advanced Logistics and Geography

-- Connectivity between star systems (Handle potential collision with partial Stage 1 migration)
CREATE TABLE IF NOT EXISTS system_lanes (
    system_a_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    system_b_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    distance_ly DOUBLE PRECISION NOT NULL,
    PRIMARY KEY (system_a_id, system_b_id)
);

-- Ensure lane_type exists
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='system_lanes' AND column_name='lane_type') THEN
        ALTER TABLE system_lanes ADD COLUMN lane_type VARCHAR(50) NOT NULL DEFAULT 'standard';
    END IF;
END $$;

-- Safely add throughput and cost modifiers to cities
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='cities' AND column_name='port_fee_per_unit') THEN
        ALTER TABLE cities ADD COLUMN port_fee_per_unit DOUBLE PRECISION NOT NULL DEFAULT 0.1;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='cities' AND column_name='port_max_throughput') THEN
        ALTER TABLE cities ADD COLUMN port_max_throughput BIGINT NOT NULL DEFAULT 10000;
    END IF;
END $$;
