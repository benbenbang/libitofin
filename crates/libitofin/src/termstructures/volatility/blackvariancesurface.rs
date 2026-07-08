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
