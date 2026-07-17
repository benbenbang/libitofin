//! Swaption calibration helper.
//!
//! Port of `ql/models/shortrate/calibrationhelpers/swaptionhelper.{hpp,cpp}`.
//! [`SwaptionHelper`] is a
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper):
//! it builds a [`Swaption`] over a fixed-vs-Ibor swap, prices its market value
//! from a quoted Black (or normal) volatility, and prices its model value
//! through the model pricing engine a calibration installs (a
//! [`JamshidianSwaptionEngine`](crate::pricingengines::JamshidianSwaptionEngine)
//! for Hull-White, say).
//!
//! ## Ported surface
//!
//! Only the `(maturity: Period, length: Period, ...)` constructor
//! (`swaptionhelper.cpp:35-58`) is ported - the constructor the short-rate
//! calibration oracle (`shortratemodels.cpp:108`) uses. The two date-based
//! constructors (`swaptionhelper.cpp:60-108`), which fix the exercise date (and
//! end date) directly rather than deriving them from `maturity` (and `length`),
//! are a thin follow-up and are deferred visibly.
//!
//! ## Deferred (visible, not silently stubbed)
//!
//! - **Overnight indexes / the `OvernightIndexedSwap` branch of `makeSwap`**
//!   (`swaptionhelper.cpp:205-215`): C++ `dynamic_pointer_cast`s the index to an
//!   `OvernightIndex` and, if it succeeds, builds an OIS whose daily fixings are
//!   combined per `averaging_method`. A Rust newtype `Shared<IborIndex>` carries
//!   no such downcast, so this port always builds the plain [`VanillaSwap`]
//!   branch (`:216-219`). Overnight indexes are therefore NOT supported here.
//!   `averaging_method` is kept in the signature for fidelity but is only read
//!   back through [`averaging_method`](SwaptionHelper::averaging_method); it does
//!   not affect the built swap. Residual risk: the crate's `OvernightIndex` holds
//!   an inner `Shared<IborIndex>`, so a caller could reach in and pass that inner
//!   index, and this helper would silently build a vanilla swap over it (the
//!   "at best a decent proxy, at worst simply wrong" warning of the C++ class
//!   doc, `swaptionhelper.hpp:36-40`). No runtime detection is possible.
//! - **`addTimesTo`** (`swaptionhelper.cpp:111-121`) builds a
//!   `DiscretizedSwaption` for the tree/lattice pricing path, which is unported;
//!   it is already omitted from the
//!   [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper)
//!   trait surface (#396), so there is nothing to implement. The analytic
//!   Jamshidian engine never calls it.
//!
//! ## Divergences from QuantLib
//!
//! - **`Settings` is read from the index, not a global.** C++ reads the global
//!   `Settings::instance()`; per D5 the core has no global, and the index already
//!   carries the explicit [`Settings`] its fixings and evaluation date live on.
//!   The helper reuses that same handle for the swaption and both the discounting
//!   and Black/Bachelier engines, so their evaluation dates are consistent by
//!   construction. This also keeps the constructor signature the oracle expects
//!   (no `settings` argument).
//! - **`model_value` / `black_price` are `&self`; the swaption is cached.** C++'s
//!   `modelValue`/`blackPrice` are const and call `calculate()` to (re)build the
//!   `mutable swaption_`. The #396 trait fixes these as `&self`, and the base's
//!   market-value cache seam is `&mut`, so the swaption is held in a [`RefCell`]:
//!   [`black_price`](SwaptionHelper::black_price) rebuilds and stores it on every
//!   call (it runs only on the stale market path or the implied-vol solver, so
//!   this refreshes after any index/term-structure/volatility change), and
//!   [`model_value`](SwaptionHelper::model_value) builds it only if absent and
//!   otherwise reuses the fresh instrument. The one unreachable edge -
//!   `model_value` called standalone immediately after an invalidation with no
//!   intervening `market_value` - would price a stale swaption; a calibration
//!   drives `calibration_error` (hence `market_value`) first, so it never occurs.
//! - **The `QL_FAIL` default of `blackPrice`'s `switch`** (`swaptionhelper.cpp:142`)
//!   is unreachable: the Rust [`VolatilityType`] is a two-variant enum, so both
//!   arms are covered and there is no default branch.
//! - **A missing model engine is an explicit `Err`.** C++ `modelValue` would
//!   dereference a null `engine_`; the port returns an error (D4).

use std::cell::RefCell;

use crate::cashflows::RateAveraging;
use crate::errors::QlResult;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::fail;
use crate::handle::Handle;
use crate::indexes::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::{
    FixedVsFloatingSwap, SettlementMethod, SettlementType, SwapType, Swaption, VanillaSwap,
};
use crate::models::calibrationhelper::{
    BlackCalibrationHelper, BlackCalibrationHelperBase, CalibrationErrorType,
};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::{
    BachelierSwaptionEngine, BlackSwaptionEngine, CashAnnuityModel, DiscountingSwapEngine,
};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::VolatilityType;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Real};

/// Calibration helper for interest-rate swaptions (`swaptionhelper.hpp:42`).
pub struct SwaptionHelper {
    base: BlackCalibrationHelperBase,
    maturity: Period,
    length: Period,
    index: Shared<IborIndex>,
    term_structure: Handle<dyn YieldTermStructure>,
    fixed_leg_tenor: Period,
    fixed_leg_day_counter: DayCounter,
    floating_leg_day_counter: DayCounter,
    strike: Option<Real>,
    nominal: Real,
    settlement_days: Option<Natural>,
    averaging_method: RateAveraging,
    settings: Shared<Settings<Date>>,
    swaption: RefCell<Option<SharedMut<Swaption>>>,
}

impl SwaptionHelper {
    /// Builds a helper from a swaption maturity and swap length
    /// (`swaptionhelper.cpp:35-58`).
    ///
    /// `strike` of `None` is C++'s `Null<Real>()` (the swaption is struck at the
    /// forward rate). The constructor registers the base's observer with the
    /// index and the term-structure handle (the C++ `registerWith(index_)` /
    /// `registerWith(termStructure_)`, `:56-57`), so a change to either
    /// invalidates the cached market value alongside the volatility handle the
    /// base already registers.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        maturity: Period,
        length: Period,
        volatility: Handle<dyn Quote>,
        index: Shared<IborIndex>,
        fixed_leg_tenor: Period,
        fixed_leg_day_counter: DayCounter,
        floating_leg_day_counter: DayCounter,
        term_structure: Handle<dyn YieldTermStructure>,
        error_type: CalibrationErrorType,
        strike: Option<Real>,
        nominal: Real,
        volatility_type: VolatilityType,
        shift: Real,
        settlement_days: Option<Natural>,
        averaging_method: RateAveraging,
    ) -> SwaptionHelper {
        let base = BlackCalibrationHelperBase::new(volatility, error_type, volatility_type, shift);
        let settings = index.base().settings().clone();

        let observer = base.observer();
        index.observable().register_observer(&observer);
        term_structure.register_observer(&observer);

        SwaptionHelper {
            base,
            maturity,
            length,
            index,
            term_structure,
            fixed_leg_tenor,
            fixed_leg_day_counter,
            floating_leg_day_counter,
            strike,
            nominal,
            settlement_days,
            averaging_method,
            settings,
            swaption: RefCell::new(None),
        }
    }

    /// The overnight-fixing averaging convention (`averagingMethod_`).
    ///
    /// Kept for fidelity with the C++ constructor; it steers only the
    /// (deferred) overnight-indexed-swap branch of `makeSwap`, so it does not
    /// affect the vanilla swap this port builds.
    pub fn averaging_method(&self) -> RateAveraging {
        self.averaging_method
    }

    /// The built swaption (`swaption()`, `swaptionhelper.hpp:100`).
    ///
    /// Builds it on first use; a subsequent [`black_price`](Self::black_price)
    /// (via the market-value path) rebuilds it, leaving the model engine
    /// installed.
    ///
    /// # Errors
    ///
    /// Propagates a failure of the swaption construction (an empty curve handle,
    /// a fair-rate solve failure).
    pub fn swaption(&self) -> QlResult<SharedMut<Swaption>> {
        self.ensure_built()
    }

    /// The underlying swap (`underlying()`, `swaptionhelper.hpp:96`).
    ///
    /// # Errors
    ///
    /// Propagates a failure of the swaption construction.
    pub fn underlying(&self) -> QlResult<SharedMut<FixedVsFloatingSwap>> {
        Ok(SharedMut::clone(self.ensure_built()?.borrow().underlying()))
    }

    /// Returns the cached swaption, building and caching it if absent.
    fn ensure_built(&self) -> QlResult<SharedMut<Swaption>> {
        let existing = self.swaption.borrow().as_ref().map(SharedMut::clone);
        match existing {
            Some(swaption) => Ok(swaption),
            None => self.build_and_store(),
        }
    }

    /// Rebuilds the swaption and replaces the cache, returning the fresh one.
    fn build_and_store(&self) -> QlResult<SharedMut<Swaption>> {
        let swaption = self.build_swaption()?;
        *self.swaption.borrow_mut() = Some(SharedMut::clone(&swaption));
        Ok(swaption)
    }

    /// `performCalculations`'s instrument construction (`swaptionhelper.cpp:152-197`):
    /// derives the exercise, start and end dates, builds the fixed and floating
    /// schedules, solves the forward off a zero-rate swap, and assembles the
    /// struck swap and the European swaption over it.
    fn build_swaption(&self) -> QlResult<SharedMut<Swaption>> {
        let calendar = self.index.fixing_calendar();
        let bdc = self.index.business_day_convention();

        let reference_date = self.term_structure.current_link()?.reference_date()?;
        let exercise_date = calendar.advance_by_period(reference_date, self.maturity, bdc, false);

        let start_date = match self.settlement_days {
            None => self
                .index
                .value_date(calendar.adjust(exercise_date, BusinessDayConvention::Following))?,
            Some(_) => calendar.advance(
                exercise_date,
                self.index.fixing_days() as Integer,
                TimeUnit::Days,
                bdc,
                false,
            ),
        };
        let end_date = calendar.advance_by_period(start_date, self.length, bdc, false);

        let fixed_schedule = Schedule::new(
            start_date,
            end_date,
            self.fixed_leg_tenor,
            calendar.clone(),
            bdc,
            bdc,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );
        let float_schedule = Schedule::new(
            start_date,
            end_date,
            self.index.tenor(),
            calendar,
            bdc,
            bdc,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );

        let swap_engine = shared_mut(DiscountingSwapEngine::new(
            self.term_structure.clone(),
            Some(false),
            None,
            None,
            Shared::clone(&self.settings),
        )) as SharedMut<dyn PricingEngine>;

        let mut temp = self.make_swap(
            fixed_schedule.clone(),
            float_schedule.clone(),
            0.0,
            SwapType::Receiver,
        )?;
        temp.base_mut()
            .set_pricing_engine(SharedMut::clone(&swap_engine));
        let forward = temp.fair_rate()?;

        let (exercise_rate, swap_type) = match self.strike {
            None => (forward, SwapType::Receiver),
            Some(strike) => (
                strike,
                if strike <= forward {
                    SwapType::Receiver
                } else {
                    SwapType::Payer
                },
            ),
        };

        let mut swap = self.make_swap(fixed_schedule, float_schedule, exercise_rate, swap_type)?;
        swap.base_mut()
            .set_pricing_engine(SharedMut::clone(&swap_engine));
        let swap = shared_mut(swap);

        let exercise = shared(EuropeanExercise::new(exercise_date)) as Shared<dyn Exercise>;
        let swaption = Swaption::new(
            swap,
            exercise,
            SettlementType::Physical,
            SettlementMethod::PhysicalOTC,
            Shared::clone(&self.settings),
        );
        Ok(shared_mut(swaption))
    }

    /// `makeSwap` (`swaptionhelper.cpp:201-216`), the [`VanillaSwap`] branch. The
    /// overnight-indexed-swap branch is deferred (see the module docs).
    fn make_swap(
        &self,
        fixed_schedule: Schedule,
        float_schedule: Schedule,
        exercise_rate: Real,
        swap_type: SwapType,
    ) -> QlResult<FixedVsFloatingSwap> {
        let swap = VanillaSwap::new(
            swap_type,
            self.nominal,
            fixed_schedule,
            exercise_rate,
            self.fixed_leg_day_counter.clone(),
            float_schedule,
            Shared::clone(&self.index),
            0.0,
            self.floating_leg_day_counter.clone(),
            None,
            Shared::clone(&self.settings),
        )?;
        Ok(swap.into_fixed_vs_floating())
    }
}

impl BlackCalibrationHelper for SwaptionHelper {
    fn base(&self) -> &BlackCalibrationHelperBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BlackCalibrationHelperBase {
        &mut self.base
    }

    /// `modelValue` (`swaptionhelper.cpp:123-127`): installs the model engine on
    /// the swaption and returns its NPV.
    fn model_value(&self) -> QlResult<Real> {
        let swaption = self.ensure_built()?;
        let Some(engine) = self.base.pricing_engine() else {
            fail!("no model pricing engine set on the swaption helper");
        };
        swaption
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(SharedMut::clone(engine));
        let value = swaption.borrow_mut().npv()?;
        Ok(value)
    }

    /// `blackPrice` (`swaptionhelper.cpp:129-150`): prices the swaption through a
    /// [`BlackSwaptionEngine`] (shifted-lognormal) or [`BachelierSwaptionEngine`]
    /// (normal) at `sigma`, then restores the model engine (`:148`) so a later
    /// price on the currently-installed engine reflects the model, not this
    /// temporary Black engine.
    fn black_price(&self, sigma: Real) -> QlResult<Real> {
        let swaption = self.build_and_store()?;

        let vol: Handle<dyn Quote> =
            Handle::new(shared(SimpleQuote::new(sigma)) as Shared<dyn Quote>);
        let engine: SharedMut<dyn PricingEngine> = match self.base.volatility_type() {
            VolatilityType::ShiftedLognormal => shared_mut(BlackSwaptionEngine::with_flat_vol(
                self.term_structure.clone(),
                vol,
                Actual365Fixed::new(),
                self.base.shift(),
                CashAnnuityModel::DiscountCurve,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>,
            VolatilityType::Normal => shared_mut(BachelierSwaptionEngine::with_flat_vol(
                self.term_structure.clone(),
                vol,
                Actual365Fixed::new(),
                self.base.shift(),
                CashAnnuityModel::DiscountCurve,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>,
        };

        swaption.borrow_mut().base_mut().set_pricing_engine(engine);
        let value = swaption.borrow_mut().npv()?;

        if let Some(model_engine) = self.base.pricing_engine() {
            swaption
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(SharedMut::clone(model_engine));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indexes::ibor::Euribor;
    use crate::interestrate::Compounding;
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;

    use crate::models::calibrationhelper::CalibrationHelper;
    use crate::models::shortrate::HullWhite;
    use crate::pricingengines::JamshidianSwaptionEngine;

    const VOL: Real = 0.20;
    const A: Real = 0.05;
    const SIGMA: Real = 0.01;

    fn today() -> Date {
        Date::new(15, Month::January, 2026)
    }

    /// A flat 5% continuously-compounded Actual365Fixed curve referenced at the
    /// evaluation date, forecasting the Euribor 6M index and discounting the
    /// swaptions (D5: one explicit [`Settings`]).
    struct Fixture {
        settings: Shared<Settings<Date>>,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<IborIndex>,
        calendar: Calendar,
    }

    impl Fixture {
        fn new() -> Fixture {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today());
            let calendar = Target::new();
            let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
                today(),
                0.05,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            ))
                as Shared<dyn YieldTermStructure>);
            let index = shared(Euribor::six_months(curve.clone(), Shared::clone(&settings)));
            Fixture {
                settings,
                curve,
                index,
                calendar,
            }
        }

        fn helper(&self, strike: Option<Real>) -> SwaptionHelper {
            self.helper_with(strike, Period::new(1, TimeUnit::Years), 1.0)
        }

        fn helper_with(
            &self,
            strike: Option<Real>,
            fixed_leg_tenor: Period,
            nominal: Real,
        ) -> SwaptionHelper {
            let vol: Handle<dyn Quote> =
                Handle::new(shared(SimpleQuote::new(VOL)) as Shared<dyn Quote>);
            SwaptionHelper::new(
                Period::new(5, TimeUnit::Years),
                Period::new(5, TimeUnit::Years),
                vol,
                Shared::clone(&self.index),
                fixed_leg_tenor,
                Thirty360::with_convention(Convention::BondBasis),
                Actual360::new(),
                self.curve.clone(),
                CalibrationErrorType::RelativePriceError,
                strike,
                nominal,
                VolatilityType::ShiftedLognormal,
                0.0,
                None,
                RateAveraging::Compound,
            )
        }

        /// An independently hand-built European swaption reproducing the helper's
        /// `performCalculations` recipe from the C++ source
        /// (`swaptionhelper.cpp:152-197`), so a construction bug in the helper
        /// makes the two diverge.
        fn reference_swaption(&self, strike: Option<Real>) -> SharedMut<Swaption> {
            let bdc = self.index.business_day_convention();
            let reference_date = self.curve.current_link().unwrap().reference_date().unwrap();
            let exercise_date = self.calendar.advance_by_period(
                reference_date,
                Period::new(5, TimeUnit::Years),
                bdc,
                false,
            );
            let start_date = self
                .index
                .value_date(
                    self.calendar
                        .adjust(exercise_date, BusinessDayConvention::Following),
                )
                .unwrap();
            let end_date = self.calendar.advance_by_period(
                start_date,
                Period::new(5, TimeUnit::Years),
                bdc,
                false,
            );

            let fixed_schedule = Schedule::new(
                start_date,
                end_date,
                Period::new(1, TimeUnit::Years),
                self.calendar.clone(),
                bdc,
                bdc,
                DateGeneration::Forward,
                false,
                Date::null(),
                Date::null(),
            );
            let float_schedule = Schedule::new(
                start_date,
                end_date,
                self.index.tenor(),
                self.calendar.clone(),
                bdc,
                bdc,
                DateGeneration::Forward,
                false,
                Date::null(),
                Date::null(),
            );

            let make_swap = |rate: Real, swap_type: SwapType| -> FixedVsFloatingSwap {
                VanillaSwap::new(
                    swap_type,
                    1.0,
                    fixed_schedule.clone(),
                    rate,
                    Thirty360::with_convention(Convention::BondBasis),
                    float_schedule.clone(),
                    Shared::clone(&self.index),
                    0.0,
                    Actual360::new(),
                    None,
                    Shared::clone(&self.settings),
                )
                .unwrap()
                .into_fixed_vs_floating()
            };

            let swap_engine = shared_mut(DiscountingSwapEngine::new(
                self.curve.clone(),
                Some(false),
                None,
                None,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>;

            let mut temp = make_swap(0.0, SwapType::Receiver);
            temp.base_mut()
                .set_pricing_engine(SharedMut::clone(&swap_engine));
            let forward = temp.fair_rate().unwrap();

            let (exercise_rate, swap_type) = match strike {
                None => (forward, SwapType::Receiver),
                Some(strike) => (
                    strike,
                    if strike <= forward {
                        SwapType::Receiver
                    } else {
                        SwapType::Payer
                    },
                ),
            };

            let mut swap = make_swap(exercise_rate, swap_type);
            swap.base_mut()
                .set_pricing_engine(SharedMut::clone(&swap_engine));

            shared_mut(Swaption::new(
                shared_mut(swap),
                shared(EuropeanExercise::new(exercise_date)) as Shared<dyn Exercise>,
                SettlementType::Physical,
                SettlementMethod::PhysicalOTC,
                Shared::clone(&self.settings),
            ))
        }

        /// Prices a swaption through the Black engine over the fixture curve at
        /// `VOL`, the `blackPrice`/market-value engine (`swaptionhelper.cpp:135`).
        fn black_price(&self, swaption: &SharedMut<Swaption>) -> Real {
            let vol: Handle<dyn Quote> =
                Handle::new(shared(SimpleQuote::new(VOL)) as Shared<dyn Quote>);
            let engine = shared_mut(BlackSwaptionEngine::with_flat_vol(
                self.curve.clone(),
                vol,
                Actual365Fixed::new(),
                0.0,
                CashAnnuityModel::DiscountCurve,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>;
            swaption.borrow_mut().base_mut().set_pricing_engine(engine);
            swaption.borrow_mut().npv().unwrap()
        }

        fn hw_model(&self) -> SharedMut<HullWhite> {
            HullWhite::new(self.curve.clone(), A, SIGMA).unwrap()
        }

        /// A standalone Jamshidian engine over the fixture's Hull-White model,
        /// the model engine a calibration installs on the helper (#392).
        fn jamshidian_engine(&self) -> SharedMut<dyn PricingEngine> {
            shared_mut(JamshidianSwaptionEngine::new(self.hw_model()))
                as SharedMut<dyn PricingEngine>
        }

        /// Prices a swaption on the given engine.
        fn price_on(
            &self,
            swaption: &SharedMut<Swaption>,
            engine: SharedMut<dyn PricingEngine>,
        ) -> Real {
            swaption.borrow_mut().base_mut().set_pricing_engine(engine);
            swaption.borrow_mut().npv().unwrap()
        }
    }

    /// The helper's `market_value` equals a swaption built independently to the
    /// C++ recipe and priced through the same Black engine, for an explicit
    /// strike (a deterministic `exercise_rate`, isolating construction) - the
    /// market-side pin of `performCalculations` and `black_price`.
    #[test]
    fn market_value_matches_an_independently_built_black_swaption_at_a_fixed_strike() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(Some(0.03));

        let market = helper.market_value().unwrap();
        let reference = fixture.black_price(&fixture.reference_swaption(Some(0.03)));

        assert!(
            (market - reference).abs() <= 1.0e-12,
            "market {market} vs independently-built black price {reference} (error {})",
            (market - reference).abs()
        );
    }

    /// The same pin for the `strike = None` (at-the-forward) path, where the
    /// helper solves the exercise rate off a zero-rate swap.
    #[test]
    fn market_value_matches_an_independently_built_black_swaption_at_the_forward() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(None);

        let market = helper.market_value().unwrap();
        let reference = fixture.black_price(&fixture.reference_swaption(None));

        assert!(
            (market - reference).abs() <= 1.0e-12,
            "market {market} vs independently-built black price {reference} (error {})",
            (market - reference).abs()
        );
    }

    /// The helper's `model_value` equals the same swaption priced through a
    /// standalone Jamshidian engine on the fixture Hull-White model (#392) - the
    /// model-side pin. The strike is explicit, so both sides build an identical
    /// swaption and the two prices agree to machine precision.
    #[test]
    fn model_value_matches_a_standalone_jamshidian_swaption() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(Some(0.03));
        helper
            .base_mut()
            .set_pricing_engine(fixture.jamshidian_engine());

        let model = helper.model_value().unwrap();
        let reference = fixture.price_on(
            &fixture.reference_swaption(Some(0.03)),
            fixture.jamshidian_engine(),
        );

        assert!(
            (model - reference).abs() <= 1.0e-12,
            "model {model} vs standalone jamshidian price {reference} (error {})",
            (model - reference).abs()
        );
    }

    /// `RelativePriceError` calibration error is `|market - model| / market` of
    /// the two prices the helper reports.
    #[test]
    fn relative_calibration_error_is_the_market_model_gap() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(Some(0.03));
        helper
            .base_mut()
            .set_pricing_engine(fixture.jamshidian_engine());

        let market = helper.market_value().unwrap();
        let model = helper.model_value().unwrap();
        let expected = (market - model).abs() / market;

        let error = helper.calibration_error().unwrap();
        assert!(
            (error - expected).abs() <= 1.0e-12,
            "calibration error {error} vs |{market} - {model}| / {market} = {expected}"
        );
    }

    /// Perturbing a construction input moves `market_value`: doubling the nominal
    /// reports a different market value, so the value genuinely reflects the
    /// built instrument rather than a constant. The independently built reference
    /// (unchanged inputs) still matches the base helper - pinned by
    /// [`market_value_matches_an_independently_built_black_swaption_at_a_fixed_strike`].
    #[test]
    fn market_value_moves_when_a_construction_input_is_perturbed() {
        let fixture = Fixture::new();
        let mut base = fixture.helper(Some(0.03));
        let mut perturbed = fixture.helper_with(Some(0.03), Period::new(1, TimeUnit::Years), 2.0);

        let base_value = base.market_value().unwrap();
        let perturbed_value = perturbed.market_value().unwrap();

        assert!(
            (perturbed_value - base_value).abs() > 1.0e-8,
            "market value did not move when the nominal was perturbed: \
             base {base_value} vs perturbed {perturbed_value}"
        );
    }

    /// The engine restore at `swaptionhelper.cpp:148`: after `market_value` runs
    /// `black_price` internally, the cached swaption is left on the model engine,
    /// not the temporary Black engine. Pricing it on its currently-installed
    /// engine gives the model price (not the Black market price). Removing the
    /// restore leaves the Black engine installed, so the price would equal
    /// `market` and this assertion would fail - which pins the restore.
    #[test]
    fn black_price_restores_the_model_engine_on_the_swaption() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(Some(0.03));
        helper
            .base_mut()
            .set_pricing_engine(fixture.jamshidian_engine());

        let market = helper.market_value().unwrap();
        let model = fixture.price_on(
            &fixture.reference_swaption(Some(0.03)),
            fixture.jamshidian_engine(),
        );

        let swaption = helper.swaption().unwrap();
        let priced_on_installed_engine = swaption.borrow_mut().npv().unwrap();

        assert!(
            (priced_on_installed_engine - model).abs() <= 1.0e-12,
            "the restored engine priced {priced_on_installed_engine}, expected the model \
             price {model}"
        );
        assert!(
            (priced_on_installed_engine - market).abs() > 1.0e-8,
            "the restored price {priced_on_installed_engine} must differ from the black \
             market price {market}"
        );
    }
}
