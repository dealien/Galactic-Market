-- Add side-aware war participant role vocabulary.
-- Keep legacy 'ally' role accepted for backward compatibility with persisted data.

ALTER TABLE war_participants
DROP CONSTRAINT IF EXISTS war_participants_role_check;

ALTER TABLE war_participants
ADD CONSTRAINT war_participants_role_check
CHECK (role IN (
    'aggressor',
    'defender',
    'aggressor_ally',
    'defender_ally',
    'ally'
));
