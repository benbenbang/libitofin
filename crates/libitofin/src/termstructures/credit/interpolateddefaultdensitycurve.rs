//! Interpolated default densities with flat terminal density extrapolation.
//!
//! Port of `ql/termstructures/credit/interpolateddefaultdensitycurve.hpp:147-172`.
//! Uses exact interpolation primitives instead of the adapter quadrature fallback.
//! The survival floor preserves NaN arithmetic results, matching QuantLib.

use crate::errors::QlResult;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::termstructures::credit::defaultdensitystructure::DefaultDensityStructure;
use crate::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Probability, Real, Time};

/// Credit curve interpolating default densities at fixed dates.
pub struct InterpolatedDefaultDensityCurve<I: Interpolator> {
    base: TermStructureBase,
    dates: Vec<Date>,
    curve: InterpolatedCurve<I>,
}

impl<I: Interpolator> InterpolatedDefaultDensityCurve<I> {
    /// Builds a curve whose first date is the reference date.
    ///
    /// # Errors
    /// Rejects invalid dates, mismatched lengths, and negative or nonfinite densities.
    pub fn new(
        dates: Vec<Date>,
        densities: Vec<Real>,
        day_counter: DayCounter,
        interpolator: I,
    ) -> QlResult<Self> {
        Self::with_calendar(dates, densities, day_counter, None, interpolator)
    }

    /// Builds a default-density curve carrying an optional calendar.
    pub fn with_calendar(
        dates: Vec<Date>,
        densities: Vec<Real>,
        day_counter: DayCounter,
        calendar: Option<Calendar>,
        interpolator: I,
    ) -> QlResult<Self> {
        require!(
            dates.len() >= interpolator.required_points().max(1),
            "not enough input dates given"
        );
        require!(dates.len() == densities.len(), "dates/data count mismatch");
        require!(
            densities
                .iter()
                .all(|density| density.is_finite() && *density >= 0.0),
            "default densities must be finite and nonnegative"
        );
        require!(
            dates.iter().all(|date| *date != Date::null()),
            "null curve date"
        );
        let reference_date = dates[0];
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference_date, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, densities, interpolator);
        curve.setup_interpolation()?;
        Ok(Self {
            base: TermStructureBase::with_reference_date(
                reference_date,
                calendar,
                Some(day_counter),
            ),
            dates,
            curve,
        })
    }

    /// The node dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node times.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node default densities.
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The node default densities.
    pub fn default_densities(&self) -> &[Real] {
        self.data()
    }

    /// The date and default-density nodes.
    pub fn nodes(&self) -> Vec<(Date, Real)> {
        self.dates
            .iter()
            .copied()
            .zip(self.data().iter().copied())
            .collect()
    }
}

pub(crate) fn survival_from_density_nodes<I: Interpolation>(
    interpolation: &I,
    t: Time,
) -> QlResult<Probability> {
    if t == 0.0 {
        return Ok(1.0);
    }
    let end = interpolation.x_max();
    let integral = if t <= end {
        interpolation.primitive(t)?
    } else {
        interpolation.primitive(end)? + interpolation.value(end)? * (t - end)
    };
    let survival = 1.0 - integral;
    Ok(if survival < 0.0 { 0.0 } else { survival })
}

pub(crate) fn default_density_from_nodes<I: Interpolation>(
    interpolation: &I,
    t: Time,
) -> QlResult<Real> {
    interpolation.value(t.min(interpolation.x_max()))
}

impl<I: Interpolator> AsObservable for InterpolatedDefaultDensityCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedDefaultDensityCurve<I> {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        *self
            .dates
            .last()
            .expect("the constructor requires node dates")
    }
}

impl<I: Interpolator + 'static> DefaultDensityStructure for InterpolatedDefaultDensityCurve<I> {}

impl<I: Interpolator + 'static> DefaultProbabilityTermStructure
    for InterpolatedDefaultDensityCurve<I>
{
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
    fn survival_probability_impl(&self, t: Time) -> QlResult<Probability> {
        survival_from_density_nodes(self.curve.interpolation()?, t)
    }
    fn default_density_impl(&self, t: Time) -> QlResult<Real> {
        default_density_from_nodes(self.curve.interpolation()?, t)
    }
}
