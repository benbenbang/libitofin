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
//! Shifted-lognormal and normal inversions share transactional result caches.
//! Optional fixed switch strikes and inversion-only `dont_throw` retain QuantLib
//! behavior. Surface, discount, index and date changes invalidate downstream users.

use std::cell::{Cell, RefCell};

use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::Event;
use crate::fail;
use crate::handle::Handle;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::indexes::{IborIndex, Index, OvernightIndex};
use crate::instrument::Instrument;
use crate::instruments::{CapFloorType, MakeCapFloor};
use crate::option::OptionType;
use crate::patterns::lazyobject::LazyObject;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::{
    BachelierCapFloorEngine, BlackCapFloorEngine, bachelier_black_formula_implied_vol,
    black_formula_implied_std_dev,
};
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
    observable: Shared<Observable>,
}

impl Observer for StripperUpdater {
    fn update(&mut self) {
        self.lazy.borrow_mut().invalidate_silently();
        self.observable.notify_observers();
    }
}

/// Optional fixed switch strike and inversion-failure fallback.
#[derive(Clone, Copy, Debug, Default)]
pub struct OptionletStripperOptions {
    /// `None` recomputes the mean forward rate after each market update.
    pub switch_strike: Option<Rate>,
    /// Replace failed implied-volatility solves with zero, never pricing errors.
    pub dont_throw: bool,
}

/// Caplet-volatility bootstrapping stripper.
pub struct OptionletStripper1 {
    base: OptionletStripper,
    options: OptionletStripperOptions,
    observable: Shared<Observable>,
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
    /// selects the normal or shifted-lognormal model and
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
        Self::new_with_options(
            term_vol_surface,
            ibor_index,
            discount,
            accuracy,
            max_iter,
            volatility_type,
            displacement,
            optionlet_frequency,
            OptionletStripperOptions::default(),
        )
    }

    /// Builds a stripper with explicit switch-strike and inversion fallback controls.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_options(
        term_vol_surface: Shared<CapFloorTermVolSurface>,
        ibor_index: Shared<IborIndex>,
        discount: Handle<dyn YieldTermStructure>,
        accuracy: Real,
        max_iter: Natural,
        volatility_type: VolatilityType,
        displacement: Real,
        optionlet_frequency: Option<Period>,
        options: OptionletStripperOptions,
    ) -> QlResult<Self> {
        if !accuracy.is_finite() || accuracy <= 0.0 || max_iter == 0 {
            fail!("accuracy and maximum iterations must be positive");
        }
        if options.switch_strike.is_some_and(|v| !v.is_finite()) {
            fail!("switch strike must be finite");
        }
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
        let observable = shared(Observable::new());
        let updater = shared_mut(StripperUpdater {
            lazy: SharedMut::clone(&lazy),
            observable: Shared::clone(&observable),
        });
        let observer = SharedMut::clone(&updater) as SharedMut<dyn Observer>;
        term_vol_surface.observable().register_observer(&observer);
        ibor_index.observable().register_observer(&observer);
        base.discount().register_observer(&observer);
        ibor_index
            .base()
            .settings()
            .register_eval_date_observer(&observer);

        Ok(OptionletStripper1 {
            base,
            options,
            observable,
            accuracy,
            max_iter,
            switch_strike: Cell::new(0.0),
            optionlet_std_devs: RefCell::new(vec![vec![FIRST_GUESS; n_strikes]; n_optionlet]),
            lazy,
            _updater: updater,
        })
    }

    /// Overnight-index form; frequency controls the output optionlet grid.
    ///
    /// QuantLib prices the upcast index through vanilla Ibor coupons here;
    /// the frequency does not override the daily tenor of those coupons.
    #[allow(clippy::too_many_arguments)]
    pub fn new_overnight(
        term_vol_surface: Shared<CapFloorTermVolSurface>,
        index: Shared<OvernightIndex>,
        discount: Handle<dyn YieldTermStructure>,
        accuracy: Real,
        max_iter: Natural,
        volatility_type: VolatilityType,
        displacement: Real,
        optionlet_frequency: Option<Period>,
        options: OptionletStripperOptions,
    ) -> QlResult<Self> {
        if optionlet_frequency.is_none() {
            fail!("an optionlet frequency is required for an overnight index");
        }
        Self::new_with_options(
            term_vol_surface,
            index.ibor_index(),
            discount,
            accuracy,
            max_iter,
            volatility_type,
            displacement,
            optionlet_frequency,
            options,
        )
    }

    /// Immutable setup shared with the ATM correction layer.
    pub fn base(&self) -> &OptionletStripper {
        &self.base
    }

    /// Payment dates corresponding to the stripped optionlets.
    pub fn optionlet_payment_dates(&self) -> QlResult<Vec<Date>> {
        self.calculate()?;
        Ok(self.base.caches().borrow().optionlet_payment_dates.clone())
    }

    /// Accrual periods corresponding to the stripped optionlets.
    pub fn optionlet_accrual_periods(&self) -> QlResult<Vec<Time>> {
        self.calculate()?;
        Ok(self
            .base
            .caches()
            .borrow()
            .optionlet_accrual_periods
            .clone())
    }

    /// The floating switch strike (mean at-the-money optionlet rate), computed on
    /// demand (`optionletstripper1.cpp:199-204`).
    pub fn switch_strike(&self) -> QlResult<Rate> {
        if let Some(strike) = self.options.switch_strike {
            return Ok(strike);
        }
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
        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.perform_calculations()))
                .unwrap_or_else(|_| {
                    Err(crate::errors::QlError::new(
                        "optionlet schedule date outside supported range",
                        file!(),
                        line!(),
                    ))
                });
        self.lazy.borrow_mut().finish_calculation(&result);
        result
    }

    fn perform_calculations(&self) -> QlResult<()> {
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
            crate::require!(
                optionlet_times[i].is_finite() && optionlet_times[i] > 0.0,
                "optionlet fixing time must be positive"
            );
            atm_optionlet_rate[i] = coupon.index_fixing()?;
        }

        let switch_strike = self
            .options
            .switch_strike
            .unwrap_or_else(|| atm_optionlet_rate.iter().sum::<Rate>() / n_optionlet as Real);

        let discount_handle = if self.base.discount().is_empty() {
            index.forwarding_term_structure().clone()
        } else {
            self.base.discount().clone()
        };
        let discount_curve = discount_handle.current_link()?;

        let vol_quote = shared(SimpleQuote::new(Some(0.20)));
        let engine: SharedMut<dyn PricingEngine> =
            if self.base.volatility_type() == VolatilityType::Normal {
                shared_mut(BachelierCapFloorEngine::with_flat_vol(
                    discount_handle,
                    Handle::new(Shared::clone(&vol_quote) as Shared<dyn Quote>),
                    day_counter,
                    Shared::clone(&settings),
                )?)
            } else {
                shared_mut(BlackCapFloorEngine::with_flat_vol(
                    discount_handle,
                    Handle::new(Shared::clone(&vol_quote) as Shared<dyn Quote>),
                    day_counter,
                    displacement,
                    Shared::clone(&settings),
                )?)
            };

        let mut optionlet_volatilities = vec![vec![0.0; n_strikes]; n_optionlet];
        let mut std_devs = self.optionlet_std_devs.borrow().clone();

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
                let solved = if self.base.volatility_type() == VolatilityType::Normal {
                    bachelier_black_formula_implied_vol(
                        optionlet_type,
                        strikes[j],
                        atm_optionlet_rate[i],
                        optionlet_times[i],
                        optionlet_price,
                        annuity,
                    )
                    .map(|v| v * optionlet_times[i].sqrt())
                } else {
                    black_formula_implied_std_dev(
                        optionlet_type,
                        strikes[j],
                        atm_optionlet_rate[i],
                        optionlet_price,
                        annuity,
                        displacement,
                        std_devs[i][j],
                        self.accuracy,
                        self.max_iter,
                    )
                };
                let std_dev = match solved {
                    Ok(value) => value,
                    Err(_) if self.options.dont_throw => 0.0,
                    Err(error) => return Err(error),
                };
                std_devs[i][j] = std_dev;
                optionlet_volatilities[i][j] = std_dev / optionlet_times[i].sqrt();
            }
        }
        *self.optionlet_std_devs.borrow_mut() = std_devs;
        self.switch_strike.set(switch_strike);

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

impl AsObservable for OptionletStripper1 {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StrippedOptionletBase for OptionletStripper1 {
    fn observable(&self) -> Option<&Observable> {
        Some(&self.observable)
    }
    fn optionlet_strikes(&self, i: usize) -> QlResult<Vec<Rate>> {
        self.calculate()?;
        self.base
            .caches()
            .borrow()
            .optionlet_strikes
            .get(i)
            .cloned()
            .ok_or_else(|| {
                crate::errors::QlError::new(
                    format!("optionlet index {i} out of range"),
                    file!(),
                    line!(),
                )
            })
    }

    fn optionlet_volatilities(&self, i: usize) -> QlResult<Vec<Volatility>> {
        self.calculate()?;
        self.base
            .caches()
            .borrow()
            .optionlet_volatilities
            .get(i)
            .cloned()
            .ok_or_else(|| {
                crate::errors::QlError::new(
                    format!("optionlet index {i} out of range"),
                    file!(),
                    line!(),
                )
            })
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;

    const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;

    fn eval_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn settings() -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(eval_date());
        settings
    }

    fn flat_curve() -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            eval_date(),
            0.04,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn euribor6m(
        curve: Handle<dyn YieldTermStructure>,
        settings: Shared<Settings<Date>>,
    ) -> Shared<IborIndex> {
        shared(crate::indexes::ibor::Euribor::six_months(curve, settings))
    }

    fn strikes() -> Vec<Rate> {
        vec![0.02, 0.03, 0.04, 0.05, 0.06]
    }

    fn flat_surface() -> Shared<CapFloorTermVolSurface> {
        let option_tenors: Vec<Period> = (1..=5).map(|n| Period::new(n, TimeUnit::Years)).collect();
        let vols: Vec<Vec<Handle<dyn Quote>>> = option_tenors
            .iter()
            .map(|_| {
                strikes()
                    .iter()
                    .map(|_| Handle::new(shared(SimpleQuote::new(Some(0.20))) as Shared<dyn Quote>))
                    .collect()
            })
            .collect();
        shared(
            CapFloorTermVolSurface::with_reference_date(
                eval_date(),
                Target::new(),
                BDC,
                option_tenors,
                strikes(),
                vols,
                Actual365Fixed::new(),
            )
            .unwrap(),
        )
    }

    fn stripper(vol_type: VolatilityType) -> OptionletStripper1 {
        let settings = settings();
        let index = euribor6m(flat_curve(), settings);
        OptionletStripper1::new(
            flat_surface(),
            index,
            Handle::<dyn YieldTermStructure>::empty(),
            1e-6,
            100,
            vol_type,
            0.0,
            None,
        )
        .unwrap()
    }

    /// Stripping the flat surface yields a full grid of finite, positive
    /// optionlet volatilities whose fixing times strictly increase, matching the
    /// base tenor/strike grid.
    #[test]
    fn strips_a_flat_surface_to_a_finite_positive_grid() {
        let stripper = stripper(VolatilityType::ShiftedLognormal);
        let n = stripper.optionlet_maturities();
        assert!(n > 1);

        let times = stripper.optionlet_fixing_times().unwrap();
        assert_eq!(times.len(), n);
        for pair in times.windows(2) {
            assert!(pair[1] > pair[0], "fixing times not increasing: {times:?}");
        }

        for i in 0..n {
            let vols = stripper.optionlet_volatilities(i).unwrap();
            assert_eq!(vols.len(), strikes().len());
            for v in vols {
                assert!(
                    v.is_finite() && v > 0.0,
                    "optionlet vol {v} at maturity {i}"
                );
            }
        }
    }

    /// A flat 20% cap/floor term-vol surface strips to optionlet volatilities
    /// near 20%: a flat term vol is the average of flat optionlet vols. This is a
    /// sanity band; the exact reprice identity is the discriminating oracle #576.
    #[test]
    fn stripped_vols_of_a_flat_surface_are_near_the_flat_input() {
        let stripper = stripper(VolatilityType::ShiftedLognormal);
        for i in 0..stripper.optionlet_maturities() {
            for v in stripper.optionlet_volatilities(i).unwrap() {
                assert!(
                    (v - 0.20).abs() < 0.02,
                    "stripped optionlet vol {v} at maturity {i} is not near 0.20"
                );
            }
        }
    }

    /// The floating switch strike is the mean at-the-money optionlet rate; on a
    /// flat 4% forward curve it sits near 4% (`optionletstripper1.cpp:86-92`).
    #[test]
    fn switch_strike_is_the_mean_atm_rate() {
        let stripper = stripper(VolatilityType::ShiftedLognormal);
        let switch = stripper.switch_strike().unwrap();
        let atm = stripper.atm_optionlet_rates().unwrap();
        let mean = atm.iter().sum::<Rate>() / atm.len() as Real;
        assert!((switch - mean).abs() < 1e-12);
        assert!((0.035..0.045).contains(&switch), "switch strike {switch}");
    }

    /// The same flat surface now supports the normal-model inversion.
    #[test]
    fn normal_volatility_type_is_supported() {
        let stripper = stripper(VolatilityType::Normal);
        assert!(
            stripper
                .optionlet_volatilities(0)
                .unwrap()
                .iter()
                .all(|v| (*v - 0.20).abs() < 1e-10)
        );
    }
}
