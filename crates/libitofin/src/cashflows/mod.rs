//! Concrete cash flows.
//!
//! Port of `ql/cashflows/`: the [`CashFlow`](crate::cashflow::CashFlow)
//! implementors, re-exported flat. The coupons follow; this module starts with
//! the flows that pay a predetermined amount.

mod dividend;
mod simplecashflow;

pub use dividend::{Dividend, FixedDividend, FractionalDividend, dividend_vector};
pub use simplecashflow::{AmortizingPayment, Redemption, SimpleCashFlow};
