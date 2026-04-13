-- Stage 1: Basic Economy schema

-- Economic actors
CREATE TABLE companies (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    company_type VARCHAR(50) NOT NULL DEFAULT 'freelancer', -- freelancer, small_company, corporation, megacorp
    home_city_id INTEGER NOT NULL REFERENCES cities(id),
    cash DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    debt DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    credit_rating VARCHAR(10) NOT NULL DEFAULT 'B',
    next_eval_tick BIGINT NOT NULL DEFAULT 1
);

-- Outstanding loans per company
CREATE TABLE loans (
    id SERIAL PRIMARY KEY,
    company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    principal DOUBLE PRECISION NOT NULL,
    interest_rate DOUBLE PRECISION NOT NULL,
    balance DOUBLE PRECISION NOT NULL
);

-- Resource deposits on celestial bodies
CREATE TABLE deposits (
    id SERIAL PRIMARY KEY,
    body_id INTEGER NOT NULL REFERENCES celestial_bodies(id) ON DELETE CASCADE,
    resource_type_id INTEGER NOT NULL REFERENCES resource_types(id),
    size_total BIGINT NOT NULL,
    size_remaining BIGINT NOT NULL,
    extraction_cost_per_unit DOUBLE PRECISION NOT NULL,
    discovered BOOLEAN NOT NULL DEFAULT true
);

-- Company inventory at a city
CREATE TABLE inventory (
    id SERIAL PRIMARY KEY,
    company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    resource_type_id INTEGER NOT NULL REFERENCES resource_types(id),
    quantity BIGINT NOT NULL DEFAULT 0,
    UNIQUE (company_id, city_id, resource_type_id)
);

-- Production and extraction facilities
CREATE TABLE facilities (
    id SERIAL PRIMARY KEY,
    city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    facility_type VARCHAR(50) NOT NULL, -- mine, refinery
    capacity INTEGER NOT NULL DEFAULT 1 -- units processed per tick
);

-- Production recipes (e.g. "Iron Ingot")
CREATE TABLE recipes (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    output_resource_id INTEGER NOT NULL REFERENCES resource_types(id),
    output_qty INTEGER NOT NULL DEFAULT 1,
    facility_type VARCHAR(50) NOT NULL,
    time_ticks INTEGER NOT NULL DEFAULT 1
);

-- Recipe input requirements (many-to-many)
CREATE TABLE recipe_inputs (
    recipe_id INTEGER NOT NULL REFERENCES recipes(id) ON DELETE CASCADE,
    resource_type_id INTEGER NOT NULL REFERENCES resource_types(id),
    quantity INTEGER NOT NULL,
    PRIMARY KEY (recipe_id, resource_type_id)
);

-- Active market orders (cleared each tick in Stage 1)
CREATE TABLE market_orders (
    id SERIAL PRIMARY KEY,
    city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    resource_type_id INTEGER NOT NULL REFERENCES resource_types(id),
    order_type VARCHAR(4) NOT NULL CHECK (order_type IN ('buy', 'sell')),
    price DOUBLE PRECISION NOT NULL,
    quantity BIGINT NOT NULL,
    created_tick BIGINT NOT NULL
);

-- Append-only OHLCV price history per resource per city per tick
CREATE TABLE market_history (
    city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    resource_type_id INTEGER NOT NULL REFERENCES resource_types(id),
    tick BIGINT NOT NULL,
    open DOUBLE PRECISION NOT NULL,
    high DOUBLE PRECISION NOT NULL,
    low DOUBLE PRECISION NOT NULL,
    close DOUBLE PRECISION NOT NULL,
    volume BIGINT NOT NULL,
    PRIMARY KEY (city_id, resource_type_id, tick)
);
