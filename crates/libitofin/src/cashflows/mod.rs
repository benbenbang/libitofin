//! Concrete cash flows.
//!
//! Port of `ql/cashflows/`, built on the [`CashFlow`](crate::cashflow::CashFlow)
//! base. The items are re-exported flat, so a coupon is `cashflows::Coupon`
//! rather than `cashflows::coupon::Coupon`.

mod coupon;
mod dividend;
mod simplecashflow;

pub use coupon::{Coupon, CouponBase};
pub use dividend::{Dividend, FixedDividend, FractionalDividend, dividend_vector};
pub use simplecashflow::{AmortizingPayment, Redemption, SimpleCashFlow};
