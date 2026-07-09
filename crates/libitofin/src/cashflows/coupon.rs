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
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time};

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

    /// The whole accrual period as a fraction of a year.
    fn accrual_period(&self) -> Time {
        let base = self.coupon_base();
        self.day_counter().year_fraction_ref(
            base.accrual_start_date,
            base.accrual_end_date,
            base.ref_period_start,
            base.ref_period_end,
        )
    }

    /// The whole accrual period in days.
    fn accrual_days(&self) -> SerialNumber {
        let base = self.coupon_base();
        self.day_counter()
            .day_count(base.accrual_start_date, base.accrual_end_date)
    }

    /// The period accrued up to `date`, as a fraction of a year.
    ///
    /// Zero outside `(accrual_start_date, payment_date]`. Once the coupon
    /// trades ex-coupon the sign flips: the buyer does not receive the coupon,
    /// so what accrues is the *negative* fraction from `date` forward to the
    /// accrual end, and it returns to zero at the accrual end. This is the one
    /// place the two accrual measures disagree: [`accrued_days`](Self::accrued_days)
    /// has no ex-coupon branch and keeps counting forward from the accrual
    /// start.
    fn accrued_period(&self, date: Date) -> Time {
        let base = self.coupon_base();
        if date <= base.accrual_start_date || date > base.payment_date {
            0.0
        } else if base
            .ex_coupon_date
            .is_some_and(|ex_coupon| ex_coupon <= date)
        {
            -self.day_counter().year_fraction_ref(
                date,
                date.max(base.accrual_end_date),
                base.ref_period_start,
                base.ref_period_end,
            )
        } else {
            self.day_counter().year_fraction_ref(
                base.accrual_start_date,
                date.min(base.accrual_end_date),
                base.ref_period_start,
                base.ref_period_end,
            )
        }
    }

    /// The days accrued up to `date`.
    ///
    /// Zero outside `(accrual_start_date, payment_date]`, and otherwise the day
    /// count from the accrual start, capped at the accrual end. Unlike
    /// [`accrued_period`](Self::accrued_period) this never goes negative.
    fn accrued_days(&self, date: Date) -> SerialNumber {
        let base = self.coupon_base();
        if date <= base.accrual_start_date || date > base.payment_date {
            0
        } else {
            self.day_counter()
                .day_count(base.accrual_start_date, date.min(base.accrual_end_date))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflow::cash_flow_has_occurred;
    use crate::event::Event;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::settings::Settings;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};

    struct TestCoupon {
        base: CouponBase,
        rate: Rate,
        day_counter: DayCounter,
        observable: Observable,
    }

    impl TestCoupon {
        fn new(base: CouponBase, rate: Rate, day_counter: DayCounter) -> TestCoupon {
            TestCoupon {
                base,
                rate,
                day_counter,
                observable: Observable::new(),
            }
        }
    }

    impl AsObservable for TestCoupon {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl Event for TestCoupon {
        fn date(&self) -> Date {
            self.base.payment_date()
        }

        fn has_occurred(
            &self,
            settings: &Settings<Date>,
            ref_date: Option<Date>,
            include_ref_date: Option<bool>,
        ) -> QlResult<bool> {
            cash_flow_has_occurred(self.date(), settings, ref_date, include_ref_date)
        }
    }

    impl CashFlow for TestCoupon {
        fn amount(&self) -> QlResult<Real> {
            Ok(self.nominal() * self.rate * self.accrual_period())
        }

        fn ex_coupon_date(&self) -> Option<Date> {
            self.base.ex_coupon_date()
        }
    }

    impl Coupon for TestCoupon {
        fn coupon_base(&self) -> &CouponBase {
            &self.base
        }

        fn rate(&self) -> QlResult<Rate> {
            Ok(self.rate)
        }

        fn day_counter(&self) -> DayCounter {
            self.day_counter.clone()
        }

        fn accrued_amount(&self, date: Date) -> QlResult<Real> {
            Ok(self.nominal() * self.rate * self.accrued_period(date))
        }
    }

    fn start() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn end() -> Date {
        Date::new(15, Month::July, 2026)
    }

    fn payment() -> Date {
        Date::new(20, Month::July, 2026)
    }

    fn coupon(ex_coupon_date: Option<Date>) -> TestCoupon {
        TestCoupon::new(
            CouponBase::new(payment(), 100.0, start(), end(), None, None, ex_coupon_date),
            0.03,
            Actual360::new(),
        )
    }

    #[test]
    fn an_absent_reference_period_defaults_to_the_accrual_period() {
        let coupon = coupon(None);

        assert_eq!(coupon.reference_period_start(), start());
        assert_eq!(coupon.reference_period_end(), end());
    }

    #[test]
    fn an_explicit_reference_period_is_kept() {
        let ref_start = Date::new(15, Month::December, 2025);
        let ref_end = Date::new(15, Month::June, 2026);
        let coupon = TestCoupon::new(
            CouponBase::new(
                payment(),
                100.0,
                start(),
                end(),
                Some(ref_start),
                Some(ref_end),
                None,
            ),
            0.03,
            Actual360::new(),
        );

        assert_eq!(coupon.reference_period_start(), ref_start);
        assert_eq!(coupon.reference_period_end(), ref_end);
    }

    #[test]
    fn the_accrual_period_spans_the_accrual_dates() {
        let coupon = coupon(None);

        assert_eq!(coupon.accrual_days(), 181);
        assert!((coupon.accrual_period() - 181.0 / 360.0).abs() < 1e-15);
        assert_eq!(coupon.date(), payment());
        assert_eq!(coupon.nominal(), 100.0);
    }

    #[test]
    fn nothing_accrues_before_the_accrual_start() {
        let coupon = coupon(None);

        assert_eq!(coupon.accrued_period(start() - 1), 0.0);
        assert_eq!(coupon.accrued_days(start() - 1), 0);
        assert_eq!(coupon.accrued_period(start()), 0.0);
        assert_eq!(coupon.accrued_days(start()), 0);
        assert_eq!(coupon.accrued_amount(start()).unwrap(), 0.0);
    }

    #[test]
    fn nothing_accrues_after_the_payment_date() {
        let coupon = coupon(None);

        assert_eq!(coupon.accrued_period(payment() + 1), 0.0);
        assert_eq!(coupon.accrued_days(payment() + 1), 0);
    }

    #[test]
    fn accrual_grows_from_the_accrual_start_and_is_capped_at_the_accrual_end() {
        let coupon = coupon(None);
        let mid = Date::new(15, Month::April, 2026);

        assert_eq!(coupon.accrued_days(mid), 90);
        assert!((coupon.accrued_period(mid) - 90.0 / 360.0).abs() < 1e-15);
        assert!((coupon.accrued_amount(mid).unwrap() - 100.0 * 0.03 * 90.0 / 360.0).abs() < 1e-13);

        assert_eq!(coupon.accrued_days(end()), 181);
        assert_eq!(coupon.accrued_days(payment()), 181);
        assert!((coupon.accrued_period(payment()) - 181.0 / 360.0).abs() < 1e-15);
    }

    /// The reference period is passed through to the day counter, so a
    /// convention that reads it sees the coupon's own dates.
    #[test]
    fn the_reference_period_reaches_the_day_counter() {
        let coupon = TestCoupon::new(
            CouponBase::new(payment(), 100.0, start(), end(), None, None, None),
            0.03,
            Thirty360::with_convention(Convention::BondBasis),
        );

        assert_eq!(coupon.accrual_days(), 180);
        assert!((coupon.accrual_period() - 0.5).abs() < 1e-15);
    }
}
