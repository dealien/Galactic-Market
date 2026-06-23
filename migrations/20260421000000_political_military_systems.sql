-- Phase: Political & Military Systems
-- Adds military units, treaties/alliances, wars, and occupation mechanics.

-- ═══════════════════════════════════════════════════════════════════════════════
-- Military Units
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE military_units (
    id SERIAL PRIMARY KEY,
    empire_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    unit_type VARCHAR(50) NOT NULL, -- 'fleet' or 'garrison'
    strength DOUBLE PRECISION NOT NULL DEFAULT 100.0,
    system_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    status VARCHAR(50) NOT NULL DEFAULT 'stationed', -- 'stationed', 'deployed', 'in_combat'
    morale DOUBLE PRECISION NOT NULL DEFAULT 1.0
);

CREATE INDEX idx_military_units_empire ON military_units(empire_id);
CREATE INDEX idx_military_units_system ON military_units(system_id);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Treaties & Alliances
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE treaties (
    id SERIAL PRIMARY KEY,
    alliance_name VARCHAR(255) NOT NULL,
    formed_tick BIGINT NOT NULL,
    dissolved_tick BIGINT -- NULL if active
);

CREATE TABLE treaty_members (
    treaty_id INTEGER NOT NULL REFERENCES treaties(id) ON DELETE CASCADE,
    empire_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    joined_tick BIGINT NOT NULL,
    PRIMARY KEY (treaty_id, empire_id)
);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Wars
-- ═══════════════════════════════════════════════════════════════════════════════
CREATE TABLE wars (
    id SERIAL PRIMARY KEY,
    aggressor_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    defender_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    start_tick BIGINT NOT NULL,
    end_tick BIGINT, -- NULL if active
    status VARCHAR(50) NOT NULL DEFAULT 'active' -- 'active', 'ceasefire', 'concluded'
);

CREATE TABLE war_participants (
    war_id INTEGER NOT NULL REFERENCES wars(id) ON DELETE CASCADE,
    empire_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    role VARCHAR(50) NOT NULL, -- 'aggressor', 'defender', 'ally'
    PRIMARY KEY (war_id, empire_id)
);

CREATE TABLE war_theaters (
    war_id INTEGER NOT NULL REFERENCES wars(id) ON DELETE CASCADE,
    system_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    PRIMARY KEY (war_id, system_id)
);

-- ═══════════════════════════════════════════════════════════════════════════════
-- Occupation & Territory
-- ═══════════════════════════════════════════════════════════════════════════════
ALTER TABLE star_systems ADD COLUMN occupier_empire_id INTEGER REFERENCES empires(id) ON DELETE SET NULL;
ALTER TABLE star_systems ADD COLUMN occupied_since_tick BIGINT;
