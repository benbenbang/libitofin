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

#[cfg(test)]
mod yoy_price_surface_to_vol_oracle {
    //! `test-suite/inflationvolatility.cpp`'s `testYoYPriceSurfaceToVol`
    //! (`:271-352`), the numeric oracle closing the whole #874 stack: the EU
    //! fixture of `setup()` (`:91-240`) and `setupPriceSurface()` (`:243-268`)
    //! built into the price surface, stripped through
    //! `InterpolatedYoYOptionletStripper<Linear>` under the unit-displaced
    //! Black engine at slope -0.5, and the K-surface's `Dslice` pinned against
    //! `volATyear1[]`/`volATyear3[]` to `eps = 1e-4` (`:320-335`).
    //!
    //! Unlike the sibling ATM oracle (`yoycapfloortermpricesurface.rs`), this
    //! test needs `setup()`'s `yoyEU` curve (`:164-190`) linked to the ratio
    //! index: the engine forecasts each optionlet's forward off the *index's
    //! own* year-on-year curve, while the stripper's generic index reads the
    //! price surface's bootstrapped one.
    //!
    //! The upstream headers carry a stale `\bug Tests currently fail` note;
    //! the C++ test was built and run against this tree on 2026-08-25 and
    //! passes clean, so the header literals are the live target.

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::inflation::EuHicp;
    use crate::indexes::inflationindex::{
        CpiInterpolationType, YoYInflationIndex, inflation_period,
    };
    use crate::math::interpolations::cubic::Cubic;
    use crate::math::matrix::Matrix;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
    use crate::termstructures::inflation::interpolatedyoyinflationcurve::InterpolatedYoYInflationCurve;
    use crate::termstructures::inflation::yoycapfloortermpricesurface::InterpolatedYoYCapFloorTermPriceSurface;
    use crate::termstructures::yields::InterpolatedZeroCurve;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month::November;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::types::Integer;

    use super::super::InterpolatedYoYOptionletStripper;

    /// EUR nominal zero times, in years of 365 days (`:101-105`).
    const TIMES_EUR: [Real; 25] = [
        0.0109589, 0.0684932, 0.263014, 0.317808, 0.567123, 0.816438, 1.06575, 1.31507, 1.56438,
        2.0137, 3.01918, 4.01644, 5.01644, 6.01644, 7.01644, 8.01644, 9.02192, 10.0192, 12.0192,
        15.0247, 20.0301, 25.0356, 30.0329, 40.0384, 50.0466,
    ];

    /// EUR nominal zero rates (`:106-110`).
    const RATES_EUR: [Real; 25] = [
        0.0415600, 0.0426840, 0.0470980, 0.0458506, 0.0449550, 0.0439784, 0.0431887, 0.0426604,
        0.0422925, 0.0424591, 0.0421477, 0.0421853, 0.0424016, 0.0426969, 0.0430804, 0.0435011,
        0.0439368, 0.0443825, 0.0452589, 0.0463389, 0.0472636, 0.0473401, 0.0470629, 0.0461092,
        0.0450794,
    ];

    /// EU year-on-year rates for the index's own curve (`:164-170`); the
    /// first sits in the base period.
    const YOY_EU_RATES: [Real; 31] = [
        0.0237951, 0.0238749, 0.0240334, 0.0241934, 0.0243567, 0.0245323, 0.0247213, 0.0249348,
        0.0251768, 0.0254337, 0.0257258, 0.0260217, 0.0263006, 0.0265538, 0.0267803, 0.0269378,
        0.0270608, 0.0271363, 0.0272, 0.0272512, 0.0272927, 0.027317, 0.0273615, 0.0273811,
        0.0274063, 0.0274307, 0.0274625, 0.027527, 0.0275952, 0.0276734, 0.027794,
    ];

    /// EU cap strikes (`:196`) and floor strikes (`:207`).
    const C_STRIKES_EU: [Real; 6] = [0.02, 0.025, 0.03, 0.035, 0.04, 0.05];
    const F_STRIKES_EU: [Real; 6] = [-0.01, 0.00, 0.005, 0.01, 0.015, 0.02];

    /// EU cap prices by strike (rows) and maturity (columns) (`:199-205`).
    const C_PRICES_EU: [[Real; 7]; 6] = [
        [116.225, 204.945, 296.285, 434.29, 654.47, 844.775, 1132.33],
        [34.305, 71.575, 114.1, 184.33, 307.595, 421.395, 602.35],
        [6.37, 19.085, 35.635, 66.42, 127.69, 189.685, 296.195],
        [1.325, 5.745, 12.585, 26.945, 58.95, 94.08, 158.985],
        [0.501, 2.37, 5.38, 13.065, 31.91, 53.95, 96.97],
        [0.501, 0.695, 1.47, 4.415, 12.86, 23.75, 46.7],
    ];

    /// EU floor prices by strike (rows) and maturity (columns) (`:208-214`).
    const F_PRICES_EU: [[Real; 7]; 6] = [
        [0.501, 0.851, 2.44, 6.645, 16.23, 26.85, 46.365],
        [0.501, 2.236, 5.555, 13.075, 28.46, 44.525, 73.08],
        [1.025, 3.935, 9.095, 19.64, 39.93, 60.375, 96.02],
        [2.465, 7.885, 16.155, 31.6, 59.34, 86.21, 132.045],
        [6.9, 17.92, 32.085, 56.08, 95.95, 132.85, 194.18],
        [23.52, 47.625, 74.085, 114.355, 175.72, 229.565, 316.285],
    ];

    /// The T = 1y and T = 3y constant-time lines (`:320-329`), one entry per
    /// strike of the 11-strike union.
    const VOL_AT_YEAR1: [Real; 11] = [
        0.0129, 0.0094, 0.0083, 0.0073, 0.0064, 0.0058, 0.0042, 0.0046, 0.0053, 0.0064, 0.0098,
    ];
    const VOL_AT_YEAR3: [Real; 11] = [
        0.0080, 0.0058, 0.0051, 0.0045, 0.0040, 0.0035, 0.0026, 0.0028, 0.0033, 0.0040, 0.0061,
    ];

    const EPS: Real = 1e-4;

    fn eval_date() -> Date {
        Date::new(23, November, 2007)
    }

    fn cf_maturities_eu() -> Vec<Period> {
        [3, 5, 7, 10, 15, 20, 30]
            .into_iter()
            .map(|n| Period::new(n, TimeUnit::Years))
            .collect()
    }

    fn matrix_of(rows: &[[Real; 7]; 6]) -> Matrix {
        let mut m = Matrix::with_size(6, 7);
        for (i, row) in rows.iter().enumerate() {
            for (j, &value) in row.iter().enumerate() {
                m[(i, j)] = value;
            }
        }
        m
    }

    /// The EUR nominal curve (`:129-140`), the day fraction truncated as the
    /// C++ casts do.
    fn eur_nominal_curve() -> Handle<dyn YieldTermStructure> {
        let dates: Vec<Date> = TIMES_EUR
            .iter()
            .map(|&t| {
                let ys = t.floor() as i32;
                let ds = ((t - Real::from(ys)) * 365.0) as i32;
                eval_date() + Period::new(ys, TimeUnit::Years) + Period::new(ds, TimeUnit::Days)
            })
            .collect();
        let curve = InterpolatedZeroCurve::<Cubic>::new(
            dates,
            RATES_EUR.to_vec(),
            Actual365Fixed::new(),
            Cubic,
        )
        .expect("25 well-ordered nodes");
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>)
    }

    /// The oracle (`:271-352`). `setup()` links the index to the `yoyEU`
    /// curve, whose base date sits one month back of the evaluation date and
    /// whose thirty yearly nodes run off the 2-month-lagged cap start date
    /// (`:172-190`); `setupPriceSurface()` builds the 3-month-lag price
    /// surface over the 6x7 EU matrices; the stripper then strips it at slope
    /// -0.5 under the unit-displaced engine, whose volatility link starts
    /// empty ("the vol gets set in the stripper ... else no point", `:287`).
    #[test]
    fn d_slice_recovers_the_one_and_three_year_vol_lines() {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(eval_date());

        let yoy_handle = crate::handle::RelinkableHandle::<dyn YoYInflationTermStructure>::empty();
        let index = shared(
            YoYInflationIndex::from_underlying(shared(EuHicp::new(Shared::clone(&settings))))
                .with_term_structure(yoy_handle.handle()),
        );

        let base_date = inflation_period(
            eval_date() - Period::new(1, TimeUnit::Months),
            index.frequency(),
        )
        .unwrap()
        .0;
        let mut dates = vec![base_date];
        let mut rates = vec![YOY_EU_RATES[0]];
        let cap_start = Target::new().advance(
            eval_date(),
            -2 as Integer,
            TimeUnit::Months,
            BusinessDayConvention::ModifiedFollowing,
            false,
        );
        for (i, &rate) in YOY_EU_RATES.iter().enumerate().skip(1) {
            dates.push(Target::new().advance(
                cap_start,
                i as Integer,
                TimeUnit::Years,
                BusinessDayConvention::ModifiedFollowing,
                false,
            ));
            rates.push(rate);
        }
        let yoy_eu = shared(
            InterpolatedYoYInflationCurve::<Linear>::new(
                eval_date(),
                dates,
                rates,
                Frequency::Monthly,
                Actual365Fixed::new(),
                Linear,
                None,
            )
            .expect("31 well-ordered nodes"),
        ) as Shared<dyn YoYInflationTermStructure>;
        yoy_handle.link_to(yoy_eu);

        let price_surface = shared(
            InterpolatedYoYCapFloorTermPriceSurface::new(
                0,
                Period::new(3, TimeUnit::Months),
                Shared::clone(&index),
                CpiInterpolationType::Linear,
                eur_nominal_curve(),
                Actual365Fixed::new(),
                Target::new(),
                BusinessDayConvention::ModifiedFollowing,
                C_STRIKES_EU.to_vec(),
                F_STRIKES_EU.to_vec(),
                cf_maturities_eu(),
                matrix_of(&C_PRICES_EU),
                matrix_of(&F_PRICES_EU),
                Shared::clone(&settings),
            )
            .expect("the EU fixture is consistent"),
        ) as Shared<dyn YoYCapFloorTermPriceSurface>;
        let lag = price_surface.observation_lag();

        let vol_handle = RelinkableHandle::<dyn YoYOptionletVolatilitySurface>::empty();
        let pricer = shared_mut(YoYInflationCapFloorEngine::unit_displaced(
            Shared::clone(&index),
            vol_handle.handle(),
            eur_nominal_curve(),
        ));
        let stripper = shared(InterpolatedYoYOptionletStripper::<Linear>::new())
            as Shared<dyn YoYOptionletStripper>;

        let yoy_surf = KInterpolatedYoYOptionletVolatilitySurface::<Linear>::new(
            0,
            Target::new(),
            BusinessDayConvention::ModifiedFollowing,
            Actual365Fixed::new(),
            lag,
            price_surface,
            &pricer,
            &vol_handle,
            stripper,
            -0.5,
            Shared::clone(&settings),
        )
        .expect("the stripping succeeds");

        let base = yoy_surf.base_date().unwrap();

        let year1 = yoy_surf
            .d_slice(base + Period::new(1, TimeUnit::Years))
            .unwrap();
        assert_eq!(year1.0.len(), 11);
        for (i, (vol, expected)) in year1.1.iter().zip(VOL_AT_YEAR1).enumerate() {
            assert!(
                (vol - expected).abs() < EPS,
                "could not recover 1yr vol at strike {i} ({}): {vol} vs {expected}",
                year1.0[i]
            );
        }

        let year3 = yoy_surf
            .d_slice(base + Period::new(3, TimeUnit::Years))
            .unwrap();
        assert_eq!(year3.0.len(), 11);
        for (i, (vol, expected)) in year3.1.iter().zip(VOL_AT_YEAR3).enumerate() {
            assert!(
                (vol - expected).abs() < EPS,
                "could not recover 3yr vol at strike {i} ({}): {vol} vs {expected}",
                year3.0[i]
            );
        }
    }
}
