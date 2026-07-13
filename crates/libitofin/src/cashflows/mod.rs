//! Concrete cash flows.
//!
//! Port of `ql/cashflows/`, built on the [`CashFlow`](crate::cashflow::CashFlow)
//! base. The items are re-exported flat, so a coupon is `cashflows::Coupon`
//! rather than `cashflows::coupon::Coupon`.

#[allow(clippy::module_inception)]
mod cashflows;
mod coupon;
mod couponpricer;
mod dividend;
mod duration;
mod fixedratecoupon;
mod fixedrateleg;
mod floatingratecoupon;
mod iborcoupon;
mod simplecashflow;

pub use cashflows::CashFlows;
pub use coupon::{Coupon, CouponBase};
pub use couponpricer::{BlackIborCouponPricer, FloatingRateCouponPricer};
pub use dividend::{Dividend, FixedDividend, FractionalDividend, dividend_vector};
pub use duration::Duration;
pub use fixedratecoupon::FixedRateCoupon;
pub use fixedrateleg::FixedRateLeg;
pub use floatingratecoupon::{FloatingIndex, FloatingRateCoupon};
pub use iborcoupon::IborCoupon;
pub use simplecashflow::{AmortizingPayment, Redemption, SimpleCashFlow};
