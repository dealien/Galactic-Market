-- Add fertility multiplier to planets (celestial_bodies)
-- Fertility affects how efficiently plantations produce food
-- Range: 0.0 (barren) to 3.0 (super-fertile)
-- Default 1.0 represents "normal" fertility
ALTER TABLE celestial_bodies ADD COLUMN fertility DOUBLE PRECISION NOT NULL DEFAULT 1.0;

-- Add constraint to ensure fertility is within reasonable bounds
ALTER TABLE celestial_bodies ADD CONSTRAINT fertility_range CHECK (fertility >= 0.0 AND fertility <= 3.0);
