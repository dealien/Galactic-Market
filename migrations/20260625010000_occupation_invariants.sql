-- Enforce nullability invariants for star system occupation fields.
-- If a system is occupied, occupied_since_tick must be present.
-- If a system is not occupied, occupied_since_tick must be NULL.
-- Legacy rows with occupier set but no start tick are invalid occupation records;
-- clear occupier so both fields return to a consistent "not occupied" state.
UPDATE star_systems
SET occupier_empire_id = NULL
WHERE occupier_empire_id IS NOT NULL
  AND occupied_since_tick IS NULL;

-- Legacy rows can also contain stale occupied_since_tick values after occupation
-- ended; clear these orphaned timestamps.
UPDATE star_systems
SET occupied_since_tick = NULL
WHERE occupier_empire_id IS NULL
  AND occupied_since_tick IS NOT NULL;

ALTER TABLE star_systems
ADD CONSTRAINT star_systems_occupation_fields_consistent_chk
CHECK ((occupier_empire_id IS NULL) = (occupied_since_tick IS NULL));
