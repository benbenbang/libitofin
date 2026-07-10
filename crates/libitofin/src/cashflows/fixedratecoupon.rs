//! Coupons paying a fixed rate.
//!
//! Port of `ql/cashflows/fixedratecoupon.{hpp,cpp}`: [`FixedRateCoupon`], a
//! [`Coupon`] accruing an [`InterestRate`] on its nominal.
//!
//! ## Divergences from QuantLib
//!
//! C++ caches `amount_` behind `LazyObject::performCalculations`. The amount is
//! a pure function of immutable state, so the port recomputes it, in keeping
//! with [`CashFlow`](crate::cashflow::CashFlow) leaving caching to the concrete
//! flows.
//!
//! The two C++ constructors overload on their third argument; Rust names them
//! [`FixedRateCoupon::new`] (an explicit [`InterestRate`]) and
//! [`FixedRateCoupon::from_rate`] (a rate plus a day counter, taken as simple
//! and annual). The `accept(AcyclicVisitor&)` override has no counterpart.

use crate::cashflow::{CashFlow, cash_flow_has_occurred};
use crate::cashflows::coupon::{Coupon, CouponBase};
use crate::errors::QlResult;
use crate::event::Event;
use crate::interestrate::{Compounding, InterestRate};
use crate::patterns::observable::{AsObservable, Observable};
use crate::settings::Settings;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real};

/// A [`Coupon`] paying a fixed [`InterestRate`] over its accrual period.
pub struct FixedRateCoupon {
    base: CouponBase,
    rate: InterestRate,
    observable: Observable,
}

impl FixedRateCoupon {
    /// A coupon accruing `interest_rate` on `nominal` over
    /// `[accrual_start_date, accrual_end_date]`, paid on `payment_date`.
    ///
    /// A `None` reference-period bound defaults to the matching accrual date.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        payment_date: Date,
        nominal: Real,
        interest_rate: InterestRate,
        accrual_start_date: Date,
        accrual_end_date: Date,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        ex_coupon_date: Option<Date>,
    ) -> FixedRateCoupon {
        FixedRateCoupon {
            base: CouponBase::new(
                payment_date,
                nominal,
                accrual_start_date,
                accrual_end_date,
                ref_period_start,
                ref_period_end,
                ex_coupon_date,
            ),
            rate: interest_rate,
            observable: Observable::new(),
        }
    }

    /// The same coupon, with the rate quoted as
    /// [`Simple`](Compounding::Simple) and [`Annual`](Frequency::Annual)
    /// against `day_counter`.
    #[allow(clippy::too_many_arguments)]
    pub fn from_rate(
        payment_date: Date,
        nominal: Real,
        rate: Rate,
        day_counter: DayCounter,
        accrual_start_date: Date,
        accrual_end_date: Date,
        ref_period_start: Option<Date>,
        ref_period_end: Option<Date>,
        ex_coupon_date: Option<Date>,
    ) -> FixedRateCoupon {
        let interest_rate =
            InterestRate::new(rate, day_counter, Compounding::Simple, Frequency::Annual)
                .expect("a simple annual rate has no frequency precondition");
        FixedRateCoupon::new(
            payment_date,
            nominal,
            interest_rate,
            accrual_start_date,
            accrual_end_date,
            ref_period_start,
            ref_period_end,
            ex_coupon_date,
        )
    }

    /// The rate the coupon accrues at, with its conventions.
    pub fn interest_rate(&self) -> &InterestRate {
        &self.rate
    }
}

impl AsObservable for FixedRateCoupon {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Event for FixedRateCoupon {
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

impl CashFlow for FixedRateCoupon {
    /// # Errors
    ///
    /// Propagates the [`InterestRate::compound_factor_between_ref`] domain
    /// checks.
    fn amount(&self) -> QlResult<Real> {
        let factor = self.rate.compound_factor_between_ref(
            self.accrual_start_date(),
            self.accrual_end_date(),
            self.reference_period_start(),
            self.reference_period_end(),
        )?;
        Ok(self.nominal() * (factor - 1.0))
    }

    fn ex_coupon_date(&self) -> Option<Date> {
        self.base.ex_coupon_date()
    }
}

impl Coupon for FixedRateCoupon {
    fn coupon_base(&self) -> &CouponBase {
        &self.base
    }

    fn rate(&self) -> QlResult<Rate> {
        Ok(self.rate.rate())
    }

    fn day_counter(&self) -> DayCounter {
        self.rate.day_counter().clone()
    }

    /// The amount accrued up to `date`, compounded rather than prorated: it is
    /// `nominal * (compoundFactor - 1)`, not `amount() * accruedPeriod() /
    /// accrualPeriod()`, and the two agree only for a simple rate.
    ///
    /// Zero outside `(accrual_start_date, payment_date]`, and negative once the
    /// coupon [trades ex-coupon](Coupon::trades_ex_coupon_on), mirroring the
    /// sign flip of [`Coupon::accrued_period`].
    ///
    /// # Errors
    ///
    /// Propagates the [`InterestRate::compound_factor_between_ref`] domain
    /// checks.
    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        if date <= self.accrual_start_date() || date > self.base.payment_date() {
            return Ok(0.0);
        }
        let (d1, d2, sign) = if self.trades_ex_coupon_on(date) {
            (date, date.max(self.accrual_end_date()), -1.0)
        } else {
            (
                self.accrual_start_date(),
                date.min(self.accrual_end_date()),
                1.0,
            )
        };
        let factor = self.rate.compound_factor_between_ref(
            d1,
            d2,
            self.reference_period_start(),
            self.reference_period_end(),
        )?;
        Ok(sign * self.nominal() * (factor - 1.0))
    }
}

#[cfg(test)]
mod tests {
    //! No test-suite case exercises `FixedRateCoupon::amount` or
    //! `accruedAmount` directly: `cashflows.cpp` reaches them only through
    //! `CashFlows::accruedAmount`, and the six cases it does cover are ported
    //! against the leg, in `fixedrateleg.rs`. Every constant asserted below is
    //! therefore derived from `fixedratecoupon.cpp` rather than read off a
    //! QuantLib test.

    use super::*;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};

    fn start() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn end() -> Date {
        Date::new(15, Month::July, 2026)
    }

    fn payment() -> Date {
        Date::new(20, Month::July, 2026)
    }

    fn coupon(ex_coupon_date: Option<Date>) -> FixedRateCoupon {
        FixedRateCoupon::from_rate(
            payment(),
            100.0,
            0.03,
            Actual360::new(),
            start(),
            end(),
            None,
            None,
            ex_coupon_date,
        )
    }

    #[test]
    fn a_simple_rate_accrues_the_prorated_amount() {
        let coupon = coupon(None);

        assert_eq!(coupon.rate().unwrap(), 0.03);
        assert_eq!(coupon.date(), payment());
        assert!((coupon.amount().unwrap() - 100.0 * 0.03 * 181.0 / 360.0).abs() < 1e-13);
    }

    /// `performCalculations` compounds the rate over the accrual period rather
    /// than prorating it, so a compounded coupon pays more than `nominal * rate
    /// * accrualPeriod()`. Both figures are derived from
    /// `fixedratecoupon.cpp`, not from a QuantLib test case.
    #[test]
    fn a_compounded_rate_accrues_more_than_the_prorated_amount() {
        let one_year = Date::new(15, Month::January, 2027);
        let rate = InterestRate::new(
            0.06,
            Thirty360::with_convention(Convention::BondBasis),
            Compounding::Compounded,
            Frequency::Semiannual,
        )
        .unwrap();
        let coupon =
            FixedRateCoupon::new(one_year, 100.0, rate, start(), one_year, None, None, None);

        assert!((coupon.accrual_period() - 1.0).abs() < 1e-15);
        assert!((coupon.amount().unwrap() - 6.09).abs() < 1e-12);
        assert!((coupon.amount().unwrap() - 100.0 * 0.06 * 1.0).abs() > 0.08);
    }

    /// The accrued amount compounds too: half a year of a semiannual 6% rate is
    /// exactly one compounding period, so it accrues 3.0, not half of 6.09.
    #[test]
    fn the_accrued_amount_compounds_rather_than_prorating_the_amount() {
        let one_year = Date::new(15, Month::January, 2027);
        let rate = InterestRate::new(
            0.06,
            Thirty360::with_convention(Convention::BondBasis),
            Compounding::Compounded,
            Frequency::Semiannual,
        )
        .unwrap();
        let coupon =
            FixedRateCoupon::new(one_year, 100.0, rate, start(), one_year, None, None, None);

        assert!((coupon.accrued_amount(end()).unwrap() - 3.0).abs() < 1e-12);
    }

    #[test]
    fn nothing_accrues_outside_the_accrual_range() {
        let coupon = coupon(None);

        assert_eq!(coupon.accrued_amount(start() - 1).unwrap(), 0.0);
        assert_eq!(coupon.accrued_amount(start()).unwrap(), 0.0);
        assert_eq!(coupon.accrued_amount(payment() + 1).unwrap(), 0.0);
    }

    #[test]
    fn the_accrued_amount_grows_and_is_capped_at_the_accrual_end() {
        let coupon = coupon(None);
        let mid = Date::new(15, Month::April, 2026);

        assert!((coupon.accrued_amount(mid).unwrap() - 100.0 * 0.03 * 90.0 / 360.0).abs() < 1e-13);
        assert!(
            (coupon.accrued_amount(payment()).unwrap() - coupon.amount().unwrap()).abs() < 1e-13
        );
    }

    /// `accruedAmount` flips sign from the ex-coupon date on, and returns to
    /// zero at the accrual end.
    #[test]
    fn the_accrued_amount_goes_negative_ex_coupon() {
        let ex_coupon = Date::new(1, Month::July, 2026);
        let coupon = coupon(Some(ex_coupon));

        assert!(coupon.accrued_amount(ex_coupon - 1).unwrap() > 0.0);
        assert!(
            (coupon.accrued_amount(ex_coupon).unwrap() + 100.0 * 0.03 * 14.0 / 360.0).abs() < 1e-13
        );
        assert_eq!(coupon.accrued_amount(end()).unwrap(), 0.0);
        assert_eq!(coupon.accrued_amount(payment()).unwrap(), 0.0);
    }

    /// The ex-coupon date reaches the accrual through
    /// [`CashFlow::ex_coupon_date`], so it and
    /// [`CashFlow::trading_ex_coupon`] cannot disagree.
    #[test]
    fn the_ex_coupon_date_is_the_one_the_cash_flow_reports() {
        let ex_coupon = Date::new(1, Month::July, 2026);
        let coupon = coupon(Some(ex_coupon));
        let settings = Settings::new();

        assert_eq!(coupon.ex_coupon_date(), Some(ex_coupon));
        assert!(coupon.trades_ex_coupon_on(ex_coupon));
        assert!(!coupon.trades_ex_coupon_on(ex_coupon - 1));
        assert!(
            coupon
                .trading_ex_coupon(&settings, Some(ex_coupon))
                .unwrap()
        );
        assert!(coupon.accrued_period(ex_coupon) < 0.0);
    }
}
