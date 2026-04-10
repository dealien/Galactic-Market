CREATE TABLE empires (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    government_type VARCHAR(50) NOT NULL,
    currency VARCHAR(50) NOT NULL,
    tax_rate_base DOUBLE PRECISION NOT NULL
);

CREATE TABLE sectors (
    id SERIAL PRIMARY KEY,
    empire_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    strategic_value DOUBLE PRECISION NOT NULL
);

CREATE TABLE star_systems (
    id SERIAL PRIMARY KEY,
    sector_id INTEGER NOT NULL REFERENCES sectors(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    star_type VARCHAR(50) NOT NULL,
    resource_modifier DOUBLE PRECISION NOT NULL
);

CREATE TABLE celestial_bodies (
    id SERIAL PRIMARY KEY,
    system_id INTEGER NOT NULL REFERENCES star_systems(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    body_type VARCHAR(50) NOT NULL,
    mass DOUBLE PRECISION NOT NULL,
    habitable BOOLEAN NOT NULL,
    population_cap BIGINT NOT NULL
);

CREATE TABLE cities (
    id SERIAL PRIMARY KEY,
    body_id INTEGER NOT NULL REFERENCES celestial_bodies(id) ON DELETE CASCADE,
    name VARCHAR(255) NOT NULL,
    population BIGINT NOT NULL,
    infrastructure_lvl INTEGER NOT NULL,
    port_tier INTEGER NOT NULL
);

CREATE TABLE resource_types (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    category VARCHAR(50) NOT NULL,
    base_mass_kg DOUBLE PRECISION NOT NULL,
    stackable BOOLEAN NOT NULL
);
