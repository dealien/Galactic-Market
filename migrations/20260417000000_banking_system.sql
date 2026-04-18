-- Banking System Implementation Plan (Issue #5)

-- 1. Create bank_accounts table to hold corporate and consumer deposits
CREATE TABLE bank_accounts (
    id SERIAL PRIMARY KEY,
    company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    bank_company_id INTEGER NOT NULL REFERENCES companies(id) ON DELETE CASCADE,
    balance DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    interest_rate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    -- Companies can have one account per bank
    UNIQUE (company_id, bank_company_id)
);

-- 2. Link loans to specific lending companies (Commercial/Central Banks)
-- NULL lender_company_id implies the loan is from "the void" (legacy/prime liquidity)
ALTER TABLE loans ADD COLUMN lender_company_id INTEGER REFERENCES companies(id);
