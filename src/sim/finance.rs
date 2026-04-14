use crate::sim::state::SimState;
use tracing::debug;

/// Phase 5: Finance.
///
/// Process interest payments on outstanding loans. Interest is calculated
/// based on the current balance and added to the balance each tick.
/// Companies must pay down the interest from their cash if possible.
pub fn run_finance(state: &mut SimState) {
    let mut loan_updates = Vec::new();

    for loan in state.loans.values() {
        // Daily interest rate (simplification: interest_rate is annual, divide by 365)
        // Or for the sim, we can treat it as per-tick rate if small enough.
        // Let's assume the seeded 0.05 is annual, and 1 tick = 1 week (52 ticks/year).
        let tick_interest_rate = loan.interest_rate / 52.0;
        let interest_accrued = loan.balance * tick_interest_rate;

        loan_updates.push((loan.id, loan.company_id, interest_accrued));
    }

    for (loan_id, company_id, interest) in loan_updates {
        if let Some(company) = state.companies.get_mut(&company_id) {
            // Company tries to pay the interest from cash
            if company.cash >= interest {
                company.cash -= interest;
                debug!(company_id, interest, "Company paid interest from cash");
            } else {
                // Shortfall is added to the loan balance (compounding)
                let shortfall = interest - company.cash;
                company.cash = 0.0;

                if let Some(loan) = state.loans.get_mut(&loan_id) {
                    loan.balance += shortfall;
                    debug!(
                        company_id,
                        shortfall, "Interest shortfall added to loan balance"
                    );
                }
            }
        }
    }

    // --- Bankruptcy Detection & Debt Repayment ---
    for company in state.companies.values_mut() {
        if company.status == "bankrupt" {
            // Apply all cash to debt
            if company.cash > 0.0 {
                let payment = company.cash.min(company.debt);
                company.cash -= payment;
                company.debt -= payment;

                // Also reduce the linked Loan objects if they exist
                let company_id = company.id;
                for loan in state
                    .loans
                    .values_mut()
                    .filter(|l| l.company_id == company_id)
                {
                    let loan_payment = payment.min(loan.balance);
                    loan.balance -= loan_payment;
                }

                debug!(
                    company_id,
                    payment, "Bankrupt company applied cash to debt reduction"
                );
            }

            // Final Liquidation: If debt is gone and inventory is gone, mark as liquidated
            let inventory_count = state
                .inventories
                .values()
                .filter(|inv| inv.company_id == company.id && inv.quantity > 0)
                .count();

            if company.debt <= 0.01 && inventory_count == 0 {
                company.status = "liquidated".into();
                debug!(
                    company_id = company.id,
                    "Company has been fully LIQUIDATED and is now defunct."
                );
            }
        }

        if company.status == "active" && company.debt > 500000.0 {
            company.status = "bankrupt".into();
            debug!(
                company_id = company.id,
                "Company has gone BANKRUPT due to excessive debt!"
            );
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
            },
        );
        state.loans.insert(
            1,
            Loan {
                id: 1,
                company_id: 1,
                principal: 1000.0,
                interest_rate: 0.52, // 52% annual = 1% per week/tick
                balance: 1000.0,
            },
        );

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
            },
        );
        state.loans.insert(
            1,
            Loan {
                id: 1,
                company_id: 1,
                principal: 1000.0,
                interest_rate: 0.52,
                balance: 1000.0,
            },
        );

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
            },
        );

        run_finance(&mut state);

        assert_eq!(state.companies[&1].status, "bankrupt");
    }
}
