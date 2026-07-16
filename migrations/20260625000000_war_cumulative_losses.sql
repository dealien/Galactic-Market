-- Adds a cumulative_losses column to wars so war exhaustion accumulates across
-- ticks rather than being recomputed from scratch each tick.
ALTER TABLE wars
    ADD COLUMN cumulative_losses DOUBLE PRECISION NOT NULL DEFAULT 0;
