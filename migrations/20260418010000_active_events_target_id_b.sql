-- Add target_id_b to active_events so that blockade_lane events (which target a
-- jump lane identified by two system IDs) can be fully round-tripped through the
-- database.  For non-lane events (famine, infrastructure_damage, etc.) this
-- column will be NULL.
ALTER TABLE active_events ADD COLUMN target_id_b INTEGER;
