-- Add Copper and Tin facility fields
ALTER TABLE facilities
ADD COLUMN setup_ticks_remaining INTEGER NOT NULL DEFAULT 0,
ADD COLUMN target_resource_id INTEGER REFERENCES resource_types(id),
ADD COLUMN production_ratios JSONB;
