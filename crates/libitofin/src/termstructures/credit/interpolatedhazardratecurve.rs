//! Credit term structure interpolating hazard rates.
//!
//! Port of `ql/termstructures/credit/interpolatedhazardratecurve.hpp`:
//! [`InterpolatedHazardRateCurve`] builds a default-probability curve from
//! (date, hazard-rate) nodes on the
//! [`HazardRateStructure`] adapter and the
//! [`InterpolatedCurve`] holder, quoting the rate from the interpolation and
//! the survival probability from its primitive. The reference date is the
//! first node date (fixed).
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes (`jumps`/`jumpDates`) are not ported, per the
//!   [`defaulttermstructure`](crate::termstructures::credit::defaulttermstructure)
//!   divergence (#676); the constructors collapse to
//!   [`new`](InterpolatedHazardRateCurve::new) and
//!   [`with_calendar`](InterpolatedHazardRateCurve::with_calendar).
//! - The protected node-less constructors used by bootstrapped curves
//!   (`interpolatedhazardratecurve.hpp:74-92`) follow with the piecewise
//!   default curve (#676); this is the plain interpolated curve, as
//!   [`InterpolatedForwardCurve`](crate::termstructures::yields::InterpolatedForwardCurve)
//!   is on the yield side.
//! - The Gauss-Chebyshev survival-probability fallback of
//!   [`HazardRateStructure`] stays unported (#676) and is unreachable from
//!   here: [`survival_probability_impl`](DefaultProbabilityTermStructure::survival_probability_impl)
//!   is answered by the interpolation's own primitive
//!   (`interpolatedhazardratecurve.hpp:157-172`), which is the closed form the
//!   quadrature approximates.
//! - [`max_date`](TermStructure::max_date) is the last node date outright
//!   (`interpolatedhazardratecurve.hpp:107-109`), with none of the stored
//!   maximum-date slot that the yield-side sibling consults
//!   (`forwardcurve.hpp:111-115`).

use crate::errors::QlResult;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::credit::hazardratestructure::HazardRateStructure;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Probability, Rate, Real, Time};

/// Credit curve interpolating (date, hazard rate) nodes; beyond the last node
/// the hazard rate extrapolates flat.
pub struct InterpolatedHazardRateCurve<I: Interpolator> {
    base: TermStructureBase,
    dates: Vec<Date>,
    curve: InterpolatedCurve<I>,
}

impl<I: Interpolator> InterpolatedHazardRateCurve<I> {
    /// Curve over `(date, hazard rate)` nodes; the first date is the reference
    /// date, and the day counter converts the rest into node times.
    pub fn new(
        dates: Vec<Date>,
        hazard_rates: Vec<Rate>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<InterpolatedHazardRateCurve<I>> {
        Self::with_calendar(dates, hazard_rates, day_counter, None, interpolator)
    }

    /// Curve over `(date, hazard rate)` nodes carrying a calendar.
    pub fn with_calendar(
        dates: Vec<Date>,
        hazard_rates: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
    ) -> QlResult<InterpolatedHazardRateCurve<I>> {
        require!(
            dates.len() >= interpolator.required_points().max(1),
            "not enough input dates given"
        );
        require!(
            hazard_rates.len() == dates.len(),
            "dates/data count mismatch"
        );
        require!(
            hazard_rates.iter().all(|rate| *rate >= 0.0),
            "negative hazard rate"
        );
        let reference_date = dates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, hazard_rates, interpolator);
        curve.setup_interpolation()?;
        Ok(InterpolatedHazardRateCurve {
            base: TermStructureBase::with_reference_date(
                reference_date,
                calendar,
                Some(day_counter),
            ),
            dates,
            curve,
        })
    }

    /// The node times.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node values.
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node hazard rates (same as [`data`](Self::data)).
    pub fn hazard_rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The `(date, hazard rate)` nodes.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }

    fn last_time(&self) -> Time {
        *self
            .curve
            .times()
            .last()
            .expect("the constructor requires at least one node")
    }

    fn last_hazard_rate(&self) -> Rate {
        *self
            .curve
            .data()
            .last()
            .expect("the constructor requires at least one node")
    }
}

impl<I: Interpolator> AsObservable for InterpolatedHazardRateCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedHazardRateCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("the constructor requires at least one node")
    }
}

impl<I: Interpolator> HazardRateStructure for InterpolatedHazardRateCurve<I> {
    fn hazard_rate_curve_impl(&self, t: Time) -> QlResult<Rate> {
        if t <= self.last_time() {
            return self.curve.interpolation()?.value(t);
        }
        Ok(self.last_hazard_rate())
    }
}

impl<I: Interpolator> DefaultProbabilityTermStructure for InterpolatedHazardRateCurve<I> {
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        if t == 0.0 {
            return Ok(1.0);
        }
        let interpolation = self.curve.interpolation()?;
        let max_time = self.last_time();
        let integral = if t <= max_time {
            interpolation.primitive(t)?
        } else {
            interpolation.primitive(max_time)? + self.last_hazard_rate() * (t - max_time)
        };
        Ok((-integral).exp())
    }

    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        self.default_density_from_hazard_rate(t)
    }

    fn hazard_rate_impl(&self, t: Time) -> QlResult<Rate> {
        self.hazard_rate_curve_impl(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::interpolations::flat::BackwardFlat;
    use crate::math::interpolations::linear::Linear;
    use crate::termstructures::credit::flathazardrate::FlatHazardRate;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::timeunit::TimeUnit;

    const TOLERANCE: Real = 1.0e-15;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// Nodes at 0, 1, 2 and 5 years under Actual/360, so the times are exactly
    /// the integers the hand integration below assumes.
    fn hand_built_curve() -> InterpolatedHazardRateCurve<BackwardFlat> {
        InterpolatedHazardRateCurve::new(
            vec![
                reference(),
                reference() + 360,
                reference() + 720,
                reference() + 1800,
            ],
            vec![0.01, 0.015, 0.02, 0.03],
            Actual360::new(),
            BackwardFlat,
        )
        .unwrap()
    }

    /// The curve fixture of `testCachedMarketValue`
    /// (`creditdefaultswap.cpp:224-264`): the hazard rates are built from a
    /// table of default probabilities as `log(S1/S2) / (t2 - t1)`, so a
    /// backward-flat integration of the rates must telescope back onto the
    /// table exactly. That round trip is the oracle for
    /// `survivalProbabilityImpl`'s primitive.
    #[test]
    fn survival_probabilities_reproduce_the_cds_fixture_default_probabilities() {
        let eval_date = Date::new(9, Month::June, 2006);
        let calendar = UnitedStates::new(Market::GovernmentBond);
        let day_counter = Thirty360::with_convention(Convention::BondBasis);
        let advance = |n: i32, unit: TimeUnit| {
            calendar.advance(
                eval_date,
                n,
                unit,
                BusinessDayConvention::ModifiedFollowing,
                false,
            )
        };
        let dates = vec![
            eval_date,
            advance(6, TimeUnit::Months),
            advance(1, TimeUnit::Years),
            advance(2, TimeUnit::Years),
            advance(3, TimeUnit::Years),
            advance(4, TimeUnit::Years),
            advance(5, TimeUnit::Years),
            advance(7, TimeUnit::Years),
            advance(10, TimeUnit::Years),
        ];
        let default_probabilities: [Probability; 9] = [
            0.0000, 0.0047, 0.0093, 0.0286, 0.0619, 0.0953, 0.1508, 0.2288, 0.3666,
        ];

        let mut hazard_rates = vec![0.0];
        for i in 1..dates.len() {
            let t1 = day_counter.year_fraction(dates[0], dates[i - 1]);
            let t2 = day_counter.year_fraction(dates[0], dates[i]);
            let s1 = 1.0 - default_probabilities[i - 1];
            let s2 = 1.0 - default_probabilities[i];
            hazard_rates.push((s1 / s2).ln() / (t2 - t1));
        }

        let curve = InterpolatedHazardRateCurve::new(
            dates.clone(),
            hazard_rates,
            day_counter,
            BackwardFlat,
        )
        .unwrap();

        for (date, expected) in dates.iter().zip(default_probabilities) {
            let computed = curve.default_probability_date(*date, false).unwrap();
            assert!(
                (computed - expected).abs() <= TOLERANCE,
                "failed to reproduce the default probability at {date}: \
                 calculated {computed}, expected {expected}"
            );
        }
        assert_eq!(curve.max_date(), *dates.last().unwrap());
    }

    /// A one-node backward-flat curve integrates its single rate over every
    /// horizon, which is the closed form `FlatHazardRate` already reproduces
    /// against `defaultprobabilitycurves.cpp:118-149`.
    #[test]
    fn a_single_node_backward_flat_curve_agrees_with_the_flat_curve() {
        let rate = 0.0100;
        let curve = InterpolatedHazardRateCurve::new(
            vec![reference()],
            vec![rate],
            Actual360::new(),
            BackwardFlat,
        )
        .unwrap();
        curve.enable_extrapolation();
        let flat = FlatHazardRate::with_rate(reference(), rate, Actual360::new());

        assert_eq!(curve.max_date(), reference());
        for t in [0.0_f64, 0.5, 1.0, 5.0, 20.0] {
            let expected = flat.survival_probability(t, false).unwrap();
            assert!((curve.survival_probability(t, false).unwrap() - expected).abs() <= TOLERANCE);
            assert!((curve.hazard_rate(t, false).unwrap() - rate).abs() <= TOLERANCE);
            assert!(
                (curve.default_density(t, false).unwrap()
                    - flat.default_density(t, false).unwrap())
                .abs()
                    <= TOLERANCE
            );
        }
    }

    /// Backward-flat reads the right-hand node on every segment, so the hazard
    /// rate steps at the nodes and stays flat past the last one
    /// (`interpolatedhazardratecurve.hpp:148-155`).
    #[test]
    fn hazard_rates_step_between_nodes_and_extrapolate_flat() {
        let curve = hand_built_curve();
        let cases = [
            (0.0, 0.01),
            (0.5, 0.015),
            (1.0, 0.015),
            (1.5, 0.02),
            (2.0, 0.02),
            (3.5, 0.03),
            (5.0, 0.03),
        ];
        for (t, expected) in cases {
            assert!((curve.hazard_rate(t, false).unwrap() - expected).abs() <= TOLERANCE);
        }

        assert!(curve.hazard_rate(7.0, false).is_err());
        assert!((curve.hazard_rate(7.0, true).unwrap() - 0.03).abs() <= TOLERANCE);
    }

    /// The survival probability is `exp(-integral)`, and on a piecewise
    /// constant hazard rate the integral is a sum of `rate * dt` over the steps
    /// - here `0.015` on `(0, 1]`, `0.02` on `(1, 2]` and `0.03` on `(2, 5]`.
    #[test]
    fn survival_probabilities_match_the_hand_integrated_step_function() {
        let curve = hand_built_curve();
        let cases = [
            (0.0, 0.0),
            (0.5, 0.5 * 0.015),
            (1.0, 0.015),
            (2.0, 0.015 + 0.02),
            (3.5, 0.015 + 0.02 + 1.5 * 0.03),
            (5.0, 0.015 + 0.02 + 3.0 * 0.03),
        ];
        for (t, integral) in cases {
            let expected = (-integral as Real).exp();
            let computed = curve.survival_probability(t, false).unwrap();
            assert!(
                (computed - expected).abs() <= TOLERANCE,
                "failed to reproduce the survival probability at t = {t}: \
                 calculated {computed}, expected {expected}"
            );
            assert!(
                (curve.default_probability(t, false).unwrap() - (1.0 - expected)).abs()
                    <= TOLERANCE
            );
        }
        assert_eq!(curve.survival_probability(0.0, false).unwrap(), 1.0);
    }

    /// Past the last node the survival probability carries on under the flat
    /// tail rate (`interpolatedhazardratecurve.hpp:166-170`).
    #[test]
    fn the_survival_probability_tail_runs_on_the_last_node_rate() {
        let curve = hand_built_curve();
        let at_last_node = curve.survival_probability(5.0, false).unwrap();
        for t in [5.5_f64, 7.0, 20.0] {
            let expected = at_last_node * (-0.03 * (t - 5.0)).exp();
            assert!((curve.survival_probability(t, true).unwrap() - expected).abs() <= TOLERANCE);
        }
        assert!(curve.survival_probability(7.0, false).is_err());
    }

    /// The density is wired to the adapter's `h(t) S(t)`
    /// (`hazardratestructure.hpp:106-108`).
    #[test]
    fn the_default_density_is_the_hazard_rate_times_the_survival_probability() {
        let curve = hand_built_curve();
        for t in [0.0_f64, 0.5, 1.0, 3.5, 5.0] {
            let expected = curve.hazard_rate(t, false).unwrap()
                * curve.survival_probability(t, false).unwrap();
            assert!((curve.default_density(t, false).unwrap() - expected).abs() <= TOLERANCE);
        }
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = hand_built_curve();
        assert_eq!(curve.times(), &[0.0, 1.0, 2.0, 5.0]);
        assert_eq!(curve.hazard_rates(), &[0.01, 0.015, 0.02, 0.03]);
        assert_eq!(curve.data(), curve.hazard_rates());
        assert_eq!(curve.dates().len(), 4);
        assert_eq!(curve.nodes()[3], (reference() + 1800, 0.03));
        assert_eq!(curve.max_date(), reference() + 1800);
        assert_eq!(curve.reference_date().unwrap(), reference());
    }

    /// `initialize` (`interpolatedhazardratecurve.hpp:250-263`).
    #[test]
    fn the_constructor_rejects_invalid_nodes() {
        let Err(err) = InterpolatedHazardRateCurve::new(
            vec![reference()],
            vec![0.01],
            Actual360::new(),
            Linear,
        ) else {
            panic!("expected a required-points error")
        };
        assert!(err.message().contains("not enough input dates"));

        let Err(err) = InterpolatedHazardRateCurve::new(
            vec![reference(), reference() + 360],
            vec![0.01],
            Actual360::new(),
            BackwardFlat,
        ) else {
            panic!("expected a count-mismatch error")
        };
        assert!(err.message().contains("dates/data count mismatch"));

        let Err(err) = InterpolatedHazardRateCurve::new(
            vec![reference(), reference() + 360],
            vec![0.01, -0.001],
            Actual360::new(),
            BackwardFlat,
        ) else {
            panic!("expected a negative-rate error")
        };
        assert!(err.message().contains("negative hazard rate"));

        assert!(
            InterpolatedHazardRateCurve::new(
                vec![reference() + 360, reference()],
                vec![0.01, 0.02],
                Actual360::new(),
                BackwardFlat,
            )
            .is_err()
        );
    }
}
