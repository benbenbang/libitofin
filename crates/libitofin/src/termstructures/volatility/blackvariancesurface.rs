//! Black volatility surface modelled as a variance surface.
//!
//! Port of `ql/termstructures/volatility/equityfx/blackvariancesurface.{hpp,cpp}`:
//! [`BlackVarianceSurface`] turns a market matrix of Black volatilities per
//! (strike, date) into variances (`vol^2 * t`, with a zero-variance column
//! prepended at `t = 0`), interpolates on that surface and derives the
//! volatility back as `sqrt(var / t)` (C++'s `BlackVarianceTermStructure`
//! adapter). Bilinear interpolation is the construction default and can be
//! swapped through [`set_interpolation`](BlackVarianceSurface::set_interpolation).
//!
//! Beyond the last time node the variance extrapolates linearly in time
//! (`var(t_max, k) * t / t_max`); outside the strike grid the behaviour is
//! chosen per side via [`Extrapolation`]: clamp to the boundary strike or
//! extend the interpolation's boundary cells. Both still require the query to
//! pass the strike check, i.e. curve-level extrapolation must be on or the
//! per-call flag passed.
//!
//! ## Divergences from QuantLib
//!
//! - C++'s `setInterpolation` template method takes the interpolator traits
//!   class; here it takes an [`Interpolator2D`] and rebuilds the boxed
//!   surface, enabling extrapolation on it once (C++ passes `true` on every
//!   evaluation instead).
//! - The construction checks (`QL_REQUIRE`) become `Err`s per D4, and an
//!   empty date vector is an explicit error where C++ would read past the
//!   end.
//! - A single-strike grid (`strikes.len() == 1`) fails construction with the
//!   interpolator's own "at least 2 y points" error rather than a
//!   domain-level check, since `Bilinear`/`Bicubic` cannot span a one-node
//!   axis (the same limitation as C++'s `Bilinear`).

use crate::errors::QlResult;
use crate::math::interpolations::bilinear::Bilinear;
use crate::math::interpolations::{Interpolation2D, Interpolator2D};
use crate::math::matrix::Matrix;
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Rate, Real, Time, Volatility};

use super::{BlackVolTermStructure, VolatilityTermStructure};

/// Strike-extrapolation behaviour outside the surface's strike grid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extrapolation {
    /// Clamp the strike to the nearest grid boundary.
    ConstantExtrapolation,
    /// Let the interpolation extend its boundary cells.
    InterpolatorDefaultExtrapolation,
}

/// Black volatility surface interpolating market vols on the variance
/// surface, time-strike dependent.
pub struct BlackVarianceSurface {
    base: TermStructureBase,
    max_date: Date,
    strikes: Vec<Real>,
    times: Vec<Time>,
    variances: Vec<Vec<Real>>,
    variance_surface: Box<dyn Interpolation2D>,
    lower_extrapolation: Extrapolation,
    upper_extrapolation: Extrapolation,
}

impl BlackVarianceSurface {
    /// Surface with interpolator-default strike extrapolation on both sides
    /// (the C++ default arguments).
    ///
    /// `black_vol_matrix` holds one row per strike and one column per date.
    pub fn new(
        reference_date: Date,
        calendar: Option<Calendar>,
        dates: &[Date],
        strikes: Vec<Real>,
        black_vol_matrix: &Matrix,
        day_counter: DayCounter,
    ) -> QlResult<BlackVarianceSurface> {
        Self::with_strike_extrapolation(
            reference_date,
            calendar,
            dates,
            strikes,
            black_vol_matrix,
            day_counter,
            Extrapolation::InterpolatorDefaultExtrapolation,
            Extrapolation::InterpolatorDefaultExtrapolation,
        )
    }

    /// Surface with explicit lower/upper strike-extrapolation behaviour.
    #[allow(clippy::too_many_arguments)]
    pub fn with_strike_extrapolation(
        reference_date: Date,
        calendar: Option<Calendar>,
        dates: &[Date],
        strikes: Vec<Real>,
        black_vol_matrix: &Matrix,
        day_counter: DayCounter,
        lower_extrapolation: Extrapolation,
        upper_extrapolation: Extrapolation,
    ) -> QlResult<BlackVarianceSurface> {
        require!(
            dates.len() == black_vol_matrix.columns(),
            "mismatch between date vector and vol matrix columns"
        );
        require!(
            strikes.len() == black_vol_matrix.rows(),
            "mismatch between money-strike vector and vol matrix rows"
        );
        require!(!dates.is_empty(), "no dates given");
        require!(
            dates[0] >= reference_date,
            "cannot have dates[0] < referenceDate"
        );

        let mut times = vec![0.0; dates.len() + 1];
        let mut variances = vec![vec![0.0; dates.len() + 1]; strikes.len()];
        for j in 1..=dates.len() {
            times[j] = day_counter.year_fraction(reference_date, dates[j - 1]);
            if times[j] <= times[j - 1] || times[j].is_nan() {
                crate::fail!("dates must be sorted unique!");
            }
            for (i, row) in variances.iter_mut().enumerate() {
                let vol = black_vol_matrix[(i, j - 1)];
                row[j] = times[j] * vol * vol;
            }
        }
        let variance_surface = Self::build(&Bilinear, &times, &strikes, &variances)?;
        Ok(BlackVarianceSurface {
            base: TermStructureBase::with_reference_date(
                reference_date,
                calendar,
                Some(day_counter),
            ),
            max_date: dates[dates.len() - 1],
            strikes,
            times,
            variances,
            variance_surface,
            lower_extrapolation,
            upper_extrapolation,
        })
    }

    fn build<I>(
        interpolator: &I,
        times: &[Time],
        strikes: &[Real],
        variances: &[Vec<Real>],
    ) -> QlResult<Box<dyn Interpolation2D>>
    where
        I: Interpolator2D,
        I::Output: 'static,
    {
        let mut surface =
            interpolator.interpolate(times.to_vec(), strikes.to_vec(), variances.to_vec())?;
        surface.set_extrapolation(true);
        Ok(Box::new(surface))
    }

    /// Rebuilds the variance interpolation with another interpolator (C++'s
    /// template `setInterpolation`; bilinear is the construction default) and
    /// notifies observers.
    pub fn set_interpolation<I>(&mut self, interpolator: &I) -> QlResult<()>
    where
        I: Interpolator2D,
        I::Output: 'static,
    {
        self.variance_surface =
            Self::build(interpolator, &self.times, &self.strikes, &self.variances)?;
        self.observable().notify_observers();
        Ok(())
    }
}

impl AsObservable for BlackVarianceSurface {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for BlackVarianceSurface {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        self.max_date
    }
}

impl VolatilityTermStructure for BlackVarianceSurface {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::Following
    }

    fn min_strike(&self) -> Rate {
        self.strikes[0]
    }

    fn max_strike(&self) -> Rate {
        self.strikes[self.strikes.len() - 1]
    }
}

impl BlackVolTermStructure for BlackVarianceSurface {
    fn black_vol_impl(&self, t: Time, strike: Real) -> QlResult<Volatility> {
        let non_zero_maturity = if t == 0.0 { 0.00001 } else { t };
        let var = self.black_variance_impl(non_zero_maturity, strike)?;
        Ok((var / non_zero_maturity).sqrt())
    }

    fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
        if t == 0.0 {
            return Ok(0.0);
        }
        let mut strike = strike;
        if strike < self.strikes[0]
            && self.lower_extrapolation == Extrapolation::ConstantExtrapolation
        {
            strike = self.strikes[0];
        }
        let last_strike = self.strikes[self.strikes.len() - 1];
        if strike > last_strike && self.upper_extrapolation == Extrapolation::ConstantExtrapolation
        {
            strike = last_strike;
        }
        let t_max = self.times[self.times.len() - 1];
        if t <= t_max {
            self.variance_surface.value(t, strike)
        } else {
            Ok(self.variance_surface.value(t_max, strike)? * t / t_max)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::interpolations::bicubic::Bicubic;
    use crate::test_support::{Flag, as_observer};
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    // Strikes [90, 100, 110] x dates [ref+365d, ref+730d] (t = 1, 2 under
    // Actual365Fixed), vols per (strike, date):
    //   90:  0.20  0.25        variances:  0.0400  0.1250
    //   100: 0.18  0.22                    0.0324  0.0968
    //   110: 0.16  0.20                    0.0256  0.0800
    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn vol_matrix() -> Matrix {
        let vols = [[0.20, 0.25], [0.18, 0.22], [0.16, 0.20]];
        let mut m = Matrix::with_size(3, 2);
        for (i, row) in vols.iter().enumerate() {
            for (j, &vol) in row.iter().enumerate() {
                m[(i, j)] = vol;
            }
        }
        m
    }

    fn surface_with(lower: Extrapolation, upper: Extrapolation) -> BlackVarianceSurface {
        let reference = reference();
        BlackVarianceSurface::with_strike_extrapolation(
            reference,
            None,
            &[reference + 365, reference + 730],
            vec![90.0, 100.0, 110.0],
            &vol_matrix(),
            Actual365Fixed::new(),
            lower,
            upper,
        )
        .unwrap()
    }

    fn surface() -> BlackVarianceSurface {
        surface_with(
            Extrapolation::InterpolatorDefaultExtrapolation,
            Extrapolation::InterpolatorDefaultExtrapolation,
        )
    }

    fn assert_close(got: Real, expected: Real) {
        assert!(
            (got - expected).abs() < 1e-14,
            "got {got}, expected {expected}"
        );
    }

    #[test]
    fn nodes_reproduce_variance_and_vol() {
        let s = surface();
        assert_close(s.black_variance(1.0, 100.0, false).unwrap(), 0.0324);
        assert_close(s.black_variance(2.0, 90.0, false).unwrap(), 0.125);
        assert_close(s.black_vol(1.0, 90.0, false).unwrap(), 0.20);
        assert_close(s.black_vol(2.0, 110.0, false).unwrap(), 0.20);
        assert_close(
            s.black_vol_date(reference() + 365, 100.0, false).unwrap(),
            0.18,
        );
    }

    #[test]
    fn variance_interpolates_bilinearly_between_nodes() {
        let s = surface();
        assert_close(s.black_variance(0.5, 100.0, false).unwrap(), 0.0324 * 0.5);
        let corners = (0.04 + 0.0324 + 0.125 + 0.0968) / 4.0;
        assert_close(s.black_variance(1.5, 95.0, false).unwrap(), corners);
    }

    #[test]
    fn zero_time_has_zero_variance_and_the_short_end_vol() {
        let s = surface();
        assert_close(s.black_variance(0.0, 100.0, false).unwrap(), 0.0);
        // var is linear from (0, 0) to (1, 0.0324), so sqrt(var(eps)/eps)
        // recovers the first-node vol exactly.
        let vol = s.black_vol(0.0, 100.0, false).unwrap();
        assert!((vol - 0.18).abs() < 1e-12, "got {vol}");
    }

    #[test]
    fn variance_extrapolates_linearly_in_time_past_the_last_node() {
        let s = surface();
        assert_close(
            s.black_variance(4.0, 100.0, true).unwrap(),
            0.0968 * 4.0 / 2.0,
        );
        assert!(s.black_variance(4.0, 100.0, false).is_err());
        s.enable_extrapolation();
        assert_close(s.black_variance(4.0, 100.0, false).unwrap(), 0.0968 * 2.0);
    }

    #[test]
    fn default_strike_extrapolation_extends_the_boundary_cells() {
        let s = surface();
        assert_close(s.black_variance(1.0, 120.0, true).unwrap(), 0.0188);
        assert_close(s.black_variance(1.0, 80.0, true).unwrap(), 0.0476);
    }

    #[test]
    fn constant_strike_extrapolation_clamps_to_the_boundary() {
        let s = surface_with(
            Extrapolation::ConstantExtrapolation,
            Extrapolation::ConstantExtrapolation,
        );
        assert_close(s.black_variance(1.0, 80.0, true).unwrap(), 0.04);
        assert_close(s.black_variance(1.0, 120.0, true).unwrap(), 0.0256);

        let mixed = surface_with(
            Extrapolation::ConstantExtrapolation,
            Extrapolation::InterpolatorDefaultExtrapolation,
        );
        assert_close(mixed.black_variance(1.0, 80.0, true).unwrap(), 0.04);
        assert_close(mixed.black_variance(1.0, 120.0, true).unwrap(), 0.0188);
    }

    #[test]
    fn strike_checks_gate_the_grid_domain() {
        let s = surface();
        assert_eq!(s.min_strike(), 90.0);
        assert_eq!(s.max_strike(), 110.0);
        assert_eq!(s.max_date(), reference() + 730);
        let err = s.black_variance(1.0, 120.0, false).unwrap_err();
        assert!(err.message().contains("outside the curve domain"));
    }

    #[test]
    fn vol_and_variance_stay_consistent_off_nodes() {
        let s = surface();
        for (t, k) in [(0.7, 93.0), (1.3, 104.5), (2.0, 100.0)] {
            let vol = s.black_vol(t, k, false).unwrap();
            let var = s.black_variance(t, k, false).unwrap();
            assert!((vol * vol * t - var).abs() < 1e-14);
        }
    }

    #[test]
    fn forward_variance_is_additive_across_nodes() {
        let s = surface();
        let fwd = s.black_forward_variance(1.0, 2.0, 90.0, false).unwrap();
        assert_close(fwd, 0.125 - 0.04);
    }

    #[test]
    fn set_interpolation_reproduces_nodes_and_notifies() {
        let mut s = surface();
        let flag = Flag::new();
        s.observable().register_observer(&as_observer(&flag));
        s.set_interpolation(&Bicubic).unwrap();
        assert!(Flag::is_up(&flag));
        assert!((s.black_variance(1.0, 100.0, false).unwrap() - 0.0324).abs() < 1e-12);
        assert!((s.black_vol(2.0, 90.0, false).unwrap() - 0.25).abs() < 1e-12);
    }

    fn expect_err(result: QlResult<BlackVarianceSurface>) -> String {
        match result {
            Ok(_) => panic!("expected a construction error"),
            Err(err) => err.message().to_string(),
        }
    }

    #[test]
    fn construction_errors_match_the_quantlib_checks() {
        let reference = reference();
        let dates = [reference + 365, reference + 730];

        let err = expect_err(BlackVarianceSurface::new(
            reference,
            None,
            &[reference + 730, reference + 365],
            vec![90.0, 100.0, 110.0],
            &vol_matrix(),
            Actual365Fixed::new(),
        ));
        assert!(err.contains("sorted unique"));

        let err = expect_err(BlackVarianceSurface::new(
            reference,
            None,
            &[reference - 1, reference + 365],
            vec![90.0, 100.0, 110.0],
            &vol_matrix(),
            Actual365Fixed::new(),
        ));
        assert!(err.contains("cannot have dates[0]"));

        let err = expect_err(BlackVarianceSurface::new(
            reference,
            None,
            &dates[..1],
            vec![90.0, 100.0, 110.0],
            &vol_matrix(),
            Actual365Fixed::new(),
        ));
        assert!(err.contains("date vector"));

        let err = expect_err(BlackVarianceSurface::new(
            reference,
            None,
            &dates,
            vec![90.0, 100.0],
            &vol_matrix(),
            Actual365Fixed::new(),
        ));
        assert!(err.contains("money-strike"));

        let err = expect_err(BlackVarianceSurface::new(
            reference,
            None,
            &[],
            vec![],
            &Matrix::new(),
            Actual365Fixed::new(),
        ));
        assert!(err.contains("no dates given"));
    }
}
