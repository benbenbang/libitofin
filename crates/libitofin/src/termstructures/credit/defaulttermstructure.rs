//! Default-probability term structure.
//!
//! Port of `ql/termstructures/defaulttermstructure.{hpp,cpp}`: the
//! [`DefaultProbabilityTermStructure`] trait extends [`TermStructure`] with
//! the survival-probability, default-probability, default-density and
//! hazard-rate algebra. Two hooks are required
//! ([`survival_probability_impl`](DefaultProbabilityTermStructure::survival_probability_impl)
//! and
//! [`default_density_impl`](DefaultProbabilityTermStructure::default_density_impl),
//! `defaulttermstructure.hpp:158-160`); the third
//! ([`hazard_rate_impl`](DefaultProbabilityTermStructure::hazard_rate_impl),
//! `defaulttermstructure.hpp:162`) carries C++'s default implementation and is
//! overridden by curves that quote the hazard rate directly.
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes (the turn-of-year effect) are not ported: the `jumps` /
//!   `jumpDates` constructor arguments (`defaulttermstructure.hpp:48-63`), the
//!   jump-factor fold in `survivalProbability`
//!   (`defaulttermstructure.cpp:86-97`), the `jumpDates()` / `jumpTimes()`
//!   inspectors (`defaulttermstructure.hpp:138-139`) and the `setJumps()`
//!   bookkeeping behind the `update()` override
//!   (`defaulttermstructure.hpp:243-247`, `defaulttermstructure.cpp:64-79`).
//!   This port takes no jumps parameter at all rather than accepting and
//!   ignoring one. The omission is behaviour-free: with an empty `jumps_` the
//!   C++ `survivalProbability` skips the loop and returns
//!   `survivalProbabilityImpl(t)` unscaled (`defaulttermstructure.cpp:98-100`),
//!   and `update()` reduces to `TermStructure::update()`, which
//!   [`TermStructureBase::updater`](crate::termstructures::TermStructureBase::updater)
//!   already provides. Jumps follow with the bootstrapped credit curves in
//!   EPIC Credit (#676), matching the same deferral taken for
//!   [`YieldTermStructure`](crate::termstructures::yieldtermstructure::YieldTermStructure).
//! - C++ overloads on `Date`/`Time` become distinct method names: the plain
//!   name takes times and the `_date` / `_dates` suffix takes dates, so the
//!   two-argument `defaultProbability` overloads are
//!   [`default_probability_between`](DefaultProbabilityTermStructure::default_probability_between)
//!   (times) and
//!   [`default_probability_between_dates`](DefaultProbabilityTermStructure::default_probability_between_dates)
//!   (dates).
//! - The C++ `QL_REQUIRE` on reversed arguments
//!   (`defaulttermstructure.cpp:107-109`, `120-122`) returns an `Err` here
//!   rather than throwing.

use crate::errors::QlResult;
use crate::termstructures::TermStructure;
use crate::time::date::Date;
use crate::types::{Probability, Rate, Real, Time};
use crate::{fail, require};

/// Default-probability term structure.
///
/// Mirrors QuantLib's `DefaultProbabilityTermStructure`: concrete credit
/// curves implement
/// [`survival_probability_impl`](Self::survival_probability_impl) and
/// [`default_density_impl`](Self::default_density_impl) (both called after
/// range checking, so they must assume extrapolation is required) and inherit
/// the rest.
pub trait DefaultProbabilityTermStructure: TermStructure {
    /// Survival-probability calculation, implemented by concrete curves.
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability>;

    /// Default-density calculation, implemented by concrete curves.
    fn default_density_impl(&self, t: Time) -> QlResult<Real>;

    /// Hazard-rate calculation, derived from the density and the survival
    /// probability; curves quoting the hazard rate override it with a more
    /// efficient implementation.
    ///
    /// A zero survival probability yields a zero hazard rate rather than a
    /// division by zero (`defaulttermstructure.hpp:220-223`).
    fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
        let survival = self.survival_probability(t, true)?;
        if survival == 0.0 {
            return Ok(0.0);
        }
        Ok(self.default_density(t, true)? / survival)
    }

    /// The survival probability from the reference date to time `t`.
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure.
    fn survival_probability(&self, t: Time, extrapolate: bool) -> QlResult<Probability> {
        self.check_range_time(t, extrapolate)?;
        self.survival_probability_impl(t)
    }

    /// The survival probability from the reference date to `date`.
    fn survival_probability_date(&self, date: Date, extrapolate: bool) -> QlResult<Probability> {
        self.survival_probability(self.time_from_reference(date)?, extrapolate)
    }

    /// The default probability from the reference date to time `t`.
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure.
    fn default_probability(&self, t: Time, extrapolate: bool) -> QlResult<Probability> {
        Ok(1.0 - self.survival_probability(t, extrapolate)?)
    }

    /// The default probability from the reference date to `date`.
    fn default_probability_date(&self, date: Date, extrapolate: bool) -> QlResult<Probability> {
        Ok(1.0 - self.survival_probability_date(date, extrapolate)?)
    }

    /// The probability of default between times `t1` and `t2`.
    ///
    /// Times before the reference date contribute nothing, so a negative `t1`
    /// is clamped to a zero default probability
    /// (`defaulttermstructure.cpp:123`).
    fn default_probability_between(
        &self,
        t1: Time,
        t2: Time,
        extrapolate: bool,
    ) -> QlResult<Probability> {
        if t1.is_nan() || t2.is_nan() || t1 > t2 {
            fail!("initial time ({t1}) later than final time ({t2})");
        }
        let p1 = if t1 < 0.0 {
            0.0
        } else {
            self.default_probability(t1, extrapolate)?
        };
        let p2 = self.default_probability(t2, extrapolate)?;
        Ok(p2 - p1)
    }

    /// The probability of default between `d1` and `d2`.
    ///
    /// Dates before the reference date contribute nothing, so a `d1` that
    /// precedes it is clamped to a zero default probability
    /// (`defaulttermstructure.cpp:110-111`).
    fn default_probability_between_dates(
        &self,
        d1: Date,
        d2: Date,
        extrapolate: bool,
    ) -> QlResult<Probability> {
        require!(d1 <= d2, "initial date ({d1}) later than final date ({d2})");
        let p1 = if d1 < self.reference_date()? {
            0.0
        } else {
            self.default_probability_date(d1, extrapolate)?
        };
        let p2 = self.default_probability_date(d2, extrapolate)?;
        Ok(p2 - p1)
    }

    /// The default density at time `t`.
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure.
    fn default_density(&self, t: Time, extrapolate: bool) -> QlResult<Real> {
        self.check_range_time(t, extrapolate)?;
        self.default_density_impl(t)
    }

    /// The default density at `date`.
    fn default_density_date(&self, date: Date, extrapolate: bool) -> QlResult<Real> {
        self.default_density(self.time_from_reference(date)?, extrapolate)
    }

    /// The hazard rate at time `t`, with annual frequency and continuous
    /// compounding.
    ///
    /// The time must be calculated with the same day-counting rule used by
    /// the term structure.
    fn hazard_rate(&self, t: Time, extrapolate: bool) -> QlResult<Rate> {
        self.check_range_time(t, extrapolate)?;
        self.hazard_rate_impl(t)
    }

    /// The hazard rate at `date`, with annual frequency and continuous
    /// compounding.
    fn hazard_rate_date(&self, date: Date, extrapolate: bool) -> QlResult<Rate> {
        self.hazard_rate(self.time_from_reference(date)?, extrapolate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::TermStructureBase;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    const INTENSITY: Real = 0.04;

    struct ExponentialCurve {
        base: TermStructureBase,
        max: Date,
    }

    impl ExponentialCurve {
        fn new(reference: Date) -> ExponentialCurve {
            ExponentialCurve {
                base: TermStructureBase::with_reference_date(
                    reference,
                    None,
                    Some(Actual360::new()),
                ),
                max: reference + 360,
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
            self.max
        }
    }

    impl DefaultProbabilityTermStructure for ExponentialCurve {
        fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
            Ok((-INTENSITY * t).exp())
        }

        fn default_density_impl(&self, t: Time) -> QlResult<Real> {
            Ok(INTENSITY * (-INTENSITY * t).exp())
        }
    }

    fn curve() -> ExponentialCurve {
        ExponentialCurve::new(Date::new(15, Month::June, 2026))
    }

    fn survival(t: Time) -> Probability {
        (-INTENSITY * t).exp()
    }

    #[test]
    fn survival_probability_checks_range_then_delegates() {
        let curve = curve();
        assert!((curve.survival_probability(0.5, false).unwrap() - survival(0.5)).abs() < 1.0e-15);
        assert_eq!(curve.survival_probability(0.0, false).unwrap(), 1.0);
        assert!(curve.survival_probability(-0.5, false).is_err());
        assert!(curve.survival_probability(2.0, false).is_err());
        assert!((curve.survival_probability(2.0, true).unwrap() - survival(2.0)).abs() < 1.0e-15);
    }

    #[test]
    fn date_variants_convert_through_the_day_counter() {
        let curve = curve();
        let date = curve.reference_date().unwrap() + 180;
        assert!(
            (curve.survival_probability_date(date, false).unwrap() - survival(0.5)).abs() < 1.0e-15
        );
        assert!(
            (curve.default_density_date(date, false).unwrap() - INTENSITY * survival(0.5)).abs()
                < 1.0e-15
        );
        assert!((curve.hazard_rate_date(date, false).unwrap() - INTENSITY).abs() < 1.0e-15);
        assert!(curve.default_density(-0.5, false).is_err());
        assert!(curve.default_density(2.0, false).is_err());
    }

    #[test]
    fn default_probability_is_one_minus_survival() {
        let curve = curve();
        let date = curve.reference_date().unwrap() + 180;
        assert!(
            (curve.default_probability(0.5, false).unwrap() - (1.0 - survival(0.5))).abs()
                < 1.0e-15
        );
        assert!(
            (curve.default_probability_date(date, false).unwrap() - (1.0 - survival(0.5))).abs()
                < 1.0e-15
        );
        assert_eq!(curve.default_probability(0.0, false).unwrap(), 0.0);
        assert!(curve.default_probability(-0.5, false).is_err());
    }

    #[test]
    fn hazard_rate_recovers_the_constant_intensity() {
        let curve = curve();
        assert!((curve.hazard_rate(0.5, false).unwrap() - INTENSITY).abs() < 1.0e-15);
        assert!((curve.hazard_rate(2.0, true).unwrap() - INTENSITY).abs() < 1.0e-15);
        assert!(curve.hazard_rate(2.0, false).is_err());
        assert!(curve.hazard_rate(-0.5, false).is_err());
    }

    #[test]
    fn default_probability_between_times_clamps_and_orders() {
        let curve = curve();
        let between = curve
            .default_probability_between(0.25, 0.75, false)
            .unwrap();
        assert!((between - (survival(0.25) - survival(0.75))).abs() < 1.0e-15);

        let clamped = curve.default_probability_between(-1.0, 0.5, false).unwrap();
        assert!((clamped - (1.0 - survival(0.5))).abs() < 1.0e-15);

        assert!(
            curve
                .default_probability_between(0.75, 0.25, false)
                .is_err()
        );
    }

    #[test]
    fn default_probability_between_dates_clamps_before_the_reference() {
        let curve = curve();
        let reference = curve.reference_date().unwrap();
        let between = curve
            .default_probability_between_dates(reference + 90, reference + 270, false)
            .unwrap();
        assert!((between - (survival(0.25) - survival(0.75))).abs() < 1.0e-15);

        let clamped = curve
            .default_probability_between_dates(reference - 1, reference + 180, false)
            .unwrap();
        assert!((clamped - (1.0 - survival(0.5))).abs() < 1.0e-15);

        assert!(
            curve
                .default_probability_between_dates(reference + 270, reference + 90, false)
                .is_err()
        );
    }

    #[test]
    fn zero_survival_probability_yields_a_zero_hazard_rate() {
        struct DefaultedCurve {
            base: TermStructureBase,
        }

        impl AsObservable for DefaultedCurve {
            fn observable(&self) -> &Observable {
                self.base.observable()
            }
        }

        impl TermStructure for DefaultedCurve {
            fn base(&self) -> &TermStructureBase {
                &self.base
            }

            fn max_date(&self) -> Date {
                Date::max_date()
            }
        }

        impl DefaultProbabilityTermStructure for DefaultedCurve {
            fn survival_probability_impl(&self, _t: Time) -> QlResult<Probability> {
                Ok(0.0)
            }

            fn default_density_impl(&self, _t: Time) -> QlResult<Real> {
                Ok(1.0)
            }
        }

        let curve = DefaultedCurve {
            base: TermStructureBase::with_reference_date(
                Date::new(15, Month::June, 2026),
                None,
                Some(Actual360::new()),
            ),
        };
        assert_eq!(curve.hazard_rate(0.5, false).unwrap(), 0.0);
        assert_eq!(curve.default_probability(0.5, false).unwrap(), 1.0);
    }
}
