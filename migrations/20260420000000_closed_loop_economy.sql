-- Phase 1: Closed-Loop Economy (Issues #9 & #10)
-- Adds wage pools, taxation, population dynamics, and related analytics

-- 1. Add wage and tax tracking to cities
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='cities' AND column_name='wage_pool') THEN
        ALTER TABLE cities ADD COLUMN wage_pool DECIMAL(18,2) NOT NULL DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='cities' AND column_name='tax_collected_this_tick') THEN
        ALTER TABLE cities ADD COLUMN tax_collected_this_tick DECIMAL(18,2) NOT NULL DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='cities' AND column_name='population_growth_rate') THEN
        ALTER TABLE cities ADD COLUMN population_growth_rate DECIMAL(10,6) NOT NULL DEFAULT 0.0;
    END IF;
END $$;

-- 2. Add treasury tracking to empires
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='empires' AND column_name='treasury_balance') THEN
        ALTER TABLE empires ADD COLUMN treasury_balance DECIMAL(18,2) NOT NULL DEFAULT 0;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='empires' AND column_name='tax_rate') THEN
        ALTER TABLE empires ADD COLUMN tax_rate DECIMAL(10,6) NOT NULL DEFAULT 0.05;
    END IF;
END $$;

-- 3. Add labor costs to recipes
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='recipes' AND column_name='labor_cost_per_run') THEN
        ALTER TABLE recipes ADD COLUMN labor_cost_per_run DECIMAL(10,6) NOT NULL DEFAULT 0.0;
    END IF;
END $$;

-- 4. Add labor cost baseline to resource types (for context)
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name='resource_types' AND column_name='production_labor_cost_per_unit') THEN
        ALTER TABLE resource_types ADD COLUMN production_labor_cost_per_unit DECIMAL(10,6) NOT NULL DEFAULT 0.0;
    END IF;
END $$;

-- 5. Create population history analytics table
CREATE TABLE IF NOT EXISTS population_history (
    id SERIAL PRIMARY KEY,
    city_id INTEGER NOT NULL REFERENCES cities(id) ON DELETE CASCADE,
    tick BIGINT NOT NULL,
    population BIGINT NOT NULL,
    wage_pool DECIMAL(18,2) NOT NULL,
    food_fulfillment_ratio DECIMAL(8,4) NOT NULL,
    population_growth_rate DECIMAL(10,6) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (city_id, tick)
);

-- Create index for efficient queries
CREATE INDEX IF NOT EXISTS idx_population_history_tick ON population_history(tick);
CREATE INDEX IF NOT EXISTS idx_population_history_city ON population_history(city_id);

-- 6. Create taxation history table for analytics
CREATE TABLE IF NOT EXISTS taxation_history (
    id SERIAL PRIMARY KEY,
    empire_id INTEGER NOT NULL REFERENCES empires(id) ON DELETE CASCADE,
    tick BIGINT NOT NULL,
    corporate_tax_collected DECIMAL(18,2) NOT NULL,
    port_fees_collected DECIMAL(18,2) NOT NULL,
    total_collected DECIMAL(18,2) NOT NULL,
    treasury_balance DECIMAL(18,2) NOT NULL,
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    UNIQUE (empire_id, tick)
);

-- Create index for efficient queries
CREATE INDEX IF NOT EXISTS idx_taxation_history_tick ON taxation_history(tick);
CREATE INDEX IF NOT EXISTS idx_taxation_history_empire ON taxation_history(empire_id);

-- 7. Add constraint to ensure wage_pool is non-negative
ALTER TABLE cities ADD CONSTRAINT wage_pool_non_negative CHECK (wage_pool >= 0);

-- 8. Add constraint to ensure treasury is non-negative
ALTER TABLE empires ADD CONSTRAINT treasury_non_negative CHECK (treasury_balance >= 0);

-- 9. Add constraint to ensure tax rates are reasonable (0-100%)
ALTER TABLE empires ADD CONSTRAINT tax_rate_range CHECK (tax_rate >= 0.0 AND tax_rate <= 1.0);

-- 10. Add constraint to ensure population growth rates are reasonable (-10% to +10% per tick)
ALTER TABLE cities ADD CONSTRAINT growth_rate_range CHECK (population_growth_rate >= -0.1 AND population_growth_rate <= 0.1);
