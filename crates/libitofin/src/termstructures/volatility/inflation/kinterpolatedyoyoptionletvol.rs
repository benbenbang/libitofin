//! K-interpolated year-on-year optionlet volatility surface.
//!
//! Port of `KInterpolatedYoYOptionletVolatilitySurface`
//! (`ql/experimental/inflation/kinterpolatedyoyoptionletvolatilitysurface.hpp:45-203`):
//! the stripper provides curves in the T direction along each quoted strike,
//! and this surface interpolates *across* those strikes - no model fitting -
//! caching one K-slice per queried date.
//!
//! ## Divergences from QuantLib
//!
//! - The C++ constructor takes the engine and calls the stripper's
//!   `initialize` with it (`hpp:57-58`, `:148-155`); the Rust engine's
//!   volatility handle is immutable, so the constructor also takes the
//!   *retained* [`RelinkableHandle`] the engine reads and forwards both - the
//!   repointing contract of [`YoYOptionletStripper`]'s module docs. The
//!   `performCalculations` the C++ constructor runs (`hpp:121`) is the
//!   `initialize` call itself, so it runs here too and construction is
//!   fallible.
//! - The stored `yoyInflationCouponPricer_` and `slope_` members serve only
//!   that constructor-run calculation, so they are not kept.
//! - The `VolatilityType`/`displacement` pair is omitted unread, as on every
//!   surface in this module; the moving reference date takes the shared
//!   [`Settings`] handle (D5).
//!
//! [`YoYCapFloorTermPriceSurface`]: crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurface

use std::cell::RefCell;

use crate::errors::QlResult;
use crate::handle::RelinkableHandle;
use crate::indexes::inflationindex::InflationIndex;
use crate::math::interpolations::linear::Linear;
use crate::math::interpolations::{Interpolation, Interpolator};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengines::inflation::YoYInflationCapFloorEngine;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurface;
use crate::termstructures::volatility::VolatilityTermStructure;
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::interpolatedyoyoptionletvol::extended_value;
use super::{
    YoYOptionletStripper, YoYOptionletVolatilitySurface, YoYOptionletVolatilitySurfaceBase,
};

/// One cached K-slice: the (strikes, volatilities) profile at `date` and the
/// interpolation over it (the C++ mutable members `lastDate_`, `slice_` and
/// `tempKinterpolation_`, `hpp:83-86`).
struct SliceCache<I: Interpolator> {
    date: Date,
    slice: (Vec<Rate>, Vec<Volatility>),
    interpolation: I::Output,
}

/// K-interpolated year-on-year optionlet volatility surface
/// (`hpp:45-89`).
pub struct KInterpolatedYoYOptionletVolatilitySurface<I: Interpolator> {
    vol_base: YoYOptionletVolatilitySurfaceBase,
    cap_floor_prices: Shared<dyn YoYCapFloorTermPriceSurface>,
    stripper: Shared<dyn YoYOptionletStripper>,
    interpolator: I,
    slice_cache: RefCell<Option<SliceCache<I>>>,
}

impl KInterpolatedYoYOptionletVolatilitySurface<Linear> {
    /// Builds the surface and runs the stripping (`hpp:94-122`): the
    /// frequency comes off the price surface's index (`hpp:114`), the index
    /// is taken as not interpolated (`hpp:116`), and `stripper.initialize` -
    /// C++'s constructor-run `performCalculations` - strips `cap_floor_prices`
    /// with `pricer` at `slope`. `vol_handle` must be the link `pricer` reads
    /// its volatility through; see the module docs.
    ///
    /// # Errors
    ///
    /// As [`YoYOptionletStripper::initialize`].
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        calendar: Calendar,
        business_day_convention: BusinessDayConvention,
        day_counter: DayCounter,
        observation_lag: Period,
        cap_floor_prices: Shared<dyn YoYCapFloorTermPriceSurface>,
        pricer: &SharedMut<YoYInflationCapFloorEngine>,
        vol_handle: &RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
        stripper: Shared<dyn YoYOptionletStripper>,
        slope: Real,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<KInterpolatedYoYOptionletVolatilitySurface<Linear>> {
        let frequency = cap_floor_prices.yoy_index().frequency();
        let vol_base = YoYOptionletVolatilitySurfaceBase::new(
            settlement_days,
            calendar,
            business_day_convention,
            day_counter,
            observation_lag,
            frequency,
            false,
            settings,
        );
        stripper.initialize(&cap_floor_prices, pricer, vol_handle, slope)?;
        Ok(KInterpolatedYoYOptionletVolatilitySurface {
            vol_base,
            cap_floor_prices,
            stripper,
            interpolator: Linear,
            slice_cache: RefCell::new(None),
        })
    }
}

impl<I: Interpolator> KInterpolatedYoYOptionletVolatilitySurface<I> {
    /// The (strikes, volatilities) profile at `d` (`Dslice`, `hpp:169-175`):
    /// the stripper's slice, cached per date.
    ///
    /// # Errors
    ///
    /// As [`YoYOptionletStripper::slice`].
    pub fn d_slice(&self, d: Date) -> QlResult<(Vec<Rate>, Vec<Volatility>)> {
        self.update_slice(d)?;
        Ok(self
            .slice_cache
            .borrow()
            .as_ref()
            .expect("update_slice filled the cache")
            .slice
            .clone())
    }

    /// Refreshes the cached slice and its strike interpolation when `d` is
    /// not the date last asked for (`updateSlice`, `hpp:189-203`).
    fn update_slice(&self, d: Date) -> QlResult<()> {
        let stale = match self.slice_cache.borrow().as_ref() {
            Some(cache) => cache.date != d,
            None => true,
        };
        if stale {
            let slice = self.stripper.slice(d)?;
            let interpolation = self.interpolator.interpolate(&slice.0, &slice.1)?;
            *self.slice_cache.borrow_mut() = Some(SliceCache {
                date: d,
                slice,
                interpolation,
            });
        }
        Ok(())
    }

    /// The date-keyed volatility hook (`volatilityImpl(Date, Rate)`,
    /// `hpp:158-166`): the cached slice's strike interpolation, extended past
    /// the quoted strikes when the surface extrapolates (C++'s
    /// `enableExtrapolation` on `tempKinterpolation_`).
    fn volatility_impl_date(&self, d: Date, strike: Rate) -> QlResult<Volatility> {
        self.update_slice(d)?;
        let cache = self.slice_cache.borrow();
        let interpolation = &cache
            .as_ref()
            .expect("update_slice filled the cache")
            .interpolation;
        if self.allows_extrapolation() {
            extended_value(interpolation, strike)
        } else {
            interpolation.value(strike)
        }
    }

    /// The time-keyed hook (`volatilityImpl(Time, Rate)`, `hpp:178-187`):
    /// C++ reconstructs a date as the reference date advanced by the whole
    /// years and the 365ths of the remainder, then defers to the date-keyed
    /// hook.
    fn volatility_impl(&self, t: Time, strike: Rate) -> QlResult<Volatility> {
        let years = t.floor();
        let days = ((t - years) * 365.0).floor() as i32;
        let d = self.vol_base.term_structure_base().reference_date()?
            + Period::new(years as i32, TimeUnit::Years)
            + Period::new(days, TimeUnit::Days);
        self.volatility_impl_date(d, strike)
    }

    fn last_maturity(&self) -> Period {
        *self
            .cap_floor_prices
            .maturities()
            .last()
            .expect("the price surface carries maturities")
    }
}

impl<I: Interpolator> AsObservable for KInterpolatedYoYOptionletVolatilitySurface<I> {
    fn observable(&self) -> &Observable {
        self.vol_base.term_structure_base().observable()
    }
}

impl<I: Interpolator> TermStructure for KInterpolatedYoYOptionletVolatilitySurface<I> {
    fn base(&self) -> &TermStructureBase {
        self.vol_base.term_structure_base()
    }

    /// The reference date advanced by the last quoted maturity
    /// (`hpp:125-130`); the null date should the reference be unresolvable.
    fn max_date(&self) -> Date {
        match self.vol_base.term_structure_base().reference_date() {
            Ok(reference) => reference + self.last_maturity(),
            Err(_) => Date::null(),
        }
    }
}

impl<I: Interpolator> VolatilityTermStructure for KInterpolatedYoYOptionletVolatilitySurface<I> {
    fn business_day_convention(&self) -> BusinessDayConvention {
        self.vol_base.business_day_convention()
    }

    /// The lowest quoted strike (`hpp:133-137`).
    fn min_strike(&self) -> Rate {
        self.cap_floor_prices.strikes()[0]
    }

    /// The highest quoted strike (`hpp:140-144`).
    fn max_strike(&self) -> Rate {
        *self
            .cap_floor_prices
            .strikes()
            .last()
            .expect("the price surface carries strikes")
    }
}

impl<I: Interpolator> YoYOptionletVolatilitySurface
    for KInterpolatedYoYOptionletVolatilitySurface<I>
{
    fn base_date(&self) -> QlResult<Date> {
        self.vol_base.base_date()
    }

    fn volatility(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Volatility> {
        let observed = self.vol_base.observed(date - obs_lag)?;
        self.vol_base.check_range(
            observed,
            strike,
            self.min_strike(),
            self.max_strike(),
            TermStructure::max_date(self),
        )?;
        let t = TermStructure::time_from_reference(self, observed)?;
        self.volatility_impl(t, strike)
    }

    fn total_variance(&self, date: Date, strike: Rate, obs_lag: Period) -> QlResult<Real> {
        let volatility = self.volatility(date, strike, obs_lag)?;
        Ok(volatility * volatility * self.vol_base.time_from_base(date, obs_lag)?)
    }
}
