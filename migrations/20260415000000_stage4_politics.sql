-- Stage 4 Politics & Events
CREATE TABLE diplomatic_relations (
    id SERIAL PRIMARY KEY,
    empire_a_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    empire_b_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    tension DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    status VARCHAR(50) NOT NULL DEFAULT 'neutral', -- e.g., 'neutral', 'war', 'alliance'
    CONSTRAINT symmetric_unique_pair CHECK (empire_a_id < empire_b_id),
    UNIQUE (empire_a_id, empire_b_id)
);

-- Active events affecting the simulation
CREATE TABLE active_events (
    id SERIAL PRIMARY KEY,
    event_type VARCHAR(100) NOT NULL,
    target_id INTEGER, -- Generic ID (lane_id, city_id, etc.)
    severity DOUBLE PRECISION NOT NULL DEFAULT 1.0,
    start_tick BIGINT NOT NULL,
    end_tick BIGINT NOT NULL, -- Permanent events use a very high value
    flavor_text TEXT,
    details JSONB -- Additional event-specific data
);
