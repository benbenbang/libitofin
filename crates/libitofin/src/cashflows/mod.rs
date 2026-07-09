//! Concrete cash flows.
//!
//! Port of `ql/cashflows/`: the [`CashFlow`](crate::cashflow::CashFlow)
//! implementors, re-exported flat. The coupons follow; this module starts with
//! the flows that pay a predetermined amount.

mod simplecashflow;

pub use simplecashflow::{AmortizingPayment, Redemption, SimpleCashFlow};
