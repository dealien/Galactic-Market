-- Add aggressor_exhaustion and defender_exhaustion columns to wars table
ALTER TABLE wars ADD COLUMN aggressor_exhaustion DOUBLE PRECISION NOT NULL DEFAULT 0.0;
ALTER TABLE wars ADD COLUMN defender_exhaustion DOUBLE PRECISION NOT NULL DEFAULT 0.0;
