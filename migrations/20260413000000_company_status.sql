-- Add status field to companies to track active vs bankrupt actors
ALTER TABLE companies ADD COLUMN status VARCHAR(50) NOT NULL DEFAULT 'active';
