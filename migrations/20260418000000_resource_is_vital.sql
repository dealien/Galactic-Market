-- Add is_vital flag to resource_types.
-- Vital resources (food, water) trigger population crises when supply is insufficient.
ALTER TABLE resource_types ADD COLUMN is_vital BOOLEAN NOT NULL DEFAULT false;
