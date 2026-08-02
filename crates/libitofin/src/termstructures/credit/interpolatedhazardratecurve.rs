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
