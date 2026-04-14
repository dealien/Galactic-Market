-- Add last_trade_tick to companies to track market activity
ALTER TABLE companies ADD COLUMN last_trade_tick BIGINT NOT NULL DEFAULT 0;
