//! Coupons: cash flows that accrue over a period.
//!
//! Port of `ql/cashflows/coupon.{hpp,cpp}`. A [`Coupon`] is a
//! [`CashFlow`] whose amount accrues over `[accrual_start_date,
//! accrual_end_date]` against a nominal, measured by a [`DayCounter`] over an
//! optional reference period. It is the base of `FixedRateCoupon` and
//! `FloatingRateCoupon`.
//!
//! ## Shape
//!
//! C++ inherits the coupon's dates and nominal from an abstract `Coupon` base.
//! Rust splits that into the [`Coupon`] trait (the interface, plus the accrual
//! algebra as provided methods) and [`CouponBase`] (the state). An implementor
//! holds a `CouponBase` and hands it out through [`Coupon::coupon_base`]; the
//! provided methods read the dates from there. Both are public, so a concrete
//! coupon can be written outside this crate.
//!
//! [`Event::date`] and [`CashFlow::ex_coupon_date`] live on the supertraits and
//! must be implemented by the concrete type, forwarding to
//! [`CouponBase::payment_date`] and [`CouponBase::ex_coupon_date`]. Rust has no
//! specialization, so this trait cannot supply them.
//!
//! ## Divergences from QuantLib
//!
//! `Coupon` caches [`accrual_period`](Coupon::accrual_period) in a `mutable`
//! member seeded with `Null<Real>`. The cache has no behavioural effect and is
//! omitted here, in keeping with [`CashFlow`] leaving `LazyObject` caching to
//! the concrete flows.
//!
//! [`accrued_period`](Coupon::accrued_period) needs to know whether the coupon
//! trades ex-coupon at the date it is given. C++ calls
//! `tradingExCoupon(d)`, which resolves a null date against the evaluation date;
//! here the date is always explicit, so the check reduces to comparing it with
//! [`CashFlow::ex_coupon_date`] and no [`Settings`](crate::settings::Settings)
//! is threaded through.
//!
//! [`rate`](Coupon::rate) and [`accrued_amount`](Coupon::accrued_amount) return
//! [`QlResult`], matching [`CashFlow::amount`]: a floating-rate coupon reads an
//! index fixing that may be missing. The `accept(AcyclicVisitor&)` override and
//! the `coupon_cast` downcast have no counterpart in the port.

use crate::cashflow::CashFlow;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real};

use crate::errors::QlResult;

/// The dates and nominal every [`Coupon`] carries.
///
/// Mirrors the protected state of QuantLib's `Coupon`. A concrete coupon owns
/// one and exposes it through [`Coupon::coupon_base`].
#[derive(Clone, Debug)]
pub struct CouponBase {
    payment_date: Date,
    nominal: Real,
    accrual_start_date: Date,
    accrual_end_date: Date,
    ref_period_start: Date,
    ref_period_end: Date,
    ex_coupon_date: Option<Date>,
}

impl CouponBase {
    /// A coupon accruing `nominal` over `[accrual_start_date, accrual_end_date]`
    /// and paid on `payment_date`, which must already be a business day: the
    /// coupon does not adjust it.
    ///
    /// A `None` reference-period bound defaults to the matching accrual date,
    /// as a null `Date` does in C++.
    pub fn new(
        payment_date: Date,
        nominal: Real,
        accrual_start_date: Date,
        accrual_end_date: Date,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        ex_coupon_date: Option<Date>,
    ) -> CouponBase {
        CouponBase {
            payment_date,
            nominal,
            accrual_start_date,
            accrual_end_date,
            ref_period_start: ref_period_start.unwrap_or(accrual_start_date),
            ref_period_end: ref_period_end.unwrap_or(accrual_end_date),
            ex_coupon_date,
        }
    }

    /// The date the coupon is paid on.
    pub fn payment_date(&self) -> Date {
        self.payment_date
    }

    /// The date from which the coupon trades ex-coupon, when it has one.
    pub fn ex_coupon_date(&self) -> Option<Date> {
        self.ex_coupon_date
    }
}

/// A [`CashFlow`] accruing over a fixed period.
///
/// Mirrors QuantLib's `Coupon`: still abstract, but it gives implementors the
/// accrual-date algebra. Implementors supply the state through
/// [`coupon_base`](Self::coupon_base), the [`rate`](Self::rate) and
/// [`day_counter`](Self::day_counter) the accrual is measured with, and the
/// [`accrued_amount`](Self::accrued_amount) that rate implies.
pub trait Coupon: CashFlow {
    /// The coupon's dates and nominal.
    fn coupon_base(&self) -> &CouponBase;

    /// The rate the coupon accrues at.
    fn rate(&self) -> QlResult<Rate>;

    /// The day counter the accrual is measured with.
    fn day_counter(&self) -> DayCounter;

    /// The amount accrued up to `date`.
    fn accrued_amount(&self, date: Date) -> QlResult<Real>;

    /// The nominal the coupon accrues on.
    ///
    /// Virtual in C++, so that amortizing coupons can report the nominal
    /// outstanding at the accrual start rather than a fixed one.
    fn nominal(&self) -> Real {
        self.coupon_base().nominal
    }

    /// The start of the accrual period.
    fn accrual_start_date(&self) -> Date {
        self.coupon_base().accrual_start_date
    }

    /// The end of the accrual period.
    fn accrual_end_date(&self) -> Date {
        self.coupon_base().accrual_end_date
    }

    /// The start of the reference period.
    fn reference_period_start(&self) -> Date {
        self.coupon_base().ref_period_start
    }

    /// The end of the reference period.
    fn reference_period_end(&self) -> Date {
        self.coupon_base().ref_period_end
    }
}
