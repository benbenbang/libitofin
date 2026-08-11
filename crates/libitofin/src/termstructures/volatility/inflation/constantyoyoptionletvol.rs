//! Flat year-on-year inflation optionlet volatility.
//!
//! Port of `ConstantYoYOptionletVolatility`
//! (`yoyinflationoptionletvolatilitystructure.hpp:154-203`): one volatility for
//! every strike and every date, and the date arithmetic of the base surface
//! underneath it (`.cpp:51-166`). The volatility is either a fixed value
//! (wrapped in an unobservable quote, as in C++) or a quote handle whose
//! changes propagate to the surface's observers.
//!
//! Both C++ constructors compute the reference date off the evaluation date;
//! there is no fixed-reference-date form, so neither is there one here. The
//! shared [`Settings`] handle is taken explicitly, per D5.

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::inflationindex::inflation_period;
use crate::patterns::observable::{AsObservable, Observable};
use crate::quotes::{Quote, make_quote_handle};
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::YoYOptionletVolatilitySurface;

/// Constant year-on-year optionlet volatility, no strike or date dependence.
pub struct ConstantYoYOptionletVolatility {
    base: TermStructureBase,
    business_day_convention: BusinessDayConvention,
    volatility: Handle<dyn Quote>,
    observation_lag: Period,
    frequency: Frequency,
    index_is_interpolated: bool,
    min_strike: Rate,
    max_strike: Rate,
}

impl ConstantYoYOptionletVolatility {
    #[allow(clippy::too_many_arguments)]
    fn assemble(
        base: TermStructureBase,
        business_day_convention: BusinessDayConvention,
        volatility: Handle<dyn Quote>,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        min_strike: Rate,
        max_strike: Rate,
        observe: bool,
    ) -> ConstantYoYOptionletVolatility {
        if observe {
            volatility.register_observer(&base.updater());
        }
        ConstantYoYOptionletVolatility {
            base,
            business_day_convention,
            volatility,
            observation_lag,
            frequency,
            index_is_interpolated,
            min_strike,
            max_strike,
        }
    }

    /// A flat surface at `volatility`, its reference date moving off the
    /// evaluation date.
    ///
    /// `min_strike` and `max_strike` bound the strike domain; C++ defaults them
    /// to `-1.0` and `100.0` (`hpp:161-171`), which the port has no default
    /// arguments to carry.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        volatility: Volatility,
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        min_strike: Rate,
        max_strike: Rate,
        settings: Shared<Settings<Date>>,
    ) -> ConstantYoYOptionletVolatility {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            make_quote_handle(volatility).handle(),
            observation_lag,
            frequency,
            index_is_interpolated,
            min_strike,
            max_strike,
            false,
        )
    }

    /// A flat surface quoted by `volatility`; quote changes notify the surface's
    /// observers. See [`new`](Self::new).
    #[allow(clippy::too_many_arguments)]
    pub fn with_quote(
        volatility: Handle<dyn Quote>,
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        min_strike: Rate,
        max_strike: Rate,
        settings: Shared<Settings<Date>>,
    ) -> ConstantYoYOptionletVolatility {
        Self::assemble(
            TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            volatility,
            observation_lag,
            frequency,
            index_is_interpolated,
            min_strike,
            max_strike,
            true,
        )
    }

    /// The lag the surface itself observes inflation with (`observationLag`).
    pub fn observation_lag(&self) -> Period {
        self.observation_lag
    }

    /// How often the observed index publishes (`frequency`).
    pub fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// Whether the observed index interpolates between publications
    /// (`indexIsInterpolated`).
    pub fn index_is_interpolated(&self) -> bool {
        self.index_is_interpolated
    }

    /// `date` as the surface observes it: itself for an interpolated index, the
    /// start of its publication period otherwise (`.cpp:57-63`, `:145-151`).
    ///
    /// # Errors
    ///
    /// When the frequency admits no publication period.
    fn observed(&self, date: Date) -> QlResult<Date> {
        if self.index_is_interpolated {
            Ok(date)
        } else {
            Ok(inflation_period(date, self.frequency)?.0)
        }
    }

    /// The time from [`base_date`](YoYOptionletVolatilitySurface::base_date) to
    /// the date the surface observes for an exercise on `date` (`timeFromBase`,
    /// `.cpp:134-156`).
    ///
    /// # Errors
    ///
    /// As [`base_date`](YoYOptionletVolatilitySurface::base_date), plus a
    /// surface built without a day counter.
    pub fn time_from_base(&self, date: Date, obs_lag: Period) -> QlResult<Time> {
        let observed = self.observed(date - obs_lag)?;
        Ok(self
            .require_day_counter()?
            .year_fraction(self.base_date()?, observed))
    }

    /// The date and strike checks C++ runs before every volatility query
    /// (`checkRange`, `.cpp:66-77`).
    ///
    /// The max-date clause is omitted: this surface's
    /// [`max_date`](TermStructure::max_date) is [`Date::max_date`], so it can
    /// never fire. The `extrapolate` argument is not part of the lean trait, so
    /// the strike check runs as C++ does with the default `extrapolate = false`,
    /// still yielding to
    /// [`enable_extrapolation`](TermStructure::enable_extrapolation).
    fn check_range(&self, date: Date, strike: Rate) -> QlResult<()> {
        let base_date = self.base_date()?;
        require!(
            date >= base_date,
            "date ({date}) is before base date ({base_date})"
        );
        require!(
            self.allows_extrapolation() || (strike >= self.min_strike && strike <= self.max_strike),
            "strike ({strike}) is outside the curve domain [{min},{max}] at date = {date}",
            min = self.min_strike,
            max = self.max_strike
        );
        Ok(())
    }
}

impl AsObservable for ConstantYoYOptionletVolatility {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl TermStructure for ConstantYoYOptionletVolatility {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }

    fn max_date(&self) -> Date {
        Date::max_date()
    }
}

impl VolatilityTermStructure for ConstantYoYOptionletVolatility {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    fn min_strike(&self) -> Rate {
        self.min_strike
    }

    fn max_strike(&self) -> Rate {
        self.max_strike
    }
}

impl YoYOptionletVolatilitySurface for ConstantYoYOptionletVolatility {
    fn base_date(&self) -> QlResult<Date> {
        self.observed(self.reference_date()? - self.observation_lag)
    }

    /// Flat: the quote, whatever the date and strike, once both have passed
    /// [`check_range`](Self::check_range). C++ derives an option time and hands
    /// it to `volatilityImpl`, which discards it (`.cpp:104-114`).
    fn volatility(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Volatility> {
        let observed = self.observed(date - obs_lag)?;
        self.check_range(observed, strike)?;
        self.volatility.current_link()?.value()
    }

    fn total_variance(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Real> {
        let volatility = self.volatility(date, strike, obs_lag)?;
        Ok(volatility * volatility * self.time_from_base(date, obs_lag)?)
    }
}
