//! Year-on-year inflation optionlet stripping.
//!
//! Port of `ql/experimental/inflation/yoyoptionletstripper.hpp` (the
//! [`YoYOptionletStripper`] interface, `hpp:37-60`) and
//! `interpolatedyoyoptionletstripper.hpp` (the interpolated implementation,
//! `hpp:43-298`): from a [`YoYCapFloorTermPriceSurface`] of quoted cap/floor
//! prices, strip one [`PiecewiseYoYOptionletVolatilityCurve`] per strike, so
//! that [`slice`](YoYOptionletStripper::slice) can answer the K-profile of
//! optionlet volatilities at any date.
//!
//! ## The engine repointing divergence
//!
//! C++ `initialize` takes the shared `YoYInflationCapFloorEngine` and the
//! solver's objective calls `p_->setVolatility(hCurve)` on *every* evaluation
//! (`interpolatedyoyoptionletstripper.hpp:154-155`), overwriting the engine's
//! handle member. The Rust engine's handle is immutable, so the caller hands
//! `initialize` the *retained* [`RelinkableHandle`] the engine was built over,
//! and the objective re-links it to each freshly built
//! [`InterpolatedYoYOptionletVolatilityCurve`] instead - which notifies the
//! engine and, through it, invalidates the cap/floor being repriced. The
//! bootstrap phase then shares the same link through each
//! [`YoYOptionletHelper`], whose `set_term_structure` re-points it at the
//! curve under construction.
//!
//! ## Fidelity pins (ported faithfully, not fixed)
//!
//! - The objective's volatility curves are built with
//!   `indexIsInterpolated = false`: the C++ `ObjectiveFunction` declares the
//!   member with that default and no constructor ever assigns it (`hpp:81`,
//!   `:97-136`), even though the surface's own flag feeds everything else.
//! - The `fixingDays` the C++ `ObjectiveFunction` takes (`hpp:71`, `:102`) is
//!   never stored or read; the port's objective simply takes none. The
//!   *helpers* do read the surface's fixing days (`hpp:239`), and so do they
//!   here.
//! - Two lags travel separately: the `lag` parameter goes to
//!   `MakeYoYInflationCapFloor` (`hpp:116`) while the member `lag_` is
//!   overwritten from `surf_->observationLag()` (`hpp:112`) and feeds the vol
//!   curves. `initialize` passes its own `lag_` - also the surface's
//!   observation lag - so the two coincide in every reachable call, and the
//!   port keeps both locals.
//! - The objective's curve hardcodes `TARGET`, `ModifiedFollowing` and
//!   `Actual/365 (Fixed)` (`hpp:149-150`), not the surface's conventions; and
//!   each helper is handed a throwaway flat surface at the solved volatility
//!   (`hpp:243-254`) that the bootstrap immediately replaces.
//!
//! [`YoYCapFloorTermPriceSurface`]: crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurface

use std::cell::RefCell;
use std::marker::PhantomData;

use crate::currency::Currency;
use crate::errors::{QlError, QlResult};
use crate::handle::{Handle, RelinkableHandle};
use crate::indexes::inflation::YyGenericCpi;
use crate::indexes::inflationindex::CpiInterpolationType;
use crate::instrument::Instrument;
use crate::instruments::{CapFloorType, MakeYoYInflationCapFloor};
use crate::math::interpolations::Interpolator;
use crate::math::interpolations::linear::Linear;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::inflation::YoYInflationCapFloorEngine;
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::{Shared, SharedMut, shared};
use crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurface;
use crate::time::calendars::target::Target;
use crate::time::date::Date;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real, Size, Volatility};
use crate::{fail, require};

use super::yoyoptionlethelpers::{YoYOptionletHelper, YoYOptionletVolatilityHelper};
use super::{
    ConstantYoYOptionletVolatility, InterpolatedYoYOptionletVolatilityCurve,
    PiecewiseYoYOptionletVolatilityCurve, YoYOptionletVolatilitySurface,
};

/// Interface for inflation cap stripping from price surfaces
/// (`YoYOptionletStripper`, `yoyoptionletstripper.hpp:37-60`). Strippers
/// return K slices of the volatility surface at a given T; `initialize` does
/// the actual stripping along each K.
pub trait YoYOptionletStripper {
    /// Strips `surface` with `pricer`, whose volatility must be read through
    /// the retained `vol_handle` (see the module docs); `slope` is the assumed
    /// proportional change of the unobserved initial caplet volatility.
    ///
    /// # Errors
    ///
    /// A strike whose initial-point solve fails (C++'s `QL_FAIL` wrap,
    /// `interpolatedyoyoptionletstripper.hpp:214-216`), an unbuildable
    /// instrument or helper, or a failed per-strike bootstrap.
    fn initialize(
        &self,
        surface: &Shared<dyn YoYCapFloorTermPriceSurface>,
        pricer: &SharedMut<YoYInflationCapFloorEngine>,
        vol_handle: &RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
        slope: Real,
    ) -> QlResult<()>;

    /// The lowest quoted strike (`minStrike`).
    ///
    /// # Errors
    ///
    /// Before `initialize` has run, where C++ dereferences null.
    fn min_strike(&self) -> QlResult<Rate>;

    /// The highest quoted strike (`maxStrike`).
    ///
    /// # Errors
    ///
    /// As [`min_strike`](Self::min_strike).
    fn max_strike(&self) -> QlResult<Rate>;

    /// The quoted strike union (`strikes`).
    ///
    /// # Errors
    ///
    /// As [`min_strike`](Self::min_strike).
    fn strikes(&self) -> QlResult<Vec<Rate>>;

    /// The (strikes, volatilities) profile at `d` (`slice`), one entry per
    /// quoted strike, each read off that strike's stripped curve.
    ///
    /// # Errors
    ///
    /// As [`min_strike`](Self::min_strike), plus a date the stripped curves
    /// refuse.
    fn slice(&self, d: Date) -> QlResult<(Vec<Rate>, Vec<Volatility>)>;
}

/// What `initialize` stores (the C++ mutable members,
/// `yoyoptionletstripper.hpp:53-59` and
/// `interpolatedyoyoptionletstripper.hpp:61`).
struct StripperState {
    surface: Shared<dyn YoYCapFloorTermPriceSurface>,
    lag: Period,
    vol_curves: Vec<Shared<dyn YoYOptionletVolatilitySurface>>,
}

/// The interpolated stripper (`InterpolatedYoYOptionletStripper`,
/// `interpolatedyoyoptionletstripper.hpp:43-91`): interpolates along each K,
/// as opposed to fitting a model.
pub struct InterpolatedYoYOptionletStripper<I: Interpolator> {
    state: RefCell<Option<StripperState>>,
    _interpolator: PhantomData<I>,
}

impl InterpolatedYoYOptionletStripper<Linear> {
    /// An uninitialised stripper; the work happens in
    /// [`initialize`](YoYOptionletStripper::initialize).
    pub fn new() -> InterpolatedYoYOptionletStripper<Linear> {
        InterpolatedYoYOptionletStripper {
            state: RefCell::new(None),
            _interpolator: PhantomData,
        }
    }
}

impl Default for InterpolatedYoYOptionletStripper<Linear> {
    fn default() -> Self {
        InterpolatedYoYOptionletStripper::new()
    }
}

impl<I: Interpolator> InterpolatedYoYOptionletStripper<I> {
    fn with_state<R>(&self, f: impl FnOnce(&StripperState) -> QlResult<R>) -> QlResult<R> {
        match self.state.borrow().as_ref() {
            Some(state) => f(state),
            None => fail!("stripper not initialized: no price surface set"),
        }
    }
}

impl YoYOptionletStripper for InterpolatedYoYOptionletStripper<Linear> {
    /// The stripping (`interpolatedyoyoptionletstripper.hpp:161-275`): per
    /// quoted strike, Brent-solve the initial caplet volatility so the
    /// shortest quoted cap/floor reprices, then bootstrap a
    /// [`PiecewiseYoYOptionletVolatilityCurve`] through that strike's whole
    /// maturity column.
    fn initialize(
        &self,
        surface: &Shared<dyn YoYCapFloorTermPriceSurface>,
        pricer: &SharedMut<YoYInflationCapFloorEngine>,
        vol_handle: &RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
        slope: Real,
    ) -> QlResult<()> {
        let lag = surface.observation_lag();
        let frequency = surface.frequency();
        let index_is_interpolated = surface.index_is_interpolated();
        let fixing_days = surface.fixing_days();
        let settlement_days = 0;
        let Some(calendar) = surface.calendar() else {
            fail!("the price surface holds a calendar by construction");
        };
        let bdc = surface.business_day_convention();
        let day_counter = surface.require_day_counter()?;
        let settings = Shared::clone(surface.surface_base().settings());
        let reference = surface.reference_date()?;

        // Switch from floors to caps when out of floors (`hpp:178-180`).
        let max_floor = *surface
            .floor_strikes()
            .last()
            .expect("the surface carries floor strikes");
        let mut use_type = CapFloorType::Floor;
        let tp_min = surface.maturities()[0];

        // The "fake index" over the surface's own year-on-year curve
        // (`hpp:182-189`); C++ hands it a default-constructed `Currency()`.
        let h_yoy = Handle::new(surface.yoy_ts()?);
        let an_index = shared(
            YyGenericCpi::new(
                frequency,
                false,
                lag,
                Currency::new("", "", 0, "", "", 0),
                Shared::clone(&settings),
            )
            .with_term_structure(h_yoy),
        );

        // The shortest quoted maturity in whole years, shared by the
        // objective's instrument (`hpp:115`) and its sanity gate (`hpp:129-132`).
        let n = (surface.time_from_reference(surface.min_maturity()?)? + 0.5).floor() as Size;
        require!(n > 0, "first maturity in price surface not > 0: {n}");

        // The two seed nodes of every objective curve (`hpp:120-127`): the
        // surface's base date and one week past its shortest maturity.
        let objective_lag = surface.observation_lag();
        let d0 = surface.base_date()?;
        let d1 = surface.min_maturity()? + Period::new(7, TimeUnit::Days);
        let t0 = day_counter.year_fraction(reference, d0);
        let t1 = day_counter.year_fraction(reference, d1);

        let mut vol_curves: Vec<Shared<dyn YoYOptionletVolatilitySurface>> = Vec::new();
        for &k in surface.strikes() {
            if k > max_floor {
                use_type = CapFloorType::Cap;
            }

            let solver_tolerance = 1e-7;
            let (lo, hi) = (0.00001, 0.08);
            let guess = (hi + lo) / 2.0;
            let price_to_match = match use_type {
                CapFloorType::Cap => surface.cap_price_by_tenor(tp_min, k)?,
                _ => surface.floor_price_by_tenor(tp_min, k)?,
            };

            // The objective's cap/floor, built once per strike (`hpp:113-118`,
            // `:134`): flat interpolation, a 10,000 nominal, the shared pricer.
            let mut capfloor = MakeYoYInflationCapFloor::new(
                use_type,
                Shared::clone(&an_index),
                n,
                calendar.clone(),
                lag,
                CpiInterpolationType::Flat,
                Shared::clone(&settings),
            )
            .with_nominal(10_000.0)
            .with_strike(k)
            .build()?;
            capfloor
                .base_mut()
                .set_pricing_engine(SharedMut::clone(pricer) as SharedMut<dyn PricingEngine>);

            // The objective (`hpp:139-158`): seed a two-node curve from the
            // guess and the slope, re-point the engine's link at it, reprice.
            let error_slot: RefCell<Option<QlError>> = RefCell::new(None);
            let mut evaluate = |guess: Volatility| -> QlResult<Real> {
                let v1 = guess;
                let v0 = guess - slope * (t1 - t0) * guess;
                let curve = InterpolatedYoYOptionletVolatilityCurve::new(
                    0,
                    Target::new(),
                    crate::time::businessdayconvention::BusinessDayConvention::ModifiedFollowing,
                    Actual365Fixed::new(),
                    objective_lag,
                    frequency,
                    false,
                    vec![d0, d1],
                    vec![v0, v1],
                    -1.0,
                    3.0,
                    Linear,
                    Shared::clone(&settings),
                )?;
                vol_handle.link_to(shared(curve) as Shared<dyn YoYOptionletVolatilitySurface>);
                Ok(price_to_match - capfloor.npv()?)
            };
            let objective = |guess: Volatility| -> Real {
                match evaluate(guess) {
                    Ok(value) => value,
                    Err(error) => {
                        *error_slot.borrow_mut() = Some(error);
                        Real::NAN
                    }
                }
            };
            let found =
                match Brent::new().solve_bracketed(objective, solver_tolerance, guess, lo, hi) {
                    Ok(found) => found,
                    Err(solver_error) => {
                        let message = match error_slot.into_inner() {
                            Some(inner) => inner.message().to_string(),
                            None => solver_error.message().to_string(),
                        };
                        fail!("failed to find solution here because: {message}");
                    }
                };

            // One helper per quoted maturity, working in bps (`hpp:218-256`),
            // each handed a throwaway flat surface at the solved volatility.
            let notional = 10_000.0;
            let mut helper_instruments: Vec<Shared<dyn YoYOptionletVolatilityHelper>> = Vec::new();
            for &tp in surface.maturities() {
                let next_price = match use_type {
                    CapFloorType::Cap => surface.cap_price_by_tenor(tp, k)?,
                    _ => surface.floor_price_by_tenor(tp, k)?,
                };
                let quote =
                    Handle::new(shared(SimpleQuote::new(Some(next_price))) as Shared<dyn Quote>);
                // An integer number of periods away, enforced by rounding
                // (`hpp:232-234`).
                let n_t = (surface.time_from_reference(surface.yoy_option_date_from_tenor(tp)?)?
                    + 0.5)
                    .floor() as Size;
                let helper = YoYOptionletHelper::new(
                    quote,
                    notional,
                    use_type,
                    lag,
                    day_counter.clone(),
                    calendar.clone(),
                    fixing_days,
                    &an_index,
                    CpiInterpolationType::Flat,
                    k,
                    n_t,
                    SharedMut::clone(pricer),
                    vol_handle.clone(),
                    Shared::clone(&settings),
                )?;
                let yoy_vol_black = shared(ConstantYoYOptionletVolatility::new(
                    found,
                    settlement_days,
                    calendar.clone(),
                    bdc,
                    day_counter.clone(),
                    lag,
                    frequency,
                    false,
                    -1.0,
                    3.0,
                    Shared::clone(&settings),
                )) as Shared<dyn YoYOptionletVolatilitySurface>;
                YoYOptionletVolatilityHelper::set_term_structure(helper.as_ref(), &yoy_vol_black);
                helper_instruments.push(helper as Shared<dyn YoYOptionletVolatilityHelper>);
            }

            // The artificial vol at zero so the first section works
            // (`hpp:257-273`), and a strike band of `max(K, 0.02) / 1000`
            // around the quoted strike.
            let t_min = surface.time_from_reference(surface.yoy_option_date_from_tenor(tp_min)?)?;
            let base_yoy_volatility = found - slope * t_min * found;
            let eps = k.max(0.02) / 1000.0;
            let curve = PiecewiseYoYOptionletVolatilityCurve::new(
                settlement_days,
                calendar.clone(),
                bdc,
                day_counter.clone(),
                lag,
                frequency,
                index_is_interpolated,
                k - eps,
                k + eps,
                base_yoy_volatility,
                helper_instruments,
                Shared::clone(&settings),
            )?;
            curve.calculate()?;
            vol_curves.push(curve as Shared<dyn YoYOptionletVolatilitySurface>);
        }

        *self.state.borrow_mut() = Some(StripperState {
            surface: Shared::clone(surface),
            lag,
            vol_curves,
        });
        Ok(())
    }

    fn min_strike(&self) -> QlResult<Rate> {
        self.with_state(|state| Ok(state.surface.strikes()[0]))
    }

    fn max_strike(&self) -> QlResult<Rate> {
        self.with_state(|state| {
            Ok(*state
                .surface
                .strikes()
                .last()
                .expect("the surface carries strikes"))
        })
    }

    fn strikes(&self) -> QlResult<Vec<Rate>> {
        self.with_state(|state| Ok(state.surface.strikes().to_vec()))
    }

    /// `hpp:278-298`. C++ reads each curve with its defaulted observation
    /// lag, which the curve substitutes with its own - the surface's - so the
    /// port passes that lag explicitly.
    fn slice(&self, d: Date) -> QlResult<(Vec<Rate>, Vec<Volatility>)> {
        self.with_state(|state| {
            let strikes = state.surface.strikes();
            let mut volatilities = Vec::with_capacity(strikes.len());
            for (curve, &k) in state.vol_curves.iter().zip(strikes) {
                volatilities.push(curve.volatility(d, k, state.lag)?);
            }
            Ok((strikes.to_vec(), volatilities))
        })
    }
}
