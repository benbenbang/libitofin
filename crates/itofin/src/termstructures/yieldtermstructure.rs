//! Interest-rate term structure.
//!
//! Port of `ql/termstructures/yieldtermstructure.{hpp,cpp}`: the
//! [`YieldTermStructure`] trait extends [`TermStructure`] with the
//! discount-factor, zero-yield and forward-rate algebra, all derived from the
//! single required [`discount_impl`](YieldTermStructure::discount_impl).
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes (the turn-of-year effect: `jumps`/`jumpDates` constructor
//!   arguments, the jump inspectors and the jump-aware `discount`) are not
//!   ported; no flat curve uses them and they follow with the bootstrapped
//!   curves as a follow-up. Without jumps the C++ `update()` override is a
//!   no-op, so the base updater behaviour
//!   ([`TermStructureBase::updater`](super::TermStructureBase::updater)) is
//!   already complete.
//! - C++ overloads on `Date`/`Time`/`Period` become distinct method names
//!   (`discount_date`, `forward_rate_between`, ...), following the
//!   [`InterestRate`] port convention.

use crate::errors::QlResult;
use crate::interestrate::{Compounding, InterestRate};
use crate::require;
use crate::termstructures::TermStructure;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::types::{DiscountFactor, Time};

const DT: Time = 0.0001;

/// Interest-rate term structure.
///
/// Mirrors QuantLib's `YieldTermStructure`: concrete curves implement
/// [`discount_impl`](Self::discount_impl) (called after range checking, so it
/// must assume extrapolation is required) and inherit the rest.
pub trait YieldTermStructure: TermStructure {
    /// Discount factor calculation, implemented by concrete curves.
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor>;

    /// The discount factor from `date` to the reference date.
    fn discount_date(&self, date: Date, extrapolate: bool) -> QlResult<DiscountFactor> {
        self.discount(self.time_from_reference(date)?, extrapolate)
    }

    /// The discount factor from time `t` to the reference date.
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure.
    fn discount(&self, t: Time, extrapolate: bool) -> QlResult<DiscountFactor> {
        self.check_range_time(t, extrapolate)?;
        self.discount_impl(t)
    }

    /// The implied zero-yield rate for `date`, carrying the required
    /// day-counting rule.
    fn zero_rate_date(
        &self,
        date: Date,
        result_day_counter: DayCounter,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        let t = self.time_from_reference(date)?;
        if t == 0.0 {
            let compound = 1.0 / self.discount(DT, extrapolate)?;
            return InterestRate::implied_rate(compound, result_day_counter, comp, freq, DT);
        }
        let compound = 1.0 / self.discount(t, extrapolate)?;
        InterestRate::implied_rate_between(
            compound,
            result_day_counter,
            comp,
            freq,
            self.reference_date()?,
            date,
        )
    }

    /// The implied zero-yield rate for time `t`, carrying the same
    /// day-counting rule used by the term structure.
    ///
    /// The time must be calculated with that same rule.
    fn zero_rate(
        &self,
        t: Time,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        let Some(day_counter) = self.day_counter() else {
            crate::fail!("no day counter provided for this term structure");
        };
        let t = if t == 0.0 { DT } else { t };
        let compound = 1.0 / self.discount(t, extrapolate)?;
        InterestRate::implied_rate(compound, day_counter, comp, freq, t)
    }

    /// The forward interest rate between two dates, carrying the required
    /// day-counting rule; equal dates yield the instantaneous forward.
    fn forward_rate_between(
        &self,
        d1: Date,
        d2: Date,
        result_day_counter: DayCounter,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        if d1 == d2 {
            self.check_range_date(d1, extrapolate)?;
            let t1 = Time::max(self.time_from_reference(d1)? - DT / 2.0, 0.0);
            let t2 = t1 + DT;
            let compound = self.discount(t1, true)? / self.discount(t2, true)?;
            return InterestRate::implied_rate(compound, result_day_counter, comp, freq, DT);
        }
        require!(d1 < d2, "{d1} later than {d2}");
        let compound =
            self.discount_date(d1, extrapolate)? / self.discount_date(d2, extrapolate)?;
        InterestRate::implied_rate_between(compound, result_day_counter, comp, freq, d1, d2)
    }

    /// The forward interest rate over the period `p` starting at `date`;
    /// dates are not adjusted for holidays.
    fn forward_rate_period(
        &self,
        date: Date,
        p: Period,
        result_day_counter: DayCounter,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        self.forward_rate_between(date, date + p, result_day_counter, comp, freq, extrapolate)
    }

    /// The forward interest rate between two times, carrying the same
    /// day-counting rule used by the term structure; equal times yield the
    /// instantaneous forward.
    ///
    /// The times must be calculated with that same rule.
    fn forward_rate(
        &self,
        t1: Time,
        t2: Time,
        comp: Compounding,
        freq: Frequency,
        extrapolate: bool,
    ) -> QlResult<InterestRate> {
        let Some(day_counter) = self.day_counter() else {
            crate::fail!("no day counter provided for this term structure");
        };
        let (t1, t2, compound) = if t2 == t1 {
            self.check_range_time(t1, extrapolate)?;
            let t1 = Time::max(t1 - DT / 2.0, 0.0);
            let t2 = t1 + DT;
            let compound = self.discount(t1, true)? / self.discount(t2, true)?;
            (t1, t2, compound)
        } else {
            if t1.is_nan() || t2.is_nan() || t2 <= t1 {
                crate::fail!("t1 ({t1}) >= t2 ({t2})");
            }
            let compound = self.discount(t1, extrapolate)? / self.discount(t2, extrapolate)?;
            (t1, t2, compound)
        };
        InterestRate::implied_rate(compound, day_counter, comp, freq, t2 - t1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::TermStructureBase;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Real;

    struct ExponentialCurve {
        base: TermStructureBase,
        rate: Real,
    }

    impl ExponentialCurve {
        fn new(reference: Date, rate: Real) -> ExponentialCurve {
            ExponentialCurve {
                base: TermStructureBase::with_reference_date(
                    reference,
                    None,
                    Some(Actual360::new()),
                ),
                rate,
            }
        }
    }

    impl AsObservable for ExponentialCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for ExponentialCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl YieldTermStructure for ExponentialCurve {
        fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
            Ok((-self.rate * t).exp())
        }
    }

    fn curve() -> ExponentialCurve {
        ExponentialCurve::new(Date::new(15, Month::June, 2026), 0.05)
    }

    #[test]
    fn discount_checks_range_then_delegates() {
        let curve = curve();
        let df = curve.discount(2.0, false).unwrap();
        assert!((df - (-0.1_f64).exp()).abs() < 1.0e-15);
        assert!(curve.discount(-0.5, false).is_err());
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);
    }

    #[test]
    fn discount_date_converts_through_the_day_counter() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let df = curve.discount_date(reference + 180, false).unwrap();
        assert!((df - (-0.05_f64 * 0.5).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn zero_rate_recovers_the_continuous_rate() {
        let curve = curve();
        let zero = curve
            .zero_rate(1.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-12);

        let zero = curve
            .zero_rate(0.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-12);
    }

    #[test]
    fn zero_rate_date_round_trips_the_discount() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let date = reference + 360;
        let zero = curve
            .zero_rate_date(
                date,
                Actual360::new(),
                Compounding::Compounded,
                Frequency::Semiannual,
                false,
            )
            .unwrap();
        let df = curve.discount_date(date, false).unwrap();
        let round_trip = zero.discount_factor_between(reference, date).unwrap();
        assert!((round_trip - df).abs() < 1.0e-15);

        let at_reference = curve
            .zero_rate_date(
                reference,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
                false,
            )
            .unwrap();
        assert!((at_reference.rate() - 0.05).abs() < 1.0e-10);
    }

    #[test]
    fn forward_rate_matches_the_flat_rate() {
        let curve = curve();
        let forward = curve
            .forward_rate(0.5, 1.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - 0.05).abs() < 1.0e-12);

        let instantaneous = curve
            .forward_rate(1.0, 1.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((instantaneous.rate() - 0.05).abs() < 1.0e-9);

        assert!(
            curve
                .forward_rate(1.5, 0.5, Compounding::Continuous, Frequency::Annual, false)
                .is_err()
        );
    }

    #[test]
    fn forward_rate_between_dates_and_periods_agree() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let d1 = reference + 90;
        let d2 = reference + 270;
        let between = curve
            .forward_rate_between(
                d1,
                d2,
                Actual360::new(),
                Compounding::Simple,
                Frequency::Annual,
                false,
            )
            .unwrap();
        let by_period = curve
            .forward_rate_period(
                d1,
                Period::new(180, TimeUnit::Days),
                Actual360::new(),
                Compounding::Simple,
                Frequency::Annual,
                false,
            )
            .unwrap();
        assert_eq!(between.rate(), by_period.rate());

        let df1 = curve.discount_date(d1, false).unwrap();
        let df2 = curve.discount_date(d2, false).unwrap();
        let implied = between.compound_factor_between(d1, d2).unwrap();
        assert!((implied - df1 / df2).abs() < 1.0e-15);

        assert!(
            curve
                .forward_rate_between(
                    d2,
                    d1,
                    Actual360::new(),
                    Compounding::Simple,
                    Frequency::Annual,
                    false,
                )
                .is_err()
        );

        let instantaneous = curve
            .forward_rate_between(
                d1,
                d1,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
                false,
            )
            .unwrap();
        assert!((instantaneous.rate() - 0.05).abs() < 1.0e-9);
    }
}
