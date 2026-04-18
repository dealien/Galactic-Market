use crate::sim::state::SimState;
use tracing::debug;

/// Phase 5: Finance.
///
/// Process interest payments on outstanding loans. Interest is calculated
/// based on the current balance and added to the balance each tick.
/// Companies must pay down the interest from their cash if possible.
pub fn run_finance(state: &mut SimState) {
    // 1. Process Loan Interest (Borrower -> Lender)
    // Interest flows from borrower cash to lender cash (if lender is a company).
    let mut loan_payments = Vec::new();
    for loan in state.loans.values() {
        let tick_interest_rate = loan.interest_rate / 52.0;
        let interest_accrued = loan.balance * tick_interest_rate;
        loan_payments.push((
            loan.id,
            loan.company_id,
            loan.lender_company_id,
            interest_accrued,
        ));
    }

    for (loan_id, borrower_id, lender_id, interest) in loan_payments {
        let mut paid = 0.0;
        if let Some(borrower) = state.companies.get_mut(&borrower_id) {
            if borrower.cash >= interest {
                borrower.cash -= interest;
                paid = interest;
            } else {
                paid = borrower.cash;
                let shortfall = interest - borrower.cash;
                borrower.cash = 0.0;
                if let Some(loan) = state.loans.get_mut(&loan_id) {
                    loan.balance += shortfall;
                    debug!(
                        borrower_id,
                        shortfall, "Interest shortfall added to loan balance"
                    );
                }
            }
        }

        // Credit the lender if it's a company (Bank)
        if paid > 0.0
            && let Some(l_id) = lender_id
            && let Some(lender) = state.companies.get_mut(&l_id)
        {
            lender.cash += paid;
            debug!(l_id, paid, borrower_id, "Bank received interest payment");
        }
    }

    // 2. Process Deposit Interest (Bank -> Depositor)
    // Banks pay yield from their cash to the depositor's account balance.
    let mut deposit_yields = Vec::new();
    for account in state.bank_accounts.values() {
        let tick_yield = account.interest_rate / 52.0;
        let yield_earned = account.balance * tick_yield;
        deposit_yields.push((
            account.id,
            account.company_id,
            account.bank_company_id,
            yield_earned,
        ));
    }

    for (acc_id, _depositor_id, bank_id, yield_amt) in deposit_yields {
        let mut actual_yield = 0.0;
        if let Some(bank) = state.companies.get_mut(&bank_id) {
            if bank.cash >= yield_amt {
                bank.cash -= yield_amt;
                actual_yield = yield_amt;
            } else {
                actual_yield = bank.cash;
                bank.cash = 0.0;
            }
        }

        if actual_yield > 0.0
            && let Some(account) = state.bank_accounts.get_mut(&acc_id)
        {
            account.balance += actual_yield;
        }
    }

    // --- Bankruptcy Detection & Debt Repayment ---
    let bankrupt_ids: Vec<i32> = state
        .companies
        .iter()
        .filter(|(_, c)| c.status == "bankrupt")
        .map(|(id, _)| *id)
        .collect();

    for company_id in bankrupt_ids {
        // Apply all cash to debt
        let (cash, debt) = {
            let c = state.companies.get(&company_id).unwrap();
            (c.cash, c.debt)
        };

        if cash > 0.0 {
            let payment = cash.min(debt);
            if let Some(c) = state.companies.get_mut(&company_id) {
                c.cash -= payment;
                c.debt -= payment;
            }

            // Also reduce the linked Loan objects if they exist
            let mut remaining_payment = payment;
            let loan_ids: Vec<i32> = state
                .loans
                .values()
                .filter(|l| l.company_id == company_id)
                .map(|l| l.id)
                .collect();

            for l_id in loan_ids {
                if remaining_payment <= 0.0 {
                    break;
                }
                let (loan_payment, lender_id) = {
                    let loan = state.loans.get(&l_id).unwrap();
                    (remaining_payment.min(loan.balance), loan.lender_company_id)
                };

                if let Some(loan) = state.loans.get_mut(&l_id) {
                    loan.balance -= loan_payment;
                }
                remaining_payment -= loan_payment;

                // Credit the lender if it exists
                if let Some(l_id) = lender_id
                    && let Some(lender) = state.companies.get_mut(&l_id)
                {
                    lender.cash += loan_payment;
                }
            }

            debug!(
                company_id,
                payment, "Bankrupt company applied cash to debt reduction"
            );
        }

        // Final Liquidation Check
        let (debt, has_inventory) = {
            let c = state.companies.get(&company_id).unwrap();
            let inventory_count = state
                .inventories
                .values()
                .filter(|inv| inv.company_id == company_id && inv.quantity > 0)
                .count();
            (c.debt, inventory_count > 0)
        };

        if debt <= 0.01
            && !has_inventory
            && let Some(c) = state.companies.get_mut(&company_id)
        {
            c.status = "liquidated".into();
            debug!(
                company_id,
                "Company has been fully LIQUIDATED and is now defunct."
            );
        }
    }

    // Mark new bankruptcies
    for company in state.companies.values_mut() {
        if company.status == "active" && company.debt > 500000.0 {
            company.status = "bankrupt".into();
            debug!(
                company_id = company.id,
                "Company has gone BANKRUPT due to excessive debt!"
            );
        }
    }

    // ─── Reconciliation: Ensure company.debt matches total loan balance ───
    // This prevents double-counting bugs where debt is tracked in multiple places.
    // Uses company_to_loans reverse index for O(1) lookup instead of O(loans) filtering.
    let company_ids: Vec<i32> = state.companies.keys().cloned().collect();
    for company_id in company_ids {
        let loan_ids = state.get_company_loans(company_id);
        let total_debt: f64 = loan_ids
            .iter()
            .filter_map(|loan_id| state.loans.get(loan_id).map(|l| l.balance))
            .sum();

        if let Some(company) = state.companies.get_mut(&company_id)
            && (company.debt - total_debt).abs() > 0.01
        {
            debug!(
                company_id,
                old_debt = company.debt,
                actual_debt = total_debt,
                "Reconciled company debt from loans"
            );
            company.debt = total_debt;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::state::{Company, Loan, SimState};

    #[test]
    fn finance_charges_interest_and_deducts_cash() {
        let mut state = SimState::new();
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Test Co".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 100.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.add_loan(Loan {
            id: 1,
            company_id: 1,
            lender_company_id: None,
            principal: 1000.0,
            interest_rate: 0.52, // 52% annual = 1% per week/tick
            balance: 1000.0,
        });

        run_finance(&mut state);

        // Interest should be 1000 * 0.01 = 10.0
        assert_eq!(state.companies[&1].cash, 90.0);
        assert_eq!(state.loans[&1].balance, 1000.0);
    }

    #[test]
    fn finance_compounds_interest_if_cash_insufficient() {
        let mut state = SimState::new();
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Broke Co".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 0.0,
                debt: 0.0,
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );
        state.add_loan(Loan {
            id: 1,
            company_id: 1,
            lender_company_id: None,
            principal: 1000.0,
            interest_rate: 0.52,
            balance: 1000.0,
        });

        run_finance(&mut state);

        assert_eq!(state.companies[&1].cash, 0.0);
        assert_eq!(state.loans[&1].balance, 1010.0);
    }

    #[test]
    fn finance_triggers_bankruptcy_on_high_debt() {
        let mut state = SimState::new();
        state.companies.insert(
            1,
            Company {
                id: 1,
                name: "Debt King".into(),
                company_type: "freelancer".into(),
                home_city_id: 1,
                cash: 0.0,
                debt: 600000.0, // Over 500k limit
                next_eval_tick: 1,
                status: "active".into(),
                last_trade_tick: 0,
            },
        );

        run_finance(&mut state);

        assert_eq!(state.companies[&1].status, "bankrupt");
    }
}
