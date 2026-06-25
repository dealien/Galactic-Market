-- Enforce nullability invariants for star system occupation fields.
-- If a system is occupied, occupied_since_tick must be present.
-- If a system is not occupied, occupied_since_tick must be NULL.
UPDATE star_systems
SET occupied_since_tick = 0
WHERE occupier_empire_id IS NOT NULL
  AND occupied_since_tick IS NULL;

UPDATE star_systems
SET occupied_since_tick = NULL
WHERE occupier_empire_id IS NULL
  AND occupied_since_tick IS NOT NULL;

ALTER TABLE star_systems
ADD CONSTRAINT star_systems_occupation_fields_consistent_chk
CHECK ((occupier_empire_id IS NULL) = (occupied_since_tick IS NULL));
