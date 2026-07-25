//! Caplet-volatility bootstrapping stripper (`OptionletStripper1`).
//!
//! Port of `ql/termstructures/volatility/optionlet/optionletstripper1.{hpp,cpp}`:
//! `class OptionletStripper1 : public OptionletStripper`. It strips a
//! [`CapFloorTermVolSurface`] into a grid of optionlet (caplet/floorlet)
//! volatilities by pricing, for each cap/floor length and strike, two adjacent
//! caps and differencing their prices into a single optionlet price, then
//! inverting [`black_formula_implied_std_dev`] for the optionlet standard
//! deviation (`optionletstripper1.cpp:61-177`).
//!
//! The algorithm fills the [`OptionletStripper`] base caches and implements the
//! [`StrippedOptionletBase`] interface the interpolated optionlet surface (#576)
//! reads through. Every accessor routes through a [`LazyObject`]-backed
//! [`calculate`](OptionletStripper1::calculate), so a bumped surface quote or a
//! relinked index re-strips on the next query.
//!
//! ## Divergences from QuantLib
//!
//! - ShiftedLognormal only. The `Normal` path prices through
//!   `BachelierCapFloorEngine` (deferred #440), so it is rejected with a
//!   documented error rather than stripped (#577).
//! - The explicit `switchStrike` constructor parameter is not ported: the switch
//!   strike is always the floating mean of the at-the-money optionlet rates
//!   (`optionletstripper1.cpp:86-92`). The fixed-strike form is deferred to #577.
//! - The `dontThrow` mode is not ported: a caplet that fails to bootstrap
//!   propagates the error (fail-loud, D4) instead of writing a zero standard
//!   deviation (#577).
//! - The warm-restart guess matrix seeds to `0.14`, the C++ `firstGuess`
//!   (`optionletstripper1.cpp:57`), and is reused across recalculations exactly
//!   as C++ warm-starts each cell off its previous solve.
//! - The legacy `capletVols_`/`capFloorPrices_`/`capFloorVolatilities_`/
//!   `optionletPrices_` result accessors are omitted: they are not part of the
//!   [`StrippedOptionletBase`] interface and are unused by #576.

use std::cell::{Cell, RefCell};

use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::Event;
use crate::fail;
use crate::handle::Handle;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::indexes::{IborIndex, Index};
use crate::instrument::Instrument;
use crate::instruments::{CapFloorType, MakeCapFloor};
use crate::option::OptionType;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observer};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::{BlackCapFloorEngine, black_formula_implied_std_dev};
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::TermStructure;
use crate::termstructures::volatility::capfloor::CapFloorTermVolatilityStructure;
use crate::termstructures::volatility::{CapFloorTermVolSurface, VolatilityType};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Natural, Rate, Real, Time, Volatility};

use super::{OptionletStripper, StrippedOptionletBase};

/// The `firstGuess` seed for the shifted-lognormal solve
/// (`optionletstripper1.cpp:57`).
const FIRST_GUESS: Real = 0.14;

/// Invalidates the stripper's lazy state when the surface or the index changes,
/// so the next [`calculate`](OptionletStripper1::calculate) re-strips.
struct StripperUpdater {
    lazy: SharedMut<LazyObject>,
}

impl Observer for StripperUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
    }
}

/// Caplet-volatility bootstrapping stripper.
pub struct OptionletStripper1 {
    base: OptionletStripper,
    accuracy: Real,
    max_iter: Natural,
    switch_strike: Cell<Rate>,
    optionlet_std_devs: RefCell<Vec<Vec<Real>>>,
    lazy: SharedMut<LazyObject>,
    _updater: SharedMut<StripperUpdater>,
}

impl OptionletStripper1 {
    /// Builds a stripper over `term_vol_surface` and `ibor_index`.
    ///
    /// `discount` is the discount curve caps are priced on (empty defaults to the
    /// index forwarding curve, `optionletstripper1.cpp:94-97`). `accuracy` and
    /// `max_iter` size the implied-standard-deviation solve; `volatility_type`
    /// must be [`ShiftedLognormal`](VolatilityType::ShiftedLognormal) and
    /// `displacement` its lognormal shift. `optionlet_frequency` overrides the
    /// index tenor as the optionlet step when set.
    ///
    /// # Errors
    ///
    /// Propagates the [`OptionletStripper`] base construction (which rejects a
    /// displacement under the normal model, an empty or too-short surface).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        term_vol_surface: Shared<CapFloorTermVolSurface>,
        ibor_index: Shared<IborIndex>,
        discount: Handle<dyn YieldTermStructure>,
        accuracy: Real,
        max_iter: Natural,
        volatility_type: VolatilityType,
        displacement: Real,
        optionlet_frequency: Option<Period>,
    ) -> QlResult<OptionletStripper1> {
        let base = OptionletStripper::new(
            Shared::clone(&term_vol_surface),
            Shared::clone(&ibor_index),
            discount,
            volatility_type,
            displacement,
            optionlet_frequency,
        )?;

        let n_optionlet = base.optionlet_maturities();
        let n_strikes = base.n_strikes();

        let lazy = shared_mut(LazyObject::new(true));
        let updater = shared_mut(StripperUpdater {
            lazy: SharedMut::clone(&lazy),
        });
        let observer = SharedMut::clone(&updater) as SharedMut<dyn Observer>;
        term_vol_surface.observable().register_observer(&observer);
        ibor_index.observable().register_observer(&observer);

        Ok(OptionletStripper1 {
            base,
            accuracy,
            max_iter,
            switch_strike: Cell::new(0.0),
            optionlet_std_devs: RefCell::new(vec![vec![FIRST_GUESS; n_strikes]; n_optionlet]),
            lazy,
            _updater: updater,
        })
    }

    /// The floating switch strike (mean at-the-money optionlet rate), computed on
    /// demand (`optionletstripper1.cpp:199-204`).
    pub fn switch_strike(&self) -> QlResult<Rate> {
        self.calculate()?;
        Ok(self.switch_strike.get())
    }

    /// Re-strips the surface if a quote or the index has changed since the last
    /// run, mirroring the C++ `LazyObject::calculate` that guards
    /// `performCalculations`.
    pub fn calculate(&self) -> QlResult<()> {
        if !self.lazy.borrow_mut().start_calculation() {
            return Ok(());
        }
        let result = self.perform_calculations();
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    fn perform_calculations(&self) -> QlResult<()> {
        if self.base.volatility_type() == VolatilityType::Normal {
            fail!(
                "normal (Bachelier) optionlet stripping needs BachelierCapFloorEngine, \
                 deferred to #440/#577"
            );
        }

        let surface = Shared::clone(self.base.term_vol_surface());
        let index = Shared::clone(self.base.ibor_index());
        let settings = index.base().settings().clone();
        let displacement = self.base.displacement();
        let n_optionlet = self.base.optionlet_maturities();
        let n_strikes = self.base.n_strikes();
        let cap_floor_lengths = self.base.cap_floor_lengths().to_vec();
        let strikes = surface.strikes().to_vec();

        let Some(day_counter) = surface.day_counter() else {
            fail!("cap/floor term vol surface has no day counter");
        };

        let mut optionlet_dates = vec![Date::null(); n_optionlet];
        let mut optionlet_payment_dates = vec![Date::null(); n_optionlet];
        let mut optionlet_accrual_periods = vec![0.0; n_optionlet];
        let mut optionlet_times = vec![0.0; n_optionlet];
        let mut atm_optionlet_rate = vec![0.0; n_optionlet];

        let dummy_engine = shared_mut(BlackCapFloorEngine::with_flat_vol(
            index.forwarding_term_structure().clone(),
            Handle::new(shared(SimpleQuote::new(Some(0.20))) as Shared<dyn Quote>),
            day_counter.clone(),
            0.0,
            Shared::clone(&settings),
        )?) as SharedMut<dyn PricingEngine>;

        for i in 0..n_optionlet {
            let cap = MakeCapFloor::new(
                CapFloorType::Cap,
                cap_floor_lengths[i],
                Shared::clone(&index),
                0.04,
                Period::new(0, TimeUnit::Days),
                Shared::clone(&settings),
            )
            .with_pricing_engine(SharedMut::clone(&dummy_engine))
            .build()?;
            let Some(coupon) = cap.last_floating_rate_coupon() else {
                fail!(
                    "cap for optionlet tenor {} has no floating coupon",
                    cap_floor_lengths[i]
                );
            };
            optionlet_dates[i] = coupon.fixing_date();
            optionlet_payment_dates[i] = coupon.date();
            optionlet_accrual_periods[i] = coupon.accrual_period();
            optionlet_times[i] = surface.time_from_reference(optionlet_dates[i])?;
            atm_optionlet_rate[i] = coupon.index_fixing()?;
        }

        let switch_strike = atm_optionlet_rate.iter().sum::<Rate>() / n_optionlet as Real;
        self.switch_strike.set(switch_strike);

        let discount_handle = if self.base.discount().is_empty() {
            index.forwarding_term_structure().clone()
        } else {
            self.base.discount().clone()
        };
        let discount_curve = discount_handle.current_link()?;

        let vol_quote = shared(SimpleQuote::new(Some(0.20)));
        let engine = shared_mut(BlackCapFloorEngine::with_flat_vol(
            discount_handle,
            Handle::new(Shared::clone(&vol_quote) as Shared<dyn Quote>),
            day_counter,
            displacement,
            Shared::clone(&settings),
        )?) as SharedMut<dyn PricingEngine>;

        let mut optionlet_volatilities = vec![vec![0.0; n_strikes]; n_optionlet];
        let mut std_devs = self.optionlet_std_devs.borrow_mut();

        for j in 0..n_strikes {
            let below_switch = strikes[j] < switch_strike;
            let cap_floor_type = if below_switch {
                CapFloorType::Floor
            } else {
                CapFloorType::Cap
            };
            let optionlet_type = if below_switch {
                OptionType::Put
            } else {
                OptionType::Call
            };

            let mut previous_price = 0.0;
            for i in 0..n_optionlet {
                let vol = surface.volatility_tenor(cap_floor_lengths[i], strikes[j], true)?;
                vol_quote.set_value(Some(vol));
                let mut cap = MakeCapFloor::new(
                    cap_floor_type,
                    cap_floor_lengths[i],
                    Shared::clone(&index),
                    strikes[j],
                    Period::new(0, TimeUnit::Days),
                    Shared::clone(&settings),
                )
                .with_pricing_engine(SharedMut::clone(&engine))
                .build()?;
                let cap_price = cap.npv()?;
                let optionlet_price = cap_price - previous_price;
                previous_price = cap_price;

                let discount_factor =
                    discount_curve.discount_date(optionlet_payment_dates[i], false)?;
                let annuity = optionlet_accrual_periods[i] * discount_factor;
                let std_dev = black_formula_implied_std_dev(
                    optionlet_type,
                    strikes[j],
                    atm_optionlet_rate[i],
                    optionlet_price,
                    annuity,
                    displacement,
                    std_devs[i][j],
                    self.accuracy,
                    self.max_iter,
                )?;
                std_devs[i][j] = std_dev;
                optionlet_volatilities[i][j] = std_dev / optionlet_times[i].sqrt();
            }
        }
        drop(std_devs);

        let mut caches = self.base.caches().borrow_mut();
        caches.optionlet_dates = optionlet_dates;
        caches.optionlet_payment_dates = optionlet_payment_dates;
        caches.optionlet_accrual_periods = optionlet_accrual_periods;
        caches.optionlet_times = optionlet_times;
        caches.atm_optionlet_rate = atm_optionlet_rate;
        caches.optionlet_volatilities = optionlet_volatilities;
        Ok(())
    }
}

impl StrippedOptionletBase for OptionletStripper1 {
    fn optionlet_strikes(&self, i: usize) -> QlResult<Vec<Rate>> {
        self.calculate()?;
        Ok(self.base.caches().borrow().optionlet_strikes[i].clone())
    }

    fn optionlet_volatilities(&self, i: usize) -> QlResult<Vec<Volatility>> {
        self.calculate()?;
        Ok(self.base.caches().borrow().optionlet_volatilities[i].clone())
    }

    fn optionlet_fixing_dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.base.caches().borrow().optionlet_dates.clone())
    }

    fn optionlet_fixing_times(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self.base.caches().borrow().optionlet_times.clone())
    }

    fn optionlet_maturities(&self) -> usize {
        self.base.optionlet_maturities()
    }

    fn atm_optionlet_rates(&self) -> QlResult<Vec<Rate>> {
        self.calculate()?;
        Ok(self.base.caches().borrow().atm_optionlet_rate.clone())
    }

    fn day_counter(&self) -> Option<DayCounter> {
        self.base.day_counter()
    }

    fn calendar(&self) -> Option<Calendar> {
        self.base.calendar()
    }

    fn settlement_days(&self) -> QlResult<Natural> {
        self.base.settlement_days()
    }

    fn business_day_convention(&self) -> BusinessDayConvention {
        self.base.business_day_convention()
    }

    fn volatility_type(&self) -> VolatilityType {
        self.base.volatility_type()
    }

    fn displacement(&self) -> Real {
        self.base.displacement()
    }
}
