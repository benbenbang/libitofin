//! Concrete cash flows.
//!
//! Port of `ql/cashflows/`, built on the [`CashFlow`](crate::cashflow::CashFlow)
//! base. The items are re-exported flat, so a coupon is `cashflows::Coupon`
//! rather than `cashflows::coupon::Coupon`.

mod capflooredcoupon;
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
mod iborleg;
mod overnightindexedcoupon;
mod overnightindexedcouponpricer;
mod overnightleg;
mod rateaveraging;
mod simplecashflow;

pub use capflooredcoupon::{CappedFlooredCoupon, CappedFlooredIborCoupon};
pub use cashflows::CashFlows;
pub use coupon::{Coupon, CouponBase};
pub use couponpricer::{BlackIborCouponPricer, FloatingRateCouponPricer};
pub use dividend::{Dividend, FixedDividend, FractionalDividend, dividend_vector};
pub use duration::Duration;
pub use fixedratecoupon::FixedRateCoupon;
pub use fixedrateleg::FixedRateLeg;
pub use floatingratecoupon::{FloatingIndex, FloatingRateCoupon};
pub use iborcoupon::IborCoupon;
pub use iborleg::{AttachPricer, IborLeg, set_coupon_pricer};
pub use overnightindexedcoupon::OvernightIndexedCoupon;
pub use overnightindexedcouponpricer::{
    CompoundingOvernightIndexedCouponPricer, OvernightSchedule,
};
pub use overnightleg::OvernightLeg;
pub use rateaveraging::RateAveraging;
pub use simplecashflow::{AmortizingPayment, Redemption, SimpleCashFlow};
