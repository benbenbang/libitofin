//! Interpolated year-on-year optionlet volatility curve.
//!
//! Port of `InterpolatedYoYOptionletVolatilityCurve`
//! (`ql/experimental/inflation/yoyinflationoptionletvolatilitystructure2.hpp:39-186`):
//! a flat-smile surface, interpolated in the time direction and constant in
//! strike, which the optionlet stripper rebuilds on every solver iteration and
//! the piecewise bootstrap produces per strike. It embeds the shared
//! [`YoYOptionletVolatilitySurfaceBase`] holder.
//!
//! ## Divergences from QuantLib
//!
//! - The moving reference date takes the shared [`Settings`] handle explicitly
//!   (D5), and every volatility query takes its observation lag explicitly, as
//!   the module docs of [`super`] record for the flat surface.
//! - The protected no-data constructor (`hpp:91-102`) exists in C++ only
//!   because `PiecewiseYoYOptionletVolatilityCurve` derives from this class;
//!   the port's piecewise curve is standalone (as
//!   [`PiecewiseYoYInflationCurve`] is), so it has no counterpart here.
//! - The `VolatilityType`/`displacement` pair is omitted unread, exactly as on
//!   [`ConstantYoYOptionletVolatility`](super::ConstantYoYOptionletVolatility).
//!
//! [`PiecewiseYoYInflationCurve`]: crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve

use crate::errors::QlResult;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::interpolatedcurve::InterpolatedCurve;
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::{YoYOptionletVolatilitySurface, YoYOptionletVolatilitySurfaceBase};

/// The value of `interpolation` at `t`, extended past either end on the end
/// segment's own slope.
///
/// The port of C++'s `interpolation_(t, true)` reads for the wired
/// interpolator: `Linear`'s allowed extrapolation continues the boundary
/// segment, which is the same extension
/// [`PiecewiseYoYInflationCurve::yoy_rate_impl`] spells out on the inflation
/// side.
///
/// [`PiecewiseYoYInflationCurve::yoy_rate_impl`]: crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve
pub(crate) fn extended_value<I: Interpolation>(interpolation: &I, t: Time) -> QlResult<Real> {
    let x_min = interpolation.x_min();
    let x_max = interpolation.x_max();
    if t < x_min {
        Ok(interpolation.value(x_min)? + interpolation.derivative(x_min)? * (t - x_min))
    } else if t > x_max {
        Ok(interpolation.value(x_max)? + interpolation.derivative(x_max)? * (t - x_max))
    } else {
        interpolation.value(t)
    }
}

/// Interpolated flat-smile year-on-year optionlet volatility curve:
/// interpolated in the time direction, constant in strike
/// (`yoyinflationoptionletvolatilitystructure2.hpp:39-114`).
pub struct InterpolatedYoYOptionletVolatilityCurve<I: Interpolator> {
    base: YoYOptionletVolatilitySurfaceBase,
    curve: InterpolatedCurve<I>,
    dates: Vec<Date>,
    min_strike: Rate,
    max_strike: Rate,
}

impl<I: Interpolator> InterpolatedYoYOptionletVolatilityCurve<I> {
    /// Curve through the volatilities quoted at the given dates
    /// (`hpp:118-153`). The dates are those of the volatility, with no lag on
    /// them, but relative to a start earlier than the reference date as always
    /// for inflation (`hpp:45-49`).
    ///
    /// The base level is set to the interpolation's own value at the base
    /// date's time, extended past the first node when that time precedes it
    /// (C++'s `interpolation_(baseTime, true)`, `hpp:149-152`).
    ///
    /// # Errors
    ///
    /// The C++ `QL_REQUIRE`s: a date/volatility count mismatch, or fewer than
    /// two dates; plus an unresolvable reference date.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        dates: Vec<Date>,
        volatilities: Vec<Volatility>,
        min_strike: Rate,
        max_strike: Rate,
        interpolator: I,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<InterpolatedYoYOptionletVolatilityCurve<I>> {
        require!(
            dates.len() == volatilities.len(),
            "must have same number of dates and vols: {} vs {}",
            dates.len(),
            volatilities.len()
        );
        require!(
            dates.len() > 1,
            "must have at least two dates: {}",
            dates.len()
        );

        let base = YoYOptionletVolatilitySurfaceBase::new(
            settlement_days,
            calendar,
            business_day_convention,
            day_counter.clone(),
            observation_lag,
            frequency,
            index_is_interpolated,
            settings,
        );
        let reference = base.term_structure_base().reference_date()?;
        let times = InterpolatedCurve::<I>::times_from_dates(&dates, reference, &day_counter)?;
        let mut curve = InterpolatedCurve::new(times, volatilities, interpolator);
        curve.setup_interpolation()?;

        let base_time = day_counter.year_fraction(reference, base.base_date()?);
        base.set_base_level(extended_value(curve.interpolation()?, base_time)?);

        Ok(InterpolatedYoYOptionletVolatilityCurve {
            base,
            curve,
            dates,
            min_strike,
            max_strike,
        })
    }

    /// The embedded shared holder.
    pub fn surface_base(&self) -> &YoYOptionletVolatilitySurfaceBase {
        &self.base
    }

    /// The node times (`hpp:81`), measured from the reference date.
    pub fn times(&self) -> &[Time] {
        self.curve.times()
    }

    /// The node dates (`hpp:82`).
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The node volatilities (`hpp:83`).
    pub fn data(&self) -> &[Real] {
        self.curve.data()
    }

    /// The (date, volatility) nodes (`hpp:84`).
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
            .expect("construction rejected fewer than two dates")
    }

    /// For the curve the strike is ignored, the smile being flat
    /// (`volatilityImpl`, `hpp:180-186`); C++ evaluates without extrapolation,
    /// so a time off the node range errors here too.
    fn volatility_impl(&self, t: Time) -> QlResult<Volatility> {
        self.curve.interpolation()?.value(t)
    }
}

impl<I: Interpolator> AsObservable for InterpolatedYoYOptionletVolatilityCurve<I> {
    fn observable(&self) -> &Observable {
        self.base.term_structure_base().observable()
    }
}

impl<I: Interpolator> TermStructure for InterpolatedYoYOptionletVolatilityCurve<I> {
    fn base(&self) -> &TermStructureBase {
        self.base.term_structure_base()
    }

    /// C++'s self-described approximation (`hpp:73-76`): the reference date
    /// advanced by the last node time rounded up to whole years. Should the
    /// tenor conversion fail, the last node date stands in.
    fn max_date(&self) -> Date {
        let t_max = *self
            .curve
            .times()
            .last()
            .expect("construction rejected fewer than two dates");
        self.option_date_from_tenor(Period::new(t_max.ceil() as i32, TimeUnit::Years))
            .unwrap_or_else(|_| self.last_date())
    }
}

impl<I: Interpolator> VolatilityTermStructure for InterpolatedYoYOptionletVolatilityCurve<I> {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.base.business_day_convention()
    }

    fn min_strike(&self) -> Rate {
        self.min_strike
    }

    fn max_strike(&self) -> Rate {
        self.max_strike
    }
}

impl<I: Interpolator> YoYOptionletVolatilitySurface for InterpolatedYoYOptionletVolatilityCurve<I> {
    fn base_date(&self) -> QlResult<Date> {
        self.base.base_date()
    }

    fn volatility(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Volatility> {
        let observed = self.base.observed(date - obs_lag)?;
        self.base.check_range(
            observed,
            strike,
            self.min_strike,
            self.max_strike,
            TermStructure::max_date(self),
        )?;
        self.volatility_impl(self.time_from_reference(observed)?)
    }

    fn total_variance(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Real> {
        let volatility = self.volatility(date, strike, obs_lag)?;
        Ok(volatility * volatility * self.base.time_from_base(date, obs_lag)?)
    }

    fn base_level(&self) -> QlResult<Volatility> {
        self.base.base_level()
    }
}
