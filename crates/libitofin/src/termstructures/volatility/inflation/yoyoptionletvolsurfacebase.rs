//! Shared holder for year-on-year optionlet volatility surfaces.
//!
//! Port of the C++ abstract base `YoYOptionletVolatilitySurface`'s members
//! (`ql/termstructures/volatility/inflation/yoyinflationoptionletvolatilitystructure.hpp:141-149`)
//! and date arithmetic (`.cpp:51-154`), including the `baseLevel` slot the
//! stripping bootstrap seeds from. On the pattern of
//! [`YoYCapFloorTermPriceSurfaceBase`]: the stripping hierarchy's concrete
//! surfaces (#874) embed one where [`ConstantYoYOptionletVolatility`] - which
//! predates it and stays untouched - carries the same arithmetic itself.
//!
//! The moving reference date takes the shared [`Settings`] handle explicitly
//! (D5), and the lag-substituting sentinel default is not carried, per the
//! module docs of [`super`].
//!
//! [`YoYCapFloorTermPriceSurfaceBase`]: crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurfaceBase
//! [`ConstantYoYOptionletVolatility`]: super::ConstantYoYOptionletVolatility

use std::cell::Cell;

use crate::errors::QlResult;
use crate::indexes::inflationindex::inflation_period;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::TermStructureBase;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::types::{Natural, Rate, Time, Volatility};

/// Shared holder for year-on-year optionlet volatility surfaces: the C++
/// abstract base's members (`yoyinflationoptionletvolatilitystructure.hpp:141-149`)
/// and its date arithmetic (`.cpp:51-154`), including the `baseLevel` slot the
/// stripping bootstrap seeds from.
pub struct YoYOptionletVolatilitySurfaceBase {
    term: TermStructureBase,
    business_day_convention: BusinessDayConvention,
    observation_lag: Period,
    frequency: Frequency,
    index_is_interpolated: bool,
    base_level: Cell<Option<Volatility>>,
}

impl YoYOptionletVolatilitySurfaceBase {
    /// The abstract-base constructor (`.cpp:31-48`): a moving term structure
    /// whose reference date follows the evaluation date `settings` carries,
    /// with the base level starting unset (C++'s `Null<Volatility>`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        frequency: Frequency,
        index_is_interpolated: bool,
        settings: Shared<Settings<Date>>,
    ) -> YoYOptionletVolatilitySurfaceBase {
        YoYOptionletVolatilitySurfaceBase {
            term: TermStructureBase::moving(settlement_days, calendar, Some(day_counter), settings),
            business_day_convention,
            observation_lag,
            frequency,
            index_is_interpolated,
            base_level: Cell::new(None),
        }
    }

    /// The wrapped term-structure holder.
    pub fn term_structure_base(&self) -> &TermStructureBase {
        &self.term
    }

    /// The convention tenors are converted to dates with (`hpp` via
    /// `VolatilityTermStructure`).
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.business_day_convention
    }

    /// The lag the surface observes inflation with (`observationLag`).
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

    /// The volatility acting as the zero-time value for bootstrapping
    /// (`baseLevel`, `hpp:124-128`); an `Err` while unset, C++'s throw on the
    /// `Null<Volatility>` sentinel.
    pub fn base_level(&self) -> QlResult<Volatility> {
        match self.base_level.get() {
            Some(level) => Ok(level),
            None => crate::fail!("base volatility, for base_date(), not set"),
        }
    }

    /// Sets the base level (`setBaseLevel`, `hpp:141`).
    pub fn set_base_level(&self, level: Volatility) {
        self.base_level.set(Some(level));
    }

    /// `date` as the surface observes it: itself for an interpolated index,
    /// the start of its publication period otherwise (`.cpp:57-63`).
    ///
    /// # Errors
    ///
    /// When the frequency admits no publication period.
    pub fn observed(&self, date: Date) -> QlResult<Date> {
        if self.index_is_interpolated {
            Ok(date)
        } else {
            Ok(inflation_period(date, self.frequency)?.0)
        }
    }

    /// The reference date pulled back by the surface's own lag and observed
    /// (`baseDate`, `.cpp:51-64`).
    ///
    /// # Errors
    ///
    /// When the reference date cannot be resolved, or the frequency admits no
    /// publication period.
    pub fn base_date(&self) -> QlResult<Date> {
        self.observed(self.term.reference_date()? - self.observation_lag)
    }

    /// The time from [`base_date`](Self::base_date) to the date observed for
    /// an exercise on `date` under the handed lag (`timeFromBase`,
    /// `.cpp:133-154`).
    ///
    /// # Errors
    ///
    /// As [`base_date`](Self::base_date), plus a surface built without a day
    /// counter.
    pub fn time_from_base(&self, date: Date, obs_lag: Period) -> QlResult<Time> {
        let observed = self.observed(date - obs_lag)?;
        let Some(day_counter) = self.term.day_counter() else {
            crate::fail!("day counter not provided for this volatility surface");
        };
        Ok(day_counter.year_fraction(self.base_date()?, observed))
    }

    /// The checks C++ runs before every volatility query (`checkRange(Date...)`,
    /// `.cpp:67-78`), on an already observed date: not before the base date,
    /// and - unless extrapolation is enabled - not past `max_date` nor struck
    /// outside `[min_strike, max_strike]`.
    pub fn check_range(
        &self,
        date: Date,
        strike: Rate,
        min_strike: Rate,
        max_strike: Rate,
        max_date: Date,
    ) -> QlResult<()> {
        let base_date = self.base_date()?;
        require!(
            date >= base_date,
            "date ({date}) is before base date ({base_date})"
        );
        require!(
            self.term.allows_extrapolation() || date <= max_date,
            "date ({date}) is past max curve date ({max_date})"
        );
        require!(
            self.term.allows_extrapolation() || (strike >= min_strike && strike <= max_strike),
            "strike ({strike}) is outside the curve domain [{min_strike},{max_strike}] at date = \
             {date}"
        );
        Ok(())
    }
}
