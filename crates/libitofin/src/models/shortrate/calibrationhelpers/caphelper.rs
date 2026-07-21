//! Cap calibration helper.
//!
//! Port of `ql/models/shortrate/calibrationhelpers/caphelper.{hpp,cpp}`.
//! [`CapHelper`] is a
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper):
//! it builds an at-the-money [`Cap`](crate::instruments::CapFloor) over a
//! fixed-vs-Ibor swap's floating leg, prices its market value from a quoted
//! Black volatility, and prices its model value through the model pricing engine
//! a calibration installs (an
//! [`AnalyticCapFloorEngine`](crate::pricingengines::AnalyticCapFloorEngine) for
//! Hull-White).
//!
//! The cap is struck at the swap's fair rate: `performCalculations`
//! (`caphelper.cpp:91-144`) builds the floating and fixed legs on schedules
//! spanning `include_first_swaplet ? referenceDate : referenceDate + indexTenor`
//! to `referenceDate + length`, prices a plain fixed-vs-float [`Swap`] on the
//! discount curve, and solves `fairRate = 0.04 - NPV / (legBPS(fixed) / 1e-4)`
//! off it.
//!
//! ## Deferred (visible, not silently stubbed)
//!
//! - **The Normal/Bachelier `black_price` branch** (`caphelper.cpp:78-81`): the
//!   C++ switch prices a `Normal` volatility through a `BachelierCapFloorEngine`,
//!   which is not ported (only [`BlackCapFloorEngine`] is on main). The
//!   [`ShiftedLognormal`](VolatilityType::ShiftedLognormal) arm is ported; the
//!   [`Normal`](VolatilityType::Normal) arm returns an error naming the deferral.
//!   `ShiftedLognormal` is the C++ default, so the calibration oracle is
//!   unaffected.
//! - **`addTimesTo`** (`caphelper.cpp:51-61`) builds a `DiscretizedCapFloor` for
//!   the tree/lattice pricing path, which is unported; it is already omitted from
//!   the [`BlackCalibrationHelper`] trait surface (the lattice deferral of
//!   `calibrationhelper.rs:35`), so there is nothing to implement. The analytic
//!   cap engine never calls it.
//!
//! ## Divergences from QuantLib
//!
//! - **The dead `dummyIndex` is omitted.** C++ builds an `IborIndex("dummy", ...)`
//!   (`caphelper.cpp:103-112`) that is never read: both the floating leg
//!   (`:121`) and the schedules are built from the original `index_`. Porting the
//!   dummy index would construct an index nothing uses, so it is dropped.
//! - **`CapFloor::cap` takes concrete coupons.** C++ passes the erased `Leg` to
//!   both `Swap` and `Cap`. The Rust [`CapFloor::cap`] takes
//!   `Vec<Shared<IborCoupon>>` (it cannot downcast an erased leg back to a
//!   coupon), so the coupons are built once with [`IborLeg::coupons`] and shared:
//!   erased into the swap's floating leg, and passed concrete into the cap.
//! - **`Settings` is read from the index, not a global.** Per D5 the core has no
//!   `Settings::instance()`; the index already carries the explicit [`Settings`]
//!   its fixings and evaluation date live on, and the helper reuses that handle
//!   for the swap, the cap and both engines.
//! - **`model_value` / `black_price` are `&self`; the cap is cached.** As
//!   [`SwaptionHelper`](super::SwaptionHelper) does, the built cap is held in a
//!   [`RefCell`]: [`black_price`](CapHelper::black_price) rebuilds and stores it on
//!   every call (the stale market path or the implied-vol solver), and
//!   [`model_value`](CapHelper::model_value) reuses the fresh instrument, building
//!   it only if absent.
//! - **A missing model engine is an explicit `Err`.** C++ `modelValue` would
//!   dereference a null `engine_`; the port returns an error (D4).

use std::cell::RefCell;

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{FixedRateLeg, IborLeg};
use crate::errors::QlResult;
use crate::fail;
use crate::handle::Handle;
use crate::indexes::IborIndex;
use crate::indexes::index::Index;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::instrument::Instrument;
use crate::instruments::{CapFloor, Swap};
use crate::interestrate::Compounding;
use crate::models::calibrationhelper::{
    BlackCalibrationHelper, BlackCalibrationHelperBase, CalibrationErrorType,
};
use crate::pricingengine::PricingEngine;
use crate::pricingengines::{BlackCapFloorEngine, DiscountingSwapEngine};
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::VolatilityType;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::Real;

/// The dummy fixed rate the swap is priced at to back out its fair rate
/// (`caphelper.cpp:94`).
const DUMMY_FIXED_RATE: Real = 0.04;

/// Calibration helper for an at-the-money cap (`caphelper.hpp:35`).
pub struct CapHelper {
    base: BlackCalibrationHelperBase,
    length: Period,
    index: Shared<IborIndex>,
    term_structure: Handle<dyn YieldTermStructure>,
    fixed_leg_frequency: Frequency,
    fixed_leg_day_counter: DayCounter,
    include_first_swaplet: bool,
    settings: Shared<Settings<Date>>,
    cap: RefCell<Option<SharedMut<CapFloor>>>,
}

impl CapHelper {
    /// Builds a helper for a cap of the given `length` (`caphelper.cpp:33-49`).
    ///
    /// The constructor registers the base's observer with the index and the
    /// term-structure handle (the C++ `registerWith(index_)` /
    /// `registerWith(termStructure_)`, `:47-48`), so a change to either
    /// invalidates the cached market value alongside the volatility handle the
    /// base already registers.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        length: Period,
        volatility: Handle<dyn Quote>,
        index: Shared<IborIndex>,
        fixed_leg_frequency: Frequency,
        fixed_leg_day_counter: DayCounter,
        include_first_swaplet: bool,
        term_structure: Handle<dyn YieldTermStructure>,
        error_type: CalibrationErrorType,
        volatility_type: VolatilityType,
        shift: Real,
    ) -> CapHelper {
        let base = BlackCalibrationHelperBase::new(volatility, error_type, volatility_type, shift);
        let settings = index.base().settings().clone();

        let observer = base.observer();
        index.observable().register_observer(&observer);
        term_structure.register_observer(&observer);

        CapHelper {
            base,
            length,
            index,
            term_structure,
            fixed_leg_frequency,
            fixed_leg_day_counter,
            include_first_swaplet,
            settings,
            cap: RefCell::new(None),
        }
    }

    /// The built at-the-money cap (`cap_`).
    ///
    /// Builds it on first use; a subsequent [`black_price`](Self::black_price)
    /// (via the market-value path) rebuilds it, leaving the model engine
    /// installed.
    ///
    /// # Errors
    ///
    /// Propagates a failure of the cap construction (an empty curve handle, a
    /// fair-rate solve failure).
    pub fn cap(&self) -> QlResult<SharedMut<CapFloor>> {
        self.ensure_built()
    }

    /// Returns the cached cap, building and caching it if absent.
    fn ensure_built(&self) -> QlResult<SharedMut<CapFloor>> {
        let existing = self.cap.borrow().as_ref().map(SharedMut::clone);
        match existing {
            Some(cap) => Ok(cap),
            None => self.build_and_store(),
        }
    }

    /// Rebuilds the cap and replaces the cache, returning the fresh one.
    fn build_and_store(&self) -> QlResult<SharedMut<CapFloor>> {
        let cap = self.build_cap()?;
        *self.cap.borrow_mut() = Some(SharedMut::clone(&cap));
        Ok(cap)
    }

    /// `performCalculations`'s instrument construction (`caphelper.cpp:91-140`):
    /// derives the schedule bounds, builds the floating and fixed legs, solves the
    /// fair rate off a dummy-rate swap, and assembles the ATM cap struck there.
    fn build_cap(&self) -> QlResult<SharedMut<CapFloor>> {
        let index_tenor = self.index.tenor();
        let calendar = self.index.fixing_calendar();
        let bdc = self.index.business_day_convention();

        let reference_date = self.term_structure.current_link()?.reference_date()?;
        let start_date = if self.include_first_swaplet {
            reference_date
        } else {
            reference_date + index_tenor
        };
        let maturity = reference_date + self.length;

        let float_schedule = Schedule::new(
            start_date,
            maturity,
            index_tenor,
            calendar.clone(),
            bdc,
            bdc,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );
        let coupons = IborLeg::new(float_schedule, Shared::clone(&self.index))
            .with_notionals(vec![1.0])
            .with_payment_adjustment(bdc)
            .with_fixing_days(0)
            .coupons()?;
        let floating_leg: Leg = coupons
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect();

        let fixed_schedule = Schedule::new(
            start_date,
            maturity,
            Period::try_from(self.fixed_leg_frequency)?,
            calendar,
            BusinessDayConvention::Unadjusted,
            BusinessDayConvention::Unadjusted,
            DateGeneration::Forward,
            false,
            Date::null(),
            Date::null(),
        );
        let fixed_leg = FixedRateLeg::new(fixed_schedule)
            .with_notionals(vec![1.0])
            .with_coupon_rate(
                DUMMY_FIXED_RATE,
                self.fixed_leg_day_counter.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )?
            .with_payment_adjustment(bdc)
            .build()?;

        let mut swap = Swap::two_leg(floating_leg, fixed_leg, Shared::clone(&self.settings));
        swap.base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                self.term_structure.clone(),
                Some(false),
                None,
                None,
                Shared::clone(&self.settings),
            )) as SharedMut<dyn PricingEngine>);
        let npv = swap.npv()?;
        let fixed_leg_bps = swap.leg_bps(1)?;
        let fair_rate = DUMMY_FIXED_RATE - npv / (fixed_leg_bps / 1.0e-4);

        let cap = CapFloor::cap(coupons, vec![fair_rate], Shared::clone(&self.settings))?;
        Ok(shared_mut(cap))
    }
}

impl BlackCalibrationHelper for CapHelper {
    fn base(&self) -> &BlackCalibrationHelperBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BlackCalibrationHelperBase {
        &mut self.base
    }

    /// `modelValue` (`caphelper.cpp:63-67`): installs the model engine on the cap
    /// and returns its NPV.
    fn model_value(&self) -> QlResult<Real> {
        let cap = self.ensure_built()?;
        let Some(engine) = self.base.pricing_engine() else {
            fail!("no model pricing engine set on the cap helper");
        };
        cap.borrow_mut()
            .base_mut()
            .set_pricing_engine(SharedMut::clone(engine));
        let value = cap.borrow_mut().npv()?;
        Ok(value)
    }

    /// `blackPrice` (`caphelper.cpp:69-89`): prices the cap through a
    /// [`BlackCapFloorEngine`] (shifted-lognormal) at `sigma`, then restores the
    /// model engine (`:87`) so a later price on the installed engine reflects the
    /// model, not this temporary Black engine. The `Normal` branch's
    /// `BachelierCapFloorEngine` is not ported (see the module docs).
    fn black_price(&self, sigma: Real) -> QlResult<Real> {
        let cap = self.build_and_store()?;

        let engine: SharedMut<dyn PricingEngine> = match self.base.volatility_type() {
            VolatilityType::ShiftedLognormal => {
                let vol: Handle<dyn Quote> =
                    Handle::new(shared(SimpleQuote::new(sigma)) as Shared<dyn Quote>);
                shared_mut(BlackCapFloorEngine::with_flat_vol(
                    self.term_structure.clone(),
                    vol,
                    crate::time::daycounters::actual365fixed::Actual365Fixed::new(),
                    self.base.shift(),
                    Shared::clone(&self.settings),
                )?) as SharedMut<dyn PricingEngine>
            }
            VolatilityType::Normal => fail!(
                "CapHelper Normal volatility needs a BachelierCapFloorEngine, which is not ported"
            ),
        };

        cap.borrow_mut().base_mut().set_pricing_engine(engine);
        let value = cap.borrow_mut().npv()?;

        if let Some(model_engine) = self.base.pricing_engine() {
            cap.borrow_mut()
                .base_mut()
                .set_pricing_engine(SharedMut::clone(model_engine));
        }
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    //! CapHelper has no QuantLib cached number (it appears in the test-suite only
    //! in the un-ported `libormarketmodel.cpp`), so the oracle is Rust-authored:
    //! a market-side pin against an independently-built Black cap, and a
    //! Hull-White calibration-convergence wiring check.

    use super::*;
    use crate::cashflows::IborCoupon;
    use crate::indexes::ibor::Euribor;
    use crate::instruments::CapFloor;
    use crate::interestrate::Compounding;
    use crate::math::optimization::endcriteria::EndCriteria;
    use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
    use crate::models::calibrationhelper::CalibrationHelper;
    use crate::models::model::{CalibratedModelHolder, calibrate, calibration_value};
    use crate::models::shortrate::HullWhite;
    use crate::pricingengines::AnalyticCapFloorEngine;
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;

    const VOL: Real = 0.20;
    const A: Real = 0.05;
    const SIGMA: Real = 0.01;

    fn today() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn years(n: i32) -> Period {
        Period::new(n, TimeUnit::Years)
    }

    /// The fixed-leg conventions the helper and the independent reference share.
    fn fixed_leg_frequency() -> Frequency {
        Frequency::Annual
    }

    fn fixed_leg_day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    /// A flat 5% continuously-compounded Actual365Fixed curve referenced at the
    /// evaluation date, forecasting the Euribor 6M index and discounting the caps
    /// (D5: one explicit [`Settings`]).
    struct Fixture {
        settings: Shared<Settings<Date>>,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<IborIndex>,
    }

    impl Fixture {
        fn new() -> Fixture {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today());
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
            }
        }

        fn helper(&self, include_first_swaplet: bool, length: Period) -> CapHelper {
            let vol: Handle<dyn Quote> =
                Handle::new(shared(SimpleQuote::new(VOL)) as Shared<dyn Quote>);
            CapHelper::new(
                length,
                vol,
                Shared::clone(&self.index),
                fixed_leg_frequency(),
                fixed_leg_day_counter(),
                include_first_swaplet,
                self.curve.clone(),
                CalibrationErrorType::RelativePriceError,
                VolatilityType::ShiftedLognormal,
                0.0,
            )
        }

        /// An independently hand-built ATM cap reproducing the helper's
        /// `performCalculations` recipe (`caphelper.cpp:91-140`) - including
        /// solving the fair rate off its OWN dummy-rate swap rather than reading
        /// the helper's strike, so a fair-rate bug in the helper makes the two
        /// diverge.
        fn reference_cap(
            &self,
            include_first_swaplet: bool,
            length: Period,
        ) -> SharedMut<CapFloor> {
            let index_tenor = self.index.tenor();
            let calendar = self.index.fixing_calendar();
            let bdc = self.index.business_day_convention();
            let reference_date = self.curve.current_link().unwrap().reference_date().unwrap();
            let start_date = if include_first_swaplet {
                reference_date
            } else {
                reference_date + index_tenor
            };
            let maturity = reference_date + length;

            let float_schedule = Schedule::new(
                start_date,
                maturity,
                index_tenor,
                calendar.clone(),
                bdc,
                bdc,
                DateGeneration::Forward,
                false,
                Date::null(),
                Date::null(),
            );
            let coupons: Vec<Shared<IborCoupon>> =
                IborLeg::new(float_schedule, Shared::clone(&self.index))
                    .with_notionals(vec![1.0])
                    .with_payment_adjustment(bdc)
                    .with_fixing_days(0)
                    .coupons()
                    .unwrap();
            let floating_leg: Leg = coupons
                .iter()
                .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
                .collect();

            let fixed_schedule = Schedule::new(
                start_date,
                maturity,
                Period::try_from(fixed_leg_frequency()).unwrap(),
                calendar,
                BusinessDayConvention::Unadjusted,
                BusinessDayConvention::Unadjusted,
                DateGeneration::Forward,
                false,
                Date::null(),
                Date::null(),
            );
            let fixed_leg = FixedRateLeg::new(fixed_schedule)
                .with_notionals(vec![1.0])
                .with_coupon_rate(
                    0.04,
                    fixed_leg_day_counter(),
                    Compounding::Simple,
                    Frequency::Annual,
                )
                .unwrap()
                .with_payment_adjustment(bdc)
                .build()
                .unwrap();

            let mut swap = Swap::two_leg(floating_leg, fixed_leg, Shared::clone(&self.settings));
            swap.base_mut()
                .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                    self.curve.clone(),
                    Some(false),
                    None,
                    None,
                    Shared::clone(&self.settings),
                )) as SharedMut<dyn PricingEngine>);
            let npv = swap.npv().unwrap();
            let fixed_leg_bps = swap.leg_bps(1).unwrap();
            let fair_rate = 0.04 - npv / (fixed_leg_bps / 1.0e-4);

            shared_mut(
                CapFloor::cap(coupons, vec![fair_rate], Shared::clone(&self.settings)).unwrap(),
            )
        }

        /// Prices a cap through the Black engine over the fixture curve at `vol`,
        /// the market-value engine (`caphelper.cpp:75`).
        fn black_price_of(&self, cap: &SharedMut<CapFloor>, vol: Real) -> Real {
            let vol_handle: Handle<dyn Quote> =
                Handle::new(shared(SimpleQuote::new(vol)) as Shared<dyn Quote>);
            let engine = shared_mut(
                BlackCapFloorEngine::with_flat_vol(
                    self.curve.clone(),
                    vol_handle,
                    Actual365Fixed::new(),
                    0.0,
                    Shared::clone(&self.settings),
                )
                .unwrap(),
            ) as SharedMut<dyn PricingEngine>;
            cap.borrow_mut().base_mut().set_pricing_engine(engine);
            cap.borrow_mut().npv().unwrap()
        }

        fn hw_model(&self) -> SharedMut<HullWhite> {
            HullWhite::new(self.curve.clone(), A, SIGMA).unwrap()
        }
    }

    /// The market-side pin: the helper's `market_value` (which caches
    /// `black_price(VOL)`) equals an independently-built ATM cap priced through
    /// the same Black engine at the same vol and curve, to machine precision. This
    /// pins `performCalculations` (schedules, fair rate, ATM cap) and `black_price`
    /// without any model engine.
    #[test]
    fn market_value_matches_an_independently_built_black_cap() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(true, years(5));

        let market = helper.market_value().unwrap();
        let reference = fixture.black_price_of(&fixture.reference_cap(true, years(5)), VOL);

        assert!(
            (market - reference).abs() <= 1.0e-12,
            "market {market} vs independently-built black cap {reference} (error {})",
            (market - reference).abs()
        );
    }

    /// The same pin on the `include_first_swaplet = false` schedule, whose start
    /// date is one index tenor later.
    #[test]
    fn market_value_matches_the_reference_without_the_first_swaplet() {
        let fixture = Fixture::new();
        let mut helper = fixture.helper(false, years(5));

        let market = helper.market_value().unwrap();
        let reference = fixture.black_price_of(&fixture.reference_cap(false, years(5)), VOL);

        assert!(
            (market - reference).abs() <= 1.0e-12,
            "market {market} vs reference {reference} (error {})",
            (market - reference).abs()
        );
    }

    /// Confirm-by-stubbing (a): perturbing the cap length moves the market value,
    /// so it genuinely reflects the built instrument rather than a constant.
    #[test]
    fn market_value_moves_when_the_length_is_perturbed() {
        let fixture = Fixture::new();
        let mut short = fixture.helper(true, years(2));
        let mut long = fixture.helper(true, years(3));

        let short_value = short.market_value().unwrap();
        let long_value = long.market_value().unwrap();

        assert!(
            (long_value - short_value).abs() > 1.0e-8,
            "market value did not move when the length was perturbed: \
             2y {short_value} vs 3y {long_value}"
        );
    }

    /// Confirm-by-stubbing (b): `include_first_swaplet` changes the schedule start
    /// and so the value.
    #[test]
    fn including_the_first_swaplet_changes_the_value() {
        let fixture = Fixture::new();
        let mut with = fixture.helper(true, years(5));
        let mut without = fixture.helper(false, years(5));

        let with_value = with.market_value().unwrap();
        let without_value = without.market_value().unwrap();

        assert!(
            (with_value - without_value).abs() > 1.0e-8,
            "the first-swaplet flag did not change the value: \
             with {with_value} vs without {without_value}"
        );
    }

    /// The Normal/Bachelier deferral (`caphelper.cpp:78-81`): `black_price` under
    /// [`VolatilityType::Normal`] returns an error naming the unported engine
    /// rather than silently pricing through the lognormal one.
    #[test]
    fn normal_volatility_black_price_is_a_visible_deferral() {
        let fixture = Fixture::new();
        let vol: Handle<dyn Quote> =
            Handle::new(shared(SimpleQuote::new(0.01)) as Shared<dyn Quote>);
        let helper = CapHelper::new(
            years(5),
            vol,
            Shared::clone(&fixture.index),
            fixed_leg_frequency(),
            fixed_leg_day_counter(),
            true,
            fixture.curve.clone(),
            CalibrationErrorType::RelativePriceError,
            VolatilityType::Normal,
            0.0,
        );

        let err = helper.black_price(0.01).unwrap_err();
        assert!(
            err.message().contains("BachelierCapFloorEngine"),
            "unexpected error message: {}",
            err.message()
        );
    }

    /// The calibration-runs wiring oracle: Hull-White, pricing caps through the
    /// analytic cap engine (#438), calibrates to a strip of CapHelpers via
    /// Levenberg-Marquardt. There is no cached target, so the assertion is that
    /// the fit converges and the aggregate calibration residual shrinks (LM
    /// minimises the aggregate root-sum-of-squares, so an individual helper's
    /// error may grow even as the total falls).
    #[test]
    fn hull_white_calibrates_to_a_cap_strip() {
        let fixture = Fixture::new();
        let model = fixture.hw_model();
        let engine = shared_mut(AnalyticCapFloorEngine::new(
            SharedMut::clone(&model),
            Shared::clone(&fixture.settings),
        )) as SharedMut<dyn PricingEngine>;

        let helpers: Vec<SharedMut<dyn CalibrationHelper>> = [2, 3, 4, 5]
            .into_iter()
            .map(|length| {
                let mut helper = fixture.helper(true, years(length));
                helper
                    .base_mut()
                    .set_pricing_engine(SharedMut::clone(&engine));
                shared_mut(helper) as SharedMut<dyn CalibrationHelper>
            })
            .collect();

        let initial = model.borrow().calibrated_model().params();
        let pre = calibration_value(&model, &initial, &helpers).unwrap();

        let mut method = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false);
        let end_criteria = EndCriteria::new(10000, Some(100), 1e-6, 1e-8, Some(1e-8)).unwrap();
        calibrate(
            &model,
            &helpers,
            &mut method,
            &end_criteria,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let fitted = model.borrow().calibrated_model().params();
        let post = calibration_value(&model, &fitted, &helpers).unwrap();
        let ec_type = model.borrow().calibrated_model().end_criteria();

        assert!(
            ec_type.succeeded(),
            "end criteria {ec_type} did not converge"
        );
        assert!(
            post.is_finite() && post < pre,
            "aggregate residual did not shrink: pre {pre} -> post {post}"
        );
    }
}
