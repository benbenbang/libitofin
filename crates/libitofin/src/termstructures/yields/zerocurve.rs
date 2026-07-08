//! Interpolated zero-rates structure.
//!
//! Port of `ql/termstructures/yield/zerocurve.hpp`:
//! [`InterpolatedZeroCurve`] builds a [`YieldTermStructure`] from
//! (date, zero-rate) nodes interpolated in zero-rate space, wiring the
//! [`ZeroYieldStructure`] adapter over an embedded
//! [`InterpolatedCurve`]; [`ZeroCurve`] is the C++ `typedef` for the
//! linearly interpolated case.
//!
//! ## Divergences from QuantLib
//!
//! - Jump quotes are not ported, per the established
//!   [`yieldtermstructure`](super::super::yieldtermstructure) divergence.
//! - The protected detached/moving constructors (used only by bootstrapped
//!   subclasses) follow with the bootstrap work; the public date-vector
//!   constructors are ported.
//! - C++ reads `dates.at(0)` before checking sizes, throwing `out_of_range`
//!   on an empty vector; here every input problem is a `QlError` per D4.

use crate::errors::QlResult;
use crate::interestrate::{Compounding, InterestRate};
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::yields::ZeroYieldStructure;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::types::{DiscountFactor, Rate, Real, Time};

/// Yield term structure based on interpolation of zero rates.
///
/// The first date is the reference date; rates are stored as annual
/// continuous compounding, converting on construction when quoted otherwise.
pub struct InterpolatedZeroCurve<I: Interpolator> {
    base: TermStructureBase,
    curve: InterpolatedCurve<I>,
    dates: Vec<Date>,
}

/// Term structure based on linear interpolation of zero yields (C++'s
/// `ZeroCurve` typedef).
pub type ZeroCurve = InterpolatedZeroCurve<Linear>;

impl<I: Interpolator> InterpolatedZeroCurve<I> {
    /// Curve through continuously compounded zero rates at the given dates;
    /// the first date is the reference date.
    pub fn new(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<InterpolatedZeroCurve<I>> {
        Self::with_compounding(
            dates,
            yields,
            day_counter,
            None,
            interpolator,
            Compounding::Continuous,
            Frequency::Annual,
        )
    }

    /// Curve through zero rates quoted with an explicit compounding
    /// convention, converted to annual continuous compounding on
    /// construction (C++'s `initialize`).
    pub fn with_compounding(
        dates: Vec<Date>,
        yields: Vec<Rate>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
        compounding: Compounding,
        frequency: Frequency,
    ) -> QlResult<InterpolatedZeroCurve<I>> {
        require!(
            dates.len() >= interpolator.required_points(),
            "not enough input dates given"
        );
        require!(yields.len() == dates.len(), "dates/data count mismatch");
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, dates[0], &day_counter)?;

        let mut data = yields;
        if compounding != Compounding::Continuous {
            let dt = 1.0 / 365.0;
            let r = InterestRate::new(data[0], day_counter.clone(), compounding, frequency)?;
            data[0] = r
                .equivalent_rate(Compounding::Continuous, Frequency::NoFrequency, dt)?
                .rate();
            for i in 1..dates.len() {
                let r = InterestRate::new(data[i], day_counter.clone(), compounding, frequency)?;
                data[i] = r
                    .equivalent_rate(Compounding::Continuous, Frequency::NoFrequency, times[i])?
                    .rate();
            }
        }

        let mut curve = InterpolatedCurve::new(times, data, interpolator);
        curve.setup_interpolation()?;
        let base = TermStructureBase::with_reference_date(dates[0], calendar, Some(day_counter));
        Ok(InterpolatedZeroCurve { base, curve, dates })
    }

    /// The node times.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node values (continuously compounded zero rates).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node values (continuously compounded zero rates).
    pub fn zero_rates(&self) -> &[Rate] {
        self.curve.data()
    }

    /// The curve nodes as (date, zero-rate) pairs.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.curve.data().iter().copied())
            .collect()
    }

    fn last_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("construction rejected empty dates")
    }
}

impl<I: Interpolator> AsObservable for InterpolatedZeroCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedZeroCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.curve.max_date().unwrap_or_else(|| self.last_date())
    }
}

impl<I: Interpolator> ZeroYieldStructure for InterpolatedZeroCurve<I> {
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
        let interpolation = self.curve.interpolation()?;
        let t_max = *self
            .curve
            .times()
            .last()
            .expect("construction rejected empty dates");
        if t <= t_max {
            return interpolation.value(t);
        }

        let z_max = *self
            .curve
            .data()
            .last()
            .expect("construction rejected empty dates");
        let inst_fwd_max = z_max + t_max * interpolation.derivative(t_max)?;
        Ok((z_max * t_max + inst_fwd_max * (t - t_max)) / t)
    }
}

impl<I: Interpolator> YieldTermStructure for InterpolatedZeroCurve<I> {
    fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
        self.discount_from_zero_yield(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn sample_dates() -> Vec<Date> {
        vec![
            reference(),
            reference() + 180,
            reference() + 360,
            reference() + 720,
        ]
    }

    fn sample_zeros() -> Vec<Rate> {
        vec![0.02, 0.03, 0.04, 0.045]
    }

    fn sample() -> ZeroCurve {
        ZeroCurve::new(sample_dates(), sample_zeros(), Actual360::new(), Linear).unwrap()
    }

    #[test]
    fn zero_rates_round_trip_through_the_discount() {
        let curve = sample();
        for (t, z) in [(0.5, 0.03), (1.0, 0.04), (2.0, 0.045)] {
            let zero = curve
                .zero_rate(t, Compounding::Continuous, Frequency::Annual, false)
                .unwrap();
            assert!((zero.rate() - z).abs() < 1.0e-14);
            let df = curve.discount(t, false).unwrap();
            assert!((df - (-z * t).exp()).abs() < 1.0e-15);
        }
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);

        let short_end = curve
            .zero_rate(0.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((short_end.rate() - 0.02).abs() < 1.0e-5);
    }

    #[test]
    fn zero_rates_interpolate_linearly_between_nodes() {
        let curve = sample();
        let zero = curve
            .zero_rate(0.75, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.035).abs() < 1.0e-14);
        let df = curve.discount(1.5, false).unwrap();
        assert!((df - (-0.0425_f64 * 1.5).exp()).abs() < 1.0e-15);
    }

    #[test]
    fn extrapolation_extends_the_last_instantaneous_forward_flat() {
        let curve = sample();
        assert!(curve.discount(3.0, false).is_err());

        curve.enable_extrapolation();
        let z_max = 0.045;
        let t_max = 2.0;
        let inst_fwd_max = z_max + t_max * 0.005;
        let expected = (z_max * t_max + inst_fwd_max * (3.0 - t_max)) / 3.0;
        let zero = curve
            .zero_rate(3.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - expected).abs() < 1.0e-14);

        let forward = curve
            .forward_rate(2.5, 2.5, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - inst_fwd_max).abs() < 1.0e-12);
    }

    #[test]
    fn compounded_quotes_are_converted_to_continuous_on_construction() {
        let curve = ZeroCurve::with_compounding(
            sample_dates(),
            vec![0.05; 4],
            Actual360::new(),
            None,
            Linear,
            Compounding::Compounded,
            Frequency::Annual,
        )
        .unwrap();

        let continuous = 1.05_f64.ln();
        for &z in curve.zero_rates() {
            assert!((z - continuous).abs() < 1.0e-13);
        }
        let df = curve.discount(2.0, false).unwrap();
        assert!((df - 1.05_f64.powf(-2.0)).abs() < 1.0e-13);
        let zero = curve
            .zero_rate(1.0, Compounding::Compounded, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-14);
    }

    #[test]
    fn inspectors_expose_the_nodes() {
        let curve = sample();
        assert_eq!(curve.times(), &[0.0, 0.5, 1.0, 2.0]);
        assert_eq!(curve.dates(), &sample_dates()[..]);
        assert_eq!(curve.data(), &sample_zeros()[..]);
        assert_eq!(curve.zero_rates(), curve.data());
        assert_eq!(
            curve.nodes(),
            sample_dates()
                .into_iter()
                .zip(sample_zeros())
                .collect::<Vec<_>>()
        );
        assert_eq!(curve.max_date(), reference() + 720);
        assert_eq!(curve.reference_date().unwrap(), reference());
    }

    fn build_err(dates: Vec<Date>, zeros: Vec<Rate>) -> String {
        match ZeroCurve::new(dates, zeros, Actual360::new(), Linear) {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err.message().to_string(),
        }
    }

    #[test]
    fn constructor_rejects_bad_inputs() {
        let err = build_err(vec![reference()], vec![0.02]);
        assert!(err.contains("not enough input dates"));

        let err = build_err(sample_dates(), vec![0.02]);
        assert!(err.contains("dates/data count mismatch"));

        let mut dates = sample_dates();
        dates.swap(1, 2);
        let err = build_err(dates, sample_zeros());
        assert!(err.contains("dates not sorted"));
    }
}
