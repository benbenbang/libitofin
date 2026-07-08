//! Black volatility curve modelled as variance curve.
//!
//! Port of `ql/termstructures/volatility/equityfx/blackvariancecurve.{hpp,cpp}`:
//! [`BlackVarianceCurve`] calculates time-dependent Black volatilities from a
//! vector of (ATM) Black volatilities observed in the market, interpolating on
//! the variance curve (variance = t * vol^2, with a zero-variance node pinned
//! at the reference date). [`black_variance_impl`](BlackVolTermStructure::black_variance_impl)
//! interpolates; [`black_vol_impl`](BlackVolTermStructure::black_vol_impl)
//! derives `sqrt(variance / t)` (C++'s `BlackVarianceTermStructure` adapter).
//! There is no strike dependence; see `BlackVarianceSurface` for that.
//!
//! ## Divergences from QuantLib
//!
//! - C++ defaults to linear interpolation and can switch via the
//!   `setInterpolation<Interpolator>()` template method; here the curve is
//!   generic over the [`Interpolator`] factory, fixed at construction
//!   ([`new`](BlackVarianceCurve::new) picks [`Linear`],
//!   [`with_interpolator`](BlackVarianceCurve::with_interpolator) takes any).
//! - The `BlackVolTimeExtrapolation` helper is ported here in its curve form
//!   only; the strike-dependent surface form follows with
//!   `BlackVarianceSurface`.
//! - `UseInterpolator` time extrapolation needs the interpolation evaluated
//!   past its last node, which the L1 [`Interpolation`] trait cannot enable
//!   generically (its extrapolation flag is per object, set at construction);
//!   it returns an `Err` until the trait grows a setter.
//! - Empty date vectors are an explicit `Err` where C++ reads `dates.back()`
//!   undefined.

use crate::errors::QlResult;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::volatility::{BlackVolTermStructure, VolatilityTermStructure};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};
use crate::{fail, require};

/// Time-extrapolation strategy for Black volatility past the last node
/// (`BlackVolTimeExtrapolation::Type`, curve form).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlackVolTimeExtrapolation {
    /// Flat extrapolation of the latest available volatility.
    FlatVolatility,
    /// Delegate extrapolation to the underlying interpolation.
    UseInterpolator,
    /// Linear extrapolation of variance from the last two available nodes.
    LinearVariance,
}

/// Black volatility curve interpolated on variances.
pub struct BlackVarianceCurve<I: Interpolator = Linear> {
    base: TermStructureBase,
    max_date: Date,
    curve: InterpolatedCurve<I>,
    time_extrapolation: BlackVolTimeExtrapolation,
}

impl BlackVarianceCurve<Linear> {
    /// Variance curve with the C++ defaults: linear interpolation on
    /// variances and flat-volatility time extrapolation.
    pub fn new(
        reference_date: Date,
        dates: &[Date],
        black_vol_curve: &[Volatility],
        day_counter: DayCounter,
        force_monotone_variance: bool,
    ) -> QlResult<BlackVarianceCurve<Linear>> {
        Self::with_interpolator(
            reference_date,
            dates,
            black_vol_curve,
            day_counter,
            force_monotone_variance,
            BlackVolTimeExtrapolation::FlatVolatility,
            Linear,
        )
    }
}

impl<I: Interpolator> BlackVarianceCurve<I> {
    /// Variance curve over `(date, vol)` nodes with an explicit interpolator
    /// factory and time-extrapolation strategy.
    ///
    /// The first date must follow the reference date (the variance at the
    /// reference date is pinned to zero), dates must be sorted unique, and
    /// variances must be non-decreasing unless `force_monotone_variance` is
    /// disabled.
    pub fn with_interpolator(
        reference_date: Date,
        dates: &[Date],
        black_vol_curve: &[Volatility],
        day_counter: DayCounter,
        force_monotone_variance: bool,
        time_extrapolation: BlackVolTimeExtrapolation,
        interpolator: I,
    ) -> QlResult<BlackVarianceCurve<I>> {
        require!(
            dates.len() == black_vol_curve.len(),
            "mismatch between date vector and black vol vector"
        );
        let Some(&max_date) = dates.last() else {
            fail!("no dates given");
        };
        require!(
            dates[0] > reference_date,
            "cannot have dates[0] <= referenceDate"
        );

        let mut times = Vec::with_capacity(dates.len() + 1);
        let mut variances = Vec::with_capacity(dates.len() + 1);
        times.push(0.0);
        variances.push(0.0);
        for (j, (&date, &vol)) in dates.iter().zip(black_vol_curve).enumerate() {
            let t = day_counter.year_fraction(reference_date, date);
            if t <= times[j] || t.is_nan() {
                fail!("dates must be sorted unique!");
            }
            let variance = t * vol * vol;
            if force_monotone_variance && (variance < variances[j] || variance.is_nan()) {
                fail!("variance must be non-decreasing");
            }
            times.push(t);
            variances.push(variance);
        }

        let mut curve = InterpolatedCurve::new(times, variances, interpolator);
        curve.setup_interpolation()?;
        Ok(BlackVarianceCurve {
            base: TermStructureBase::with_reference_date(reference_date, None, Some(day_counter)),
            max_date,
            curve,
            time_extrapolation,
        })
    }

    fn last_time(&self) -> Time {
        *self
            .curve
            .times()
            .last()
            .expect("the curve holds the pinned node and at least one date node")
    }

    fn extrapolated_variance(&self, t: Time) -> QlResult<Real> {
        let interpolation = self.curve.interpolation()?;
        match self.time_extrapolation {
            BlackVolTimeExtrapolation::FlatVolatility => {
                let back = self.last_time();
                let variance = clamp_variance(interpolation.value(back)?);
                Ok(variance / back * t)
            }
            BlackVolTimeExtrapolation::UseInterpolator => {
                fail!(
                    "UseInterpolator time extrapolation needs an extrapolating interpolation, which the interpolation layer does not expose yet"
                );
            }
            BlackVolTimeExtrapolation::LinearVariance => {
                let times = self.curve.times();
                require!(
                    times.len() >= 2,
                    "at least two times required for volatility extrapolation"
                );
                let t1 = times[times.len() - 2];
                let t2 = times[times.len() - 1];
                let v1 = interpolation.value(t1)?;
                let v2 = interpolation.value(t2)?;
                linear_extrapolation(t, t1, t2, v1, v2)
            }
        }
    }
}

fn clamp_variance(variance: Real) -> Real {
    if variance < 0.0 { 0.0 } else { variance }
}

fn linear_extrapolation(t: Time, t1: Time, t2: Time, v1: Real, v2: Real) -> QlResult<Real> {
    if t <= 0.0 || t.is_nan() {
        fail!("t must be greater than 0.0");
    }
    if t <= t2 {
        fail!("t must be greater than times[1]");
    }
    if t2 <= t1 {
        fail!("times must be sorted");
    }
    if v2 < v1 || v1.is_nan() || v2.is_nan() {
        fail!("variances must be non-decreasing");
    }
    Ok(v1 + (t - t1) * (v2 - v1) / (t2 - t1))
}

impl<I: Interpolator> AsObservable for BlackVarianceCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for BlackVarianceCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.max_date
    }
}

impl<I: Interpolator> VolatilityTermStructure for BlackVarianceCurve<I> {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::Following
    }

    fn min_strike(&self) -> Rate {
        Rate::MIN
    }

    fn max_strike(&self) -> Rate {
        Rate::MAX
    }
}

impl<I: Interpolator + 'static> BlackVolTermStructure for BlackVarianceCurve<I> {
    fn black_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility> {
        let non_zero_maturity = if t == 0.0 { 0.00001 } else { t };
        let variance = self.black_variance_impl(non_zero_maturity, strike)?;
        Ok((variance / non_zero_maturity).sqrt())
    }

    fn black_variance_impl(&self, t: Time, _strike: Real) -> QlResult<Real> {
        if t <= self.last_time() {
            Ok(clamp_variance(self.curve.interpolation()?.value(t)?))
        } else {
            self.extrapolated_variance(t)
        }
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

    // Actual360 times 0.5, 1.0, 2.0; variances t * vol^2 = 0.02, 0.0625, 0.18
    // behind the pinned (0, 0) node.
    fn sample() -> BlackVarianceCurve {
        let r = reference();
        BlackVarianceCurve::new(
            r,
            &[r + 180, r + 360, r + 720],
            &[0.20, 0.25, 0.30],
            Actual360::new(),
            true,
        )
        .unwrap()
    }

    #[test]
    fn vols_at_the_nodes_round_trip() {
        let curve = sample();
        for (days, vol) in [(180, 0.20), (360, 0.25), (720, 0.30)] {
            let date = reference() + days;
            let t = curve.time_from_reference(date).unwrap();
            assert!((curve.black_vol(t, 100.0, false).unwrap() - vol).abs() < 1.0e-15);
            assert!((curve.black_vol_date(date, 100.0, false).unwrap() - vol).abs() < 1.0e-15);
            let variance = curve.black_variance(t, 100.0, false).unwrap();
            assert!((variance - t * vol * vol).abs() < 1.0e-15);
        }
    }

    #[test]
    fn variance_interpolates_linearly_between_nodes() {
        let curve = sample();
        let variance = curve.black_variance(0.75, 100.0, false).unwrap();
        assert!((variance - 0.04125).abs() < 1.0e-15);

        let vol = curve.black_vol(0.75, 100.0, false).unwrap();
        assert!((vol - (0.04125_f64 / 0.75).sqrt()).abs() < 1.0e-15);
    }

    #[test]
    fn short_end_pins_zero_variance_at_the_reference_date() {
        let curve = sample();
        assert_eq!(curve.black_variance(0.0, 100.0, false).unwrap(), 0.0);

        let variance = curve.black_variance(0.25, 100.0, false).unwrap();
        assert!((variance - 0.01).abs() < 1.0e-15);
        let vol = curve.black_vol(0.25, 100.0, false).unwrap();
        assert!((vol - 0.20).abs() < 1.0e-15);

        let at_zero = curve.black_vol(0.0, 100.0, false).unwrap();
        assert!((at_zero - 0.20).abs() < 1.0e-12);
    }

    #[test]
    fn vol_and_variance_queries_stay_consistent() {
        let curve = sample();
        for t in [0.1, 0.5, 0.9, 1.3, 2.0] {
            let vol = curve.black_vol(t, 100.0, false).unwrap();
            let variance = curve.black_variance(t, 100.0, false).unwrap();
            assert!((variance - vol * vol * t).abs() < 1.0e-15, "t={t}");
        }
    }

    #[test]
    fn constructor_rejects_invalid_inputs() {
        let r = reference();
        let dc = Actual360::new();

        let err = BlackVarianceCurve::new(r, &[r + 180], &[0.2, 0.25], dc.clone(), true)
            .err()
            .unwrap();
        assert!(err.message().contains("mismatch"));

        let err = BlackVarianceCurve::new(r, &[r, r + 180], &[0.2, 0.25], dc.clone(), true)
            .err()
            .unwrap();
        assert!(err.message().contains("cannot have dates[0]"));

        let err = BlackVarianceCurve::new(r, &[r + 360, r + 180], &[0.2, 0.25], dc.clone(), true)
            .err()
            .unwrap();
        assert!(err.message().contains("sorted unique"));

        let err = BlackVarianceCurve::new(r, &[], &[], dc, true)
            .err()
            .unwrap();
        assert!(err.message().contains("no dates"));
    }

    #[test]
    fn max_date_is_the_last_node_and_gates_the_range() {
        let curve = sample();
        assert_eq!(TermStructure::max_date(&curve), reference() + 720);
        assert!(curve.black_vol(2.0, 100.0, false).is_ok());
        assert!(curve.black_vol(2.5, 100.0, false).is_err());
        curve.enable_extrapolation();
        assert!(curve.black_vol(2.5, 100.0, false).is_ok());
    }
}
