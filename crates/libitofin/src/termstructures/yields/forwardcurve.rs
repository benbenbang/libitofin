//! Yield term structure interpolating instantaneous forward rates.
//!
//! Port of `ql/termstructures/yield/forwardcurve.hpp`:
//! [`InterpolatedForwardCurve`] builds a yield curve from (date,
//! instantaneous-forward-rate) nodes on the [`ForwardRateStructure`] and
//! [`InterpolatedCurve`] bases from #227, overriding the trapezoid default
//! with the interpolation's exact primitive; [`ForwardCurve`] is the C++
//! `typedef` on backward-flat interpolation. The reference date is the first
//! node date (fixed).
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes (`jumps`/`jumpDates`) are not ported, per the
//!   [`YieldTermStructure`] divergence (#203); the constructors collapse to
//!   [`new`](InterpolatedForwardCurve::new) and
//!   [`with_calendar`](InterpolatedForwardCurve::with_calendar).
//! - The protected detached/moving constructors used by bootstrapped curves
//!   follow with the bootstrap.
//! - Current C++ (1.42) deprecates `ForwardRateStructure` and derives this
//!   class from `ZeroYieldStructure` with only the primitive-based
//!   `zeroYieldImpl`; this port keeps the pre-1.42
//!   [`forward_impl`](ForwardRateStructure::forward_impl) (interpolate up to
//!   the last node, flat beyond) alongside the identical `zeroYieldImpl`, so
//!   the instantaneous forwards stay queryable through the trait. The numbers
//!   agree with both versions.

use crate::errors::QlResult;
use crate::math::interpolations::flat::BackwardFlat;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::yields::{ForwardRateStructure, ZeroYieldStructure};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{DiscountFactor, Rate, Real, Time};

/// Yield curve interpolating (date, instantaneous forward) nodes; beyond the
/// last node the forward extrapolates flat.
pub struct InterpolatedForwardCurve<I: Interpolator> {
    base: TermStructureBase,
    dates: Vec<Date>,
    curve: InterpolatedCurve<I>,
}

/// Term structure based on flat interpolation of forward rates (C++'s
/// `ForwardCurve` typedef).
pub type ForwardCurve = InterpolatedForwardCurve<BackwardFlat>;

impl<I: Interpolator> InterpolatedForwardCurve<I> {
    /// Curve over `(date, forward)` nodes; the first date is the reference
    /// date, and the day counter converts the rest into node times.
    pub fn new(
        dates: Vec<Date>,
        forwards: Vec<Rate>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<InterpolatedForwardCurve<I>> {
        Self::with_calendar(dates, forwards, day_counter, None, interpolator)
    }

    /// Curve over `(date, forward)` nodes carrying a calendar.
    pub fn with_calendar(
        dates: Vec<Date>,
        forwards: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
    ) -> QlResult<InterpolatedForwardCurve<I>> {
        require!(
            dates.len() >= interpolator.required_points().max(1),
            "not enough input dates given"
        );
        require!(forwards.len() == dates.len(), "dates/data count mismatch");
        let reference_date = dates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, forwards, interpolator);
        curve.setup_interpolation()?;
        Ok(InterpolatedForwardCurve {
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

    /// The node instantaneous forward rates (same as [`data`](Self::data)).
    pub fn forwards(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The `(date, forward)` nodes.
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

    fn last_forward(&self) -> Rate {
        *self
            .curve
            .data()
            .last()
            .expect("the constructor requires at least one node")
    }
}

impl<I: Interpolator> AsObservable for InterpolatedForwardCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedForwardCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.curve.max_date().unwrap_or_else(|| {
            *self
                .dates
                .last()
                .expect("the constructor requires at least one node")
        })
    }
}

impl<I: Interpolator> ForwardRateStructure for InterpolatedForwardCurve<I> {
    fn forward_impl(&self, t: Time) -> QlResult<Rate> {
        if t <= self.last_time() {
            return self.curve.interpolation()?.value(t);
        }
        Ok(self.last_forward())
    }
}

impl<I: Interpolator> ZeroYieldStructure for InterpolatedForwardCurve<I> {
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
        let interpolation = self.curve.interpolation()?;
        if t == 0.0 {
            return interpolation.value(t);
        }
        let max_time = self.last_time();
        let integral = if t <= max_time {
            interpolation.primitive(t)?
        } else {
            interpolation.primitive(max_time)? + self.last_forward() * (t - max_time)
        };
        Ok(integral / t)
    }
}

impl<I: Interpolator> YieldTermStructure for InterpolatedForwardCurve<I> {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        self.discount_from_zero_yield(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::linear::Linear;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    // Port of testCompositeZeroYieldStructures (termstructures.cpp): the
    // composite curve there subtracts the two ForwardCurves' zero yields, so
    // its expected zero rates equal the difference of the curves' zero rates.
    #[test]
    fn composite_oracle_zero_rate_differences_match_cpp() {
        let curve1 = ForwardCurve::new(
            vec![
                Date::new(10, Month::November, 2017),
                Date::new(13, Month::November, 2017),
                Date::new(12, Month::February, 2018),
                Date::new(10, Month::May, 2018),
                Date::new(10, Month::August, 2018),
                Date::new(12, Month::November, 2018),
                Date::new(21, Month::December, 2018),
                Date::new(15, Month::January, 2020),
                Date::new(31, Month::March, 2021),
                Date::new(28, Month::February, 2023),
                Date::new(21, Month::December, 2026),
                Date::new(31, Month::January, 2030),
                Date::new(28, Month::February, 2031),
                Date::new(31, Month::March, 2036),
                Date::new(28, Month::February, 2041),
                Date::new(28, Month::February, 2048),
                Date::new(31, Month::December, 2141),
            ],
            vec![
                0.0655823213132524,
                0.0655823213132524,
                0.0699455024156877,
                0.0799107139233497,
                0.0813931951022577,
                0.0841615820666691,
                0.0501297919004145,
                0.0823483583439658,
                0.0860720030924466,
                0.0922887604375688,
                0.10588902278996,
                0.117021968693922,
                0.109824660896137,
                0.109231572878364,
                0.119218123236241,
                0.128647300167664,
                0.0506086995288751,
            ],
            Actual365Fixed::new(),
            BackwardFlat,
        )
        .unwrap();

        let curve2 = ForwardCurve::new(
            vec![
                Date::new(10, Month::November, 2017),
                Date::new(13, Month::November, 2017),
                Date::new(11, Month::December, 2017),
                Date::new(12, Month::February, 2018),
                Date::new(10, Month::May, 2018),
                Date::new(31, Month::January, 2022),
                Date::new(7, Month::December, 2023),
                Date::new(31, Month::January, 2025),
                Date::new(31, Month::March, 2028),
                Date::new(7, Month::December, 2033),
                Date::new(1, Month::February, 2038),
                Date::new(2, Month::April, 2046),
                Date::new(2, Month::January, 2051),
                Date::new(31, Month::December, 2141),
            ],
            vec![
                0.056656806197189,
                0.056656806197189,
                0.0419541633454473,
                0.0286681050019797,
                0.0148840226959593,
                0.0246680238374363,
                0.0255349067810599,
                0.0298907184711927,
                0.0263943927922053,
                0.0291924526539802,
                0.0270049276163556,
                0.028775807327614,
                0.0293567711641792,
                0.010518655099659,
            ],
            Actual365Fixed::new(),
            BackwardFlat,
        )
        .unwrap();

        let cases = [
            (Date::new(10, Month::November, 2017), 0.00892551511527986),
            (Date::new(15, Month::December, 2017), 0.0278755322562788),
            (Date::new(15, Month::June, 2018), 0.0512001768603456),
            (Date::new(15, Month::September, 2029), 0.0729941474263546),
            (Date::new(15, Month::September, 2038), 0.0778333309498459),
            (Date::new(15, Month::March, 2046), 0.0828451659139004),
            (Date::new(15, Month::December, 2141), 0.0503573807521742),
        ];
        for (date, expected) in cases {
            let z1 = curve1
                .zero_rate_date(
                    date,
                    Actual365Fixed::new(),
                    Compounding::Continuous,
                    Frequency::Annual,
                    false,
                )
                .unwrap();
            let z2 = curve2
                .zero_rate_date(
                    date,
                    Actual365Fixed::new(),
                    Compounding::Continuous,
                    Frequency::Annual,
                    false,
                )
                .unwrap();
            assert!(
                (z1.rate() - z2.rate() - expected).abs() < 1.0e-10,
                "at {date}: {} vs {expected}",
                z1.rate() - z2.rate()
            );
        }
    }

    fn backward_flat_curve() -> ForwardCurve {
        ForwardCurve::new(
            vec![reference(), reference() + 360, reference() + 720],
            vec![0.03, 0.04, 0.06],
            Actual360::new(),
            BackwardFlat,
        )
        .unwrap()
    }

    #[test]
    fn backward_flat_forwards_average_into_zeros() {
        let curve = backward_flat_curve();
        // Segment forwards are the right nodes: 0.04 on (0,1], 0.06 on (1,2].
        assert!((curve.zero_yield_impl(1.0).unwrap() - 0.04).abs() < 1.0e-15);
        assert!((curve.zero_yield_impl(2.0).unwrap() - 0.05).abs() < 1.0e-15);
        assert!((curve.zero_yield_impl(0.5).unwrap() - 0.04).abs() < 1.0e-15);
        assert_eq!(curve.zero_yield_impl(0.0).unwrap(), 0.03);

        let df = curve.discount(2.0, false).unwrap();
        assert!((df - (-0.10_f64).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn beyond_the_last_node_the_forward_extrapolates_flat() {
        let curve = backward_flat_curve();
        assert_eq!(curve.forward_impl(3.0).unwrap(), 0.06);
        let zero = curve.zero_yield_impl(3.0).unwrap();
        assert!((zero - (0.04 + 0.06 + 0.06) / 3.0).abs() < 1.0e-15);

        assert!(curve.discount(3.0, false).is_err());
        curve.enable_extrapolation();
        let df = curve.discount(3.0, false).unwrap();
        assert!((df - (-zero * 3.0).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn linear_forwards_integrate_exactly() {
        let dates = vec![reference(), reference() + 360, reference() + 720];
        let forwards = vec![0.03, 0.04, 0.05];
        let curve =
            InterpolatedForwardCurve::new(dates, forwards, Actual360::new(), Linear).unwrap();
        for t in [0.25_f64, 1.0, 1.75, 2.0] {
            let expected = 0.03 + 0.005 * t;
            assert!((curve.zero_yield_impl(t).unwrap() - expected).abs() < 1.0e-15);
        }

        let forward = curve
            .forward_rate(1.5, 1.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - (0.03 + 0.01 * 1.5)).abs() < 1.0e-6);

        let zero = curve
            .zero_rate(2.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.04).abs() < 1.0e-14);
    }

    #[test]
    fn a_single_node_backward_flat_curve_is_a_flat_curve() {
        let curve = ForwardCurve::new(
            vec![reference()],
            vec![0.05],
            Actual360::new(),
            BackwardFlat,
        )
        .unwrap();
        curve.enable_extrapolation();
        assert_eq!(curve.max_date(), reference());
        for t in [0.5_f64, 2.0] {
            let df = curve.discount(t, false).unwrap();
            assert!((df - (-0.05 * t).exp()).abs() < 1.0e-15);
        }
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = backward_flat_curve();
        assert_eq!(
            curve.dates(),
            &[reference(), reference() + 360, reference() + 720]
        );
        assert_eq!(curve.times(), &[0.0, 1.0, 2.0]);
        assert_eq!(curve.forwards(), &[0.03, 0.04, 0.06]);
        assert_eq!(curve.data(), curve.forwards());
        assert_eq!(
            curve.nodes(),
            vec![
                (reference(), 0.03),
                (reference() + 360, 0.04),
                (reference() + 720, 0.06),
            ]
        );
        assert_eq!(curve.max_date(), reference() + 720);
        assert_eq!(curve.reference_date().unwrap(), reference());
    }

    #[test]
    fn constructor_rejects_invalid_nodes() {
        let Err(err) =
            InterpolatedForwardCurve::new(vec![reference()], vec![0.03], Actual360::new(), Linear)
        else {
            panic!("expected a required-points error")
        };
        assert!(err.message().contains("not enough input dates"));

        let Err(err) = ForwardCurve::new(
            vec![reference(), reference() + 360],
            vec![0.03],
            Actual360::new(),
            BackwardFlat,
        ) else {
            panic!("expected a count-mismatch error")
        };
        assert!(err.message().contains("dates/data count mismatch"));

        let Err(err) = ForwardCurve::new(
            vec![reference() + 360, reference()],
            vec![0.03, 0.04],
            Actual360::new(),
            BackwardFlat,
        ) else {
            panic!("expected a sorting error")
        };
        assert!(err.message().contains("dates not sorted"));

        assert!(ForwardCurve::new(vec![], vec![], Actual360::new(), BackwardFlat).is_err());
    }
}
