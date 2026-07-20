//! Heston-model calibration helper.
//!
//! Port of `ql/models/equity/hestonmodelhelper.{hpp,cpp}`.
//! [`HestonModelHelper`] is a
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper):
//! it prices a European vanilla option's market value from a quoted Black
//! volatility over a flat-vol surface, and its model value through the model
//! pricing engine a calibration installs (an
//! [`AnalyticHestonEngine`](crate::pricingengines::AnalyticHestonEngine)).
//!
//! ## Ported surface
//!
//! Both constructors (`hestonmodelhelper.cpp:33-62`): [`new`](HestonModelHelper::new)
//! takes the spot as a bare [`Real`] and wraps it in a constant
//! [`SimpleQuote`] (`cpp:42`); [`with_spot_handle`](HestonModelHelper::with_spot_handle)
//! takes a [`Handle<Quote>`](Handle) spot directly (`cpp:48-62`).
//!
//! ## `addTimesTo`
//!
//! `addTimesTo` (`hestonmodelhelper.hpp:56`) is literally empty `{}` in C++ - the
//! helper has no tree/lattice pricing path. It is already omitted from the
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper)
//! trait surface (`calibrationhelper.rs` deferral of `addTimesTo`), so there is
//! nothing to implement: the empty C++ body maps to no Rust method at all.
//!
//! ## Divergences from QuantLib
//!
//! - **`Settings` is an explicit constructor argument.** C++ reads the global
//!   `Settings::instance()` when the built [`VanillaOption`] checks expiry; per
//!   D5 the core has no global, so the settings are passed in and reused for the
//!   option. The Heston engine takes its maturity time from the process curves
//!   (not the settings), so the settings only gate the option's expiry check;
//!   pass the same [`Settings`] the curves and engine are anchored to.
//! - **`model_value` / `black_price` are `&self` and recompute their derived
//!   state.** C++'s are const and lean on a `LazyObject::calculate()` that caches
//!   `mutable exerciseDate_ / tau_ / type_ / option_`. This port's
//!   [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper)
//!   fixes both as `&self`, so [`derive`](HestonModelHelper::derive) recomputes
//!   the exercise date, `tau`, the discounted strike/spot and the option type on
//!   each call (a few flat-curve discount lookups) and
//!   [`model_value`](HestonModelHelper::model_value) builds a fresh
//!   [`VanillaOption`]. `derive` is pure in the helper's inputs, so this is
//!   observationally identical to the C++ cache; observers are held weakly, so a
//!   fresh option per call does not leak.
//! - **A missing model engine is an explicit `Err`.** C++ `modelValue`
//!   dereferences a null `engine_`; the port returns an error (D4).
//! - **`black_price` uses the 6-arg [`black_formula`].** C++'s 4-arg overload
//!   (`cpp:89-91`) has discount `1.0` and displacement `0.0`; the strike and
//!   forward passed are already discounted, so this port passes `discount = 1.0`
//!   and `displacement = 0.0` (passing a curve discount here would double-count).

use crate::errors::QlResult;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::fail;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use crate::models::calibrationhelper::{
    BlackCalibrationHelper, BlackCalibrationHelperBase, CalibrationErrorType,
};
use crate::option::OptionType;
use crate::pricingengines::blackformula::black_formula;
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::VolatilityType;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Real, Time};

/// The state derived from the term structures on each valuation
/// (`performCalculations`, `hestonmodelhelper.cpp:64-78`).
struct Derived {
    exercise_date: Date,
    tau: Time,
    option_type: OptionType,
    discounted_strike: Real,
    discounted_spot: Real,
}

/// Calibration helper for the Heston model (`hestonmodelhelper.hpp:32`).
pub struct HestonModelHelper {
    base: BlackCalibrationHelperBase,
    maturity: Period,
    calendar: Calendar,
    s0: Handle<dyn Quote>,
    strike_price: Real,
    risk_free_rate: Handle<dyn YieldTermStructure>,
    dividend_yield: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
}

impl HestonModelHelper {
    /// Builds a helper from a bare spot (`hestonmodelhelper.cpp:33-45`): the spot
    /// is wrapped in a constant [`SimpleQuote`] and the helper delegates to
    /// [`with_spot_handle`](Self::with_spot_handle). C++ does not register with
    /// the constant spot; this port does (`with_spot_handle` registers all three
    /// handles), a no-op since a [`SimpleQuote`] built here never changes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        maturity: Period,
        calendar: Calendar,
        s0: Real,
        strike_price: Real,
        volatility: Handle<dyn Quote>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        error_type: CalibrationErrorType,
        settings: Shared<Settings<Date>>,
    ) -> HestonModelHelper {
        let s0: Handle<dyn Quote> = Handle::new(shared(SimpleQuote::new(s0)) as Shared<dyn Quote>);
        HestonModelHelper::with_spot_handle(
            maturity,
            calendar,
            s0,
            strike_price,
            volatility,
            risk_free_rate,
            dividend_yield,
            error_type,
            settings,
        )
    }

    /// Builds a helper from a quoted spot (`hestonmodelhelper.cpp:47-62`).
    ///
    /// Registers the base's observer with the spot, risk-free and dividend
    /// handles (C++ `registerWith(s0)`/`registerWith(riskFreeRate)`/
    /// `registerWith(dividendYield)`, `:58-60`), so a change to any invalidates
    /// the cached market value alongside the volatility handle the base
    /// registers. The base uses the default `ShiftedLognormal` volatility type
    /// and zero shift of the C++ 2-arg base constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn with_spot_handle(
        maturity: Period,
        calendar: Calendar,
        s0: Handle<dyn Quote>,
        strike_price: Real,
        volatility: Handle<dyn Quote>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        error_type: CalibrationErrorType,
        settings: Shared<Settings<Date>>,
    ) -> HestonModelHelper {
        let base = BlackCalibrationHelperBase::new(
            volatility,
            error_type,
            VolatilityType::ShiftedLognormal,
            0.0,
        );

        let observer = base.observer();
        s0.register_observer(&observer);
        risk_free_rate.register_observer(&observer);
        dividend_yield.register_observer(&observer);

        HestonModelHelper {
            base,
            maturity,
            calendar,
            s0,
            strike_price,
            risk_free_rate,
            dividend_yield,
            settings,
        }
    }

    /// The year fraction to the exercise date (`maturity()`,
    /// `hestonmodelhelper.hpp:60`): the `tau` of the current derived state.
    ///
    /// # Errors
    ///
    /// Propagates a failure of [`derive`](Self::derive) (an empty curve handle).
    pub fn maturity(&self) -> QlResult<Time> {
        Ok(self.derive()?.tau)
    }

    /// `performCalculations`'s derived state (`hestonmodelhelper.cpp:64-72`):
    /// advances the risk-free reference date by the maturity, times it, and
    /// selects the option type from forward moneyness. The type is
    /// [`Call`](OptionType::Call) when the discounted strike is at least the
    /// discounted spot, else [`Put`](OptionType::Put) (`cpp:68-71`).
    fn derive(&self) -> QlResult<Derived> {
        let risk_free = self.risk_free_rate.current_link()?;
        let reference_date = risk_free.reference_date()?;
        let exercise_date = self.calendar.advance_by_period(
            reference_date,
            self.maturity,
            BusinessDayConvention::Following,
            false,
        );
        let tau = risk_free.time_from_reference(exercise_date)?;

        let discounted_strike = self.strike_price * risk_free.discount(tau, false)?;
        let dividend = self.dividend_yield.current_link()?;
        let spot = self.s0.current_link()?.value()?;
        let discounted_spot = spot * dividend.discount(tau, false)?;

        let option_type = if discounted_strike >= discounted_spot {
            OptionType::Call
        } else {
            OptionType::Put
        };

        Ok(Derived {
            exercise_date,
            tau,
            option_type,
            discounted_strike,
            discounted_spot,
        })
    }

    /// Builds the vanilla option `performCalculations` assembles
    /// (`hestonmodelhelper.cpp:73-77`): a [`PlainVanillaPayoff`] of the derived
    /// type struck at `strikePrice_` over a [`EuropeanExercise`] on the exercise
    /// date.
    fn build_option(&self, derived: &Derived) -> SharedMut<VanillaOption> {
        let payoff = shared(PlainVanillaPayoff::new(
            derived.option_type,
            self.strike_price,
        )) as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(derived.exercise_date)) as Shared<dyn Exercise>;
        shared_mut(VanillaOption::new(
            payoff,
            exercise,
            Shared::clone(&self.settings),
        ))
    }
}

impl BlackCalibrationHelper for HestonModelHelper {
    fn base(&self) -> &BlackCalibrationHelperBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BlackCalibrationHelperBase {
        &mut self.base
    }

    /// `modelValue` (`hestonmodelhelper.cpp:80-84`): installs the model engine on
    /// the option and returns its NPV.
    fn model_value(&self) -> QlResult<Real> {
        let derived = self.derive()?;
        let option = self.build_option(&derived);
        let Some(engine) = self.base.pricing_engine() else {
            fail!("no model pricing engine set on the heston model helper");
        };
        option
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(SharedMut::clone(engine));
        let value = option.borrow_mut().npv()?;
        Ok(value)
    }

    /// `blackPrice` (`hestonmodelhelper.cpp:86-92`): the Black 1976 value at the
    /// given volatility, with the strike and forward already discounted, so the
    /// discount and displacement of the 6-arg formula are `1.0` and `0.0`.
    fn black_price(&self, volatility: Real) -> QlResult<Real> {
        let derived = self.derive()?;
        let std_dev = volatility * derived.tau.sqrt();
        black_formula(
            derived.option_type,
            derived.discounted_strike,
            derived.discounted_spot,
            std_dev,
            1.0,
            0.0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::math::optimization::endcriteria::EndCriteria;
    use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
    use crate::models::calibrationhelper::CalibrationHelper;
    use crate::models::{HestonModel, calibrate};
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
    use crate::processes::HestonProcess;
    use crate::termstructures::yields::FlatForward;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::timeunit::TimeUnit;

    const VOL: Real = 0.1;

    fn today() -> Date {
        Date::new(15, Month::January, 2026)
    }

    /// The `testBlackCalibration` market data (`hestonmodel.cpp:239-251`):
    /// Actual360 flat curves at 4% risk-free / 50% dividend referenced at the
    /// evaluation date, unit spot and a flat 10% volatility, on a
    /// [`NullCalendar`]. The 50% dividend yield drives forwards well below spot,
    /// so the out-of-the-money (positive-moneyness) helpers become puts.
    struct Fixture {
        settings: Shared<Settings<Date>>,
        risk_free: Handle<dyn YieldTermStructure>,
        dividend: Handle<dyn YieldTermStructure>,
        s0: Handle<dyn Quote>,
        vol: Handle<dyn Quote>,
        calendar: Calendar,
    }

    impl Fixture {
        fn new() -> Fixture {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(today());
            let flat = |rate: Real| -> Handle<dyn YieldTermStructure> {
                Handle::new(shared(FlatForward::with_rate(
                    today(),
                    rate,
                    Actual360::new(),
                    Compounding::Continuous,
                    Frequency::Annual,
                )) as Shared<dyn YieldTermStructure>)
            };
            Fixture {
                settings,
                risk_free: flat(0.04),
                dividend: flat(0.50),
                s0: Handle::new(shared(SimpleQuote::new(1.0)) as Shared<dyn Quote>),
                vol: Handle::new(shared(SimpleQuote::new(VOL)) as Shared<dyn Quote>),
                calendar: NullCalendar::new(),
            }
        }

        /// `tau` exactly as the helper derives it (`hestonmodelhelper.cpp:65-66`):
        /// the risk-free curve's time to the maturity-advanced reference date.
        fn tau(&self, maturity: Period) -> Time {
            let risk_free = self.risk_free.current_link().unwrap();
            let reference = risk_free.reference_date().unwrap();
            let exercise = self.calendar.advance_by_period(
                reference,
                maturity,
                BusinessDayConvention::Following,
                false,
            );
            risk_free.time_from_reference(exercise).unwrap()
        }

        /// The forward `s0 * dividendDiscount / riskFreeDiscount`
        /// (`hestonmodel.cpp:257-258`).
        fn forward(&self, tau: Time) -> Real {
            let risk_free = self.risk_free.current_link().unwrap();
            let dividend = self.dividend.current_link().unwrap();
            let s0 = self.s0.current_link().unwrap().value().unwrap();
            s0 * dividend.discount(tau, false).unwrap() / risk_free.discount(tau, false).unwrap()
        }

        fn helper(&self, maturity: Period, strike: Real) -> HestonModelHelper {
            HestonModelHelper::with_spot_handle(
                maturity,
                self.calendar.clone(),
                self.s0.clone(),
                strike,
                self.vol.clone(),
                self.risk_free.clone(),
                self.dividend.clone(),
                CalibrationErrorType::RelativePriceError,
                Shared::clone(&self.settings),
            )
        }

        /// A Black price computed independently of the helper, with the strike
        /// and forward discounted and a forced option type - the reference the
        /// standalone pins compare against (`hestonmodelhelper.cpp:89-91`).
        fn direct_black(
            &self,
            maturity: Period,
            strike: Real,
            option_type: OptionType,
            vol: Real,
        ) -> Real {
            let tau = self.tau(maturity);
            let risk_free = self.risk_free.current_link().unwrap();
            let dividend = self.dividend.current_link().unwrap();
            let s0 = self.s0.current_link().unwrap().value().unwrap();
            black_formula(
                option_type,
                strike * risk_free.discount(tau, false).unwrap(),
                s0 * dividend.discount(tau, false).unwrap(),
                vol * tau.sqrt(),
                1.0,
                0.0,
            )
            .unwrap()
        }
    }

    /// STANDALONE PIN: `black_price` equals a directly computed
    /// [`black_formula`] with the discounted strike/forward, and `market_value`
    /// equals `black_price` at the quoted volatility (the base-class contract).
    /// A 6-arg wiring, discounting or `tau` bug diverges this off the optimizer.
    /// The 6M at-the-forward helper (moneyness 0) has strike equal to the
    /// forward, so its `strike * rf_discount == s0 * div_discount` and the type
    /// is a call.
    #[test]
    fn black_price_matches_a_direct_black_formula_and_market_value() {
        let fixture = Fixture::new();
        let maturity = Period::new(6, TimeUnit::Months);
        let strike = fixture.forward(fixture.tau(maturity));
        let mut helper = fixture.helper(maturity, strike);

        let expected = fixture.direct_black(maturity, strike, OptionType::Call, VOL);
        let black = helper.black_price(VOL).unwrap();
        assert!(
            (black - expected).abs() <= 1.0e-12,
            "black_price {black} vs direct black_formula {expected} (error {})",
            (black - expected).abs()
        );

        let market = helper.market_value().unwrap();
        assert!(
            (market - black).abs() <= 1.0e-12,
            "market_value {market} vs black_price {black} (error {})",
            (market - black).abs()
        );
    }

    /// CONFIRM-BY-STUBBING: the `type_` decision (`hestonmodelhelper.cpp:68-71`)
    /// is live inside `black_price`. A strike far below the forward prices as a
    /// put (matches a forced-put `black_formula`, differs from a forced call);
    /// a strike far above prices as a call. Removing the moneyness branch would
    /// break one side of each pair.
    #[test]
    fn black_price_tracks_the_option_type_decision() {
        let fixture = Fixture::new();
        let maturity = Period::new(1, TimeUnit::Years);
        let forward = fixture.forward(fixture.tau(maturity));

        let put_strike = forward * 0.5;
        let put_black = fixture
            .helper(maturity, put_strike)
            .black_price(VOL)
            .unwrap();
        let forced_put = fixture.direct_black(maturity, put_strike, OptionType::Put, VOL);
        let forced_call = fixture.direct_black(maturity, put_strike, OptionType::Call, VOL);
        assert!(
            (put_black - forced_put).abs() <= 1.0e-12,
            "a strike below the forward must price as a put: {put_black} vs {forced_put}"
        );
        assert!(
            (put_black - forced_call).abs() > 1.0e-8,
            "the type decision is dead: the put priced as a forced call ({put_black})"
        );

        let call_strike = forward * 2.0;
        let call_black = fixture
            .helper(maturity, call_strike)
            .black_price(VOL)
            .unwrap();
        let forced_call = fixture.direct_black(maturity, call_strike, OptionType::Call, VOL);
        let forced_put = fixture.direct_black(maturity, call_strike, OptionType::Put, VOL);
        assert!(
            (call_black - forced_call).abs() <= 1.0e-12,
            "a strike above the forward must price as a call: {call_black} vs {forced_call}"
        );
        assert!(
            (call_black - forced_put).abs() > 1.0e-8,
            "the type decision is dead: the call priced as a forced put ({call_black})"
        );
    }

    /// ORACLE `testBlackCalibration` (`hestonmodel.cpp:232-311`): calibrate the
    /// Heston model to a flat 10% vol surface (21 helpers over 7 maturities x 3
    /// moneyness) for three starting vol-of-vols. A flat surface has no smile, so
    /// the fit drives sigma to zero and theta/v0 to the constant variance. The
    /// theta pin is C++'s deliberately weak `|kappa * (theta - vol^2)|` (a small
    /// kappa lets theta drift), ported faithfully. The distribution check
    /// confirms the put branch is genuinely exercised (7 puts at moneyness +1,
    /// 14 calls at moneyness <= 0).
    ///
    /// STILL BLOCKED after #425 - by a DIFFERENT deferral than #415 assumed.
    ///
    /// #425 ported the small-`sigma` chF series expansion
    /// (`analytichestonengine.cpp:584-617`), so the sigma=0.1 start now converges
    /// (sigma -> ~1e-7 through the series, no panic) and passes all three
    /// assertions. But this oracle also drives the sigma=0.3 and sigma=0.5 starts,
    /// and at their INITIAL cost - params `(kappa=0.2, theta=0.02, sigma>=0.3,
    /// rho=-0.75, v0=0.01)` - `optimalControlVariate`
    /// (`analytichestonengine.cpp:707-719`) selects `AsymptoticChF` for every
    /// maturity >= 2 months (the 1-month helper stays `AngledContour` as
    /// `tau ~ 0.086 < 0.15` fails condition 1). C++ prices `AsymptoticChF`
    /// (it implements the `Ci`/`Si` branch); the Rust port defers it to issue
    /// #418, where [`ApHelper::new`] fails loud, so those 18 helpers return NaN
    /// residuals and Levenberg-Marquardt aborts at the initial point. C++
    /// `testBlackCalibration` (`hestonmodel.cpp:280-283`) builds
    /// `AnalyticHestonEngine(model, 96)`, whose `cpxLog_` is `OptimalCV`
    /// (`analytichestonengine.cpp:666`), confirming the `AsymptoticChF` path is on
    /// this oracle. Un-ignore once #418 lands.
    #[test]
    #[ignore = "blocked on AsymptoticChF (issue #418): the sigma=0.3/0.5 starts select it at the initial cost (analytichestonengine.cpp:707-719); #425 ported the small-sigma chF but that only unblocks the sigma=0.1 start"]
    fn heston_calibrates_to_a_flat_vol_surface() {
        let fixture = Fixture::new();
        let maturities = [
            Period::new(1, TimeUnit::Months),
            Period::new(2, TimeUnit::Months),
            Period::new(3, TimeUnit::Months),
            Period::new(6, TimeUnit::Months),
            Period::new(9, TimeUnit::Months),
            Period::new(1, TimeUnit::Years),
            Period::new(2, TimeUnit::Years),
        ];
        let moneynesses = [-1.0, 0.0, 1.0];

        let mut helpers: Vec<SharedMut<HestonModelHelper>> = Vec::new();
        let mut puts = 0usize;
        let mut calls = 0usize;
        for &maturity in &maturities {
            for &moneyness in &moneynesses {
                let tau = fixture.tau(maturity);
                let strike = fixture.forward(tau) * (-moneyness * VOL * tau.sqrt()).exp();
                let helper = fixture.helper(maturity, strike);

                let black = helper.black_price(VOL).unwrap();
                let call = fixture.direct_black(maturity, strike, OptionType::Call, VOL);
                let put = fixture.direct_black(maturity, strike, OptionType::Put, VOL);
                if (black - put).abs() < (black - call).abs() {
                    puts += 1;
                } else {
                    calls += 1;
                }
                helpers.push(shared_mut(helper));
            }
        }
        assert_eq!(helpers.len(), 21);
        assert_eq!(
            puts, 7,
            "the 50% dividend fixture must exercise the put branch (moneyness +1)"
        );
        assert_eq!(calls, 14);

        let tolerance = 3.0e-3;
        let expected_variance = VOL * VOL;
        for &sigma in &[0.1, 0.3, 0.5] {
            let process = shared(HestonProcess::new(
                fixture.risk_free.clone(),
                fixture.dividend.clone(),
                fixture.s0.clone(),
                0.01,
                0.2,
                0.02,
                sigma,
                -0.75,
            ));
            let model = HestonModel::new(process).unwrap();
            let engine =
                shared_mut(AnalyticHestonEngine::new(SharedMut::clone(&model), 96).unwrap())
                    as SharedMut<dyn PricingEngine>;
            for helper in &helpers {
                helper
                    .borrow_mut()
                    .base_mut()
                    .set_pricing_engine(SharedMut::clone(&engine));
            }
            let dyn_helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
                .iter()
                .map(|helper| SharedMut::clone(helper) as SharedMut<dyn CalibrationHelper>)
                .collect();

            let mut method = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false);
            let end_criteria = EndCriteria::new(400, Some(40), 1e-8, 1e-8, Some(1e-8)).unwrap();
            calibrate(
                &model,
                &dyn_helpers,
                &mut method,
                &end_criteria,
                None,
                Vec::new(),
                Vec::new(),
            )
            .unwrap();

            let model = model.borrow();
            assert!(
                model.sigma() < tolerance,
                "sigma {} exceeds {tolerance} (start {sigma})",
                model.sigma()
            );
            let theta_residual = model.kappa() * (model.theta() - expected_variance);
            assert!(
                theta_residual.abs() < tolerance,
                "kappa*(theta - vol^2) = {theta_residual} exceeds {tolerance} (start {sigma})"
            );
            assert!(
                (model.v0() - expected_variance).abs() < tolerance,
                "v0 {} vs vol^2 {expected_variance} exceeds {tolerance} (start {sigma})",
                model.v0()
            );
        }
    }
}
