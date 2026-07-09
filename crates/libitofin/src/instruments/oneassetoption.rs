//! Options on a single asset.
//!
//! Port of the vanilla-option slice of `ql/option.hpp`,
//! `ql/instruments/oneassetoption.{hpp,cpp}`, `ql/instruments/vanillaoption.hpp`
//! and `ql/instruments/europeanoption.hpp`: [`OptionArguments`] (the C++
//! `Option::arguments`), the [`Greeks`] and [`MoreGreeks`] result mix-ins,
//! [`OneAssetOptionResults`] and the [`OneAssetOption`] instrument with its
//! greek accessors.
//!
//! Deviations, all by existing design decisions:
//! - C++ stores the payoff as the base `Payoff` and engines `dynamic_cast` it
//!   to `StrikedTypePayoff`; trait objects do not cross-cast, so the arguments
//!   carry [`StrikedTypePayoff`] directly - every consumer in scope (the
//!   analytic engine, the greeks) needs the strike.
//! - `VanillaOption` and `EuropeanOption` add only constructor sugar on top of
//!   `OneAssetOption` until `impliedVolatility` (solver-backed, follow-up) and
//!   early-exercise instruments arrive, so both are aliases of it here.
//! - `isExpired` reads the evaluation date from the `Settings` handle the
//!   option is constructed with (per D5 there is no singleton). QuantLib's
//!   singleton always falls back to today's clock date, which the core lacks:
//!   with no evaluation date set the check fails with an explicit error
//!   instead of guessing (D10 - silently treating the option as live could
//!   price an expired option).
//!   The general `Event`/`simple_event` machinery of `ql/event.hpp` is
//!   follow-up work; its `hasOccurred` date comparison is ported inline.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::Exercise;
use crate::fail;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::instruments::StrikedTypePayoff;
use crate::pricingengine::{Arguments, GenericEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::types::Real;

/// Basic option arguments (the C++ `Option::arguments`).
#[derive(Default)]
pub struct OptionArguments {
    /// The payoff the option is written on.
    pub payoff: Option<Shared<dyn StrikedTypePayoff>>,
    /// The exercise schedule.
    pub exercise: Option<Shared<dyn Exercise>>,
}

impl Arguments for OptionArguments {
    fn validate(&self) -> QlResult<()> {
        if self.payoff.is_none() {
            fail!("no payoff given");
        }
        if self.exercise.is_none() {
            fail!("no exercise given");
        }
        Ok(())
    }
}

/// Additional option results (the C++ `Greeks` mix-in).
#[derive(Clone, Copy, Debug, Default)]
pub struct Greeks {
    pub delta: Option<Real>,
    pub gamma: Option<Real>,
    pub theta: Option<Real>,
    pub vega: Option<Real>,
    pub rho: Option<Real>,
    pub dividend_rho: Option<Real>,
}

impl Greeks {
    /// Clears all greeks (the C++ `reset`).
    pub fn reset(&mut self) {
        *self = Greeks::default();
    }
}

/// More additional option results (the C++ `MoreGreeks` mix-in).
#[derive(Clone, Copy, Debug, Default)]
pub struct MoreGreeks {
    pub itm_cash_probability: Option<Real>,
    pub delta_forward: Option<Real>,
    pub elasticity: Option<Real>,
    pub theta_per_day: Option<Real>,
    pub strike_sensitivity: Option<Real>,
}

impl MoreGreeks {
    /// Clears all greeks (the C++ `reset`).
    pub fn reset(&mut self) {
        *self = MoreGreeks::default();
    }
}

/// Results from a single-asset option calculation (the C++
/// `OneAssetOption::results`: instrument results plus both greek mix-ins).
#[derive(Default)]
pub struct OneAssetOptionResults {
    pub instrument: InstrumentResults,
    pub greeks: Greeks,
    pub more_greeks: MoreGreeks,
}

impl Results for OneAssetOptionResults {
    fn reset(&mut self) {
        self.instrument.reset();
        self.greeks.reset();
        self.more_greeks.reset();
    }

    fn as_instrument_results(&self) -> Option<&InstrumentResults> {
        Some(&self.instrument)
    }
}

/// Engine base for single-asset options (the C++ `OneAssetOption::engine`).
pub type OneAssetOptionEngine = GenericEngine<OptionArguments, OneAssetOptionResults>;

/// Option on a single asset.
///
/// Holds a payoff and an exercise schedule, prices through the attached
/// engine and exposes the fetched greeks. The greek accessors trigger the
/// lazy calculation and, as in C++, fail when the engine did not provide the
/// requested value.
pub struct OneAssetOption {
    base: InstrumentBase,
    payoff: Shared<dyn StrikedTypePayoff>,
    exercise: Shared<dyn Exercise>,
    settings: Shared<Settings<Date>>,
    greeks: Greeks,
    more_greeks: MoreGreeks,
}

/// Vanilla option (no discrete dividends, no barriers) on a single asset.
///
/// `impliedVolatility` is follow-up work (it needs the solver layer and a
/// Black-Scholes process); until it lands the C++ `VanillaOption` adds
/// nothing to `OneAssetOption`.
pub type VanillaOption = OneAssetOption;

/// European option on a single asset.
pub type EuropeanOption = VanillaOption;

impl OneAssetOption {
    /// Builds the option from its payoff and exercise schedule.
    ///
    /// The C++ `Instrument` constructor registers with the `Settings`
    /// evaluation date; per D5 the settings are passed explicitly and the
    /// registration happens here, so an evaluation-date change invalidates
    /// the cached results.
    pub fn new(
        payoff: Shared<dyn StrikedTypePayoff>,
        exercise: Shared<dyn Exercise>,
        settings: Shared<Settings<Date>>,
    ) -> OneAssetOption {
        let base = InstrumentBase::new();
        settings.register_eval_date_observer(&base.observer());
        OneAssetOption {
            base,
            payoff,
            exercise,
            settings,
            greeks: Greeks::default(),
            more_greeks: MoreGreeks::default(),
        }
    }

    /// The payoff the option is written on.
    pub fn payoff(&self) -> &Shared<dyn StrikedTypePayoff> {
        &self.payoff
    }

    /// The exercise schedule.
    pub fn exercise(&self) -> &Shared<dyn Exercise> {
        &self.exercise
    }

    fn greek(value: Option<Real>, description: &str) -> QlResult<Real> {
        let Some(value) = value else {
            fail!("{description} not provided");
        };
        Ok(value)
    }

    /// The option delta.
    pub fn delta(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.delta, "delta")
    }

    /// The option forward delta.
    pub fn delta_forward(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.delta_forward, "forward delta")
    }

    /// The option elasticity.
    pub fn elasticity(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.elasticity, "elasticity")
    }

    /// The option gamma.
    pub fn gamma(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.gamma, "gamma")
    }

    /// The option theta.
    pub fn theta(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.theta, "theta")
    }

    /// The option per-day theta.
    pub fn theta_per_day(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.theta_per_day, "theta per-day")
    }

    /// The option vega.
    pub fn vega(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.vega, "vega")
    }

    /// The option rho.
    pub fn rho(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.rho, "rho")
    }

    /// The option dividend rho.
    pub fn dividend_rho(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.greeks.dividend_rho, "dividend rho")
    }

    /// The sensitivity of the option value to the strike.
    pub fn strike_sensitivity(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(self.more_greeks.strike_sensitivity, "strike sensitivity")
    }

    /// The probability of the option expiring in the money.
    pub fn itm_cash_probability(&mut self) -> QlResult<Real> {
        self.calculate()?;
        Self::greek(
            self.more_greeks.itm_cash_probability,
            "in-the-money cash probability",
        )
    }
}

impl Instrument for OneAssetOption {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    /// Whether the last exercise date has occurred relative to the evaluation
    /// date (`detail::simple_event(exercise_->lastDate()).hasOccurred()`).
    fn is_expired(&self) -> QlResult<bool> {
        let Some(evaluation_date) = self.settings.evaluation_date() else {
            fail!("evaluation date not set");
        };
        let last_date = self.exercise.last_date();
        Ok(if self.settings.include_reference_date_events() {
            last_date < evaluation_date
        } else {
            last_date <= evaluation_date
        })
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(arguments) = (arguments as &mut dyn Any).downcast_mut::<OptionArguments>() else {
            fail!("wrong argument type");
        };
        arguments.payoff = Some(Shared::clone(&self.payoff));
        arguments.exercise = Some(Shared::clone(&self.exercise));
        Ok(())
    }

    fn setup_expired(&mut self) {
        let expired = InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        };
        self.base_mut().store_results(&expired);
        self.greeks = Greeks {
            delta: Some(0.0),
            gamma: Some(0.0),
            theta: Some(0.0),
            vega: Some(0.0),
            rho: Some(0.0),
            dividend_rho: Some(0.0),
        };
        self.more_greeks = MoreGreeks {
            itm_cash_probability: Some(0.0),
            delta_forward: Some(0.0),
            elasticity: Some(0.0),
            theta_per_day: Some(0.0),
            strike_sensitivity: Some(0.0),
        };
    }

    /// Reads the greeks back alongside the instrument-level results.
    ///
    /// As in C++, the values are copied without null checks: what to do about
    /// missing greeks is decided by the accessors (throw) or derived options.
    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let Some(results) = (results as &dyn Any).downcast_ref::<OneAssetOptionResults>() else {
            fail!("no greeks returned from pricing engine");
        };
        self.greeks = results.greeks;
        self.more_greeks = results.more_greeks;
        self.base_mut().store_results(&results.instrument);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    use crate::exercise::EuropeanExercise;
    use crate::instruments::PlainVanillaPayoff;
    use crate::option::OptionType;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::pricingengine::PricingEngine;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::time::date::Month;

    const SPOT: Real = 105.0;

    struct StubEngine {
        base: OneAssetOptionEngine,
        calculations: Shared<Cell<usize>>,
        provide_greeks: bool,
    }

    impl AsObservable for StubEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for StubEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }

        fn results(&self) -> &dyn Results {
            self.base.results()
        }

        fn reset(&mut self) {
            self.base.reset();
        }

        fn calculate(&mut self) -> QlResult<()> {
            self.calculations.set(self.calculations.get() + 1);
            let payoff = Shared::clone(self.base.arguments().payoff.as_ref().expect("validated"));
            let provide_greeks = self.provide_greeks;
            let results = self.base.results_mut();
            results.instrument.value = Some(payoff.value(SPOT));
            if provide_greeks {
                results.greeks = Greeks {
                    delta: Some(0.1),
                    gamma: Some(0.2),
                    theta: Some(0.3),
                    vega: Some(0.4),
                    rho: Some(0.5),
                    dividend_rho: Some(0.6),
                };
                results.more_greeks = MoreGreeks {
                    itm_cash_probability: Some(0.7),
                    delta_forward: Some(0.8),
                    elasticity: Some(0.9),
                    theta_per_day: Some(1.1),
                    strike_sensitivity: Some(1.2),
                };
            }
            Ok(())
        }
    }

    fn stub_engine(provide_greeks: bool) -> (SharedMut<StubEngine>, Shared<Cell<usize>>) {
        let calculations = shared(Cell::new(0_usize));
        let engine = shared_mut(StubEngine {
            base: OneAssetOptionEngine::new(
                OptionArguments::default(),
                OneAssetOptionResults::default(),
            ),
            calculations: Shared::clone(&calculations),
            provide_greeks,
        });
        (engine, calculations)
    }

    fn european_call(settings: &Shared<Settings<Date>>) -> EuropeanOption {
        let payoff = shared(PlainVanillaPayoff::new(OptionType::Call, 100.0));
        let exercise = shared(EuropeanExercise::new(Date::new(7, Month::July, 2027)));
        EuropeanOption::new(payoff, exercise, Shared::clone(settings))
    }

    fn settings_at(date: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(date);
        settings
    }

    /// The ticket's oracle: instantiation plus a stub-engine round trip of the
    /// NPV and every greek.
    #[test]
    fn european_option_round_trips_npv_and_greeks_through_a_stub_engine() {
        let settings = settings_at(Date::new(7, Month::July, 2026));
        let mut option = european_call(&settings);
        let (engine, calculations) = stub_engine(true);
        option.base_mut().set_pricing_engine(engine);

        assert_eq!(option.npv().unwrap(), 5.0);
        assert_eq!(option.delta().unwrap(), 0.1);
        assert_eq!(option.gamma().unwrap(), 0.2);
        assert_eq!(option.theta().unwrap(), 0.3);
        assert_eq!(option.vega().unwrap(), 0.4);
        assert_eq!(option.rho().unwrap(), 0.5);
        assert_eq!(option.dividend_rho().unwrap(), 0.6);
        assert_eq!(option.itm_cash_probability().unwrap(), 0.7);
        assert_eq!(option.delta_forward().unwrap(), 0.8);
        assert_eq!(option.elasticity().unwrap(), 0.9);
        assert_eq!(option.theta_per_day().unwrap(), 1.1);
        assert_eq!(option.strike_sensitivity().unwrap(), 1.2);
        assert_eq!(calculations.get(), 1, "accessors must hit the cache");

        assert_eq!(option.payoff().strike(), 100.0);
        assert_eq!(
            option.exercise().last_date(),
            Date::new(7, Month::July, 2027)
        );
    }

    #[test]
    fn arguments_validation_requires_payoff_and_exercise() {
        let mut arguments = OptionArguments::default();
        assert_eq!(
            arguments.validate().unwrap_err().message(),
            "no payoff given"
        );

        arguments.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)));
        assert_eq!(
            arguments.validate().unwrap_err().message(),
            "no exercise given"
        );

        arguments.exercise = Some(shared(EuropeanExercise::new(Date::new(
            7,
            Month::July,
            2027,
        ))));
        assert!(arguments.validate().is_ok());
    }

    #[test]
    fn setup_arguments_fills_payoff_and_exercise() {
        let settings = settings_at(Date::new(7, Month::July, 2026));
        let option = european_call(&settings);
        let mut arguments = OptionArguments::default();
        option.setup_arguments(&mut arguments).unwrap();

        let payoff = arguments.payoff.expect("payoff filled");
        assert_eq!(payoff.option_type(), OptionType::Call);
        assert_eq!(payoff.strike(), 100.0);
        let exercise = arguments.exercise.expect("exercise filled");
        assert_eq!(exercise.last_date(), Date::new(7, Month::July, 2027));
    }

    #[test]
    fn wrong_argument_bundle_is_reported() {
        struct OtherArguments;
        impl Arguments for OtherArguments {
            fn validate(&self) -> QlResult<()> {
                Ok(())
            }
        }

        let settings = settings_at(Date::new(7, Month::July, 2026));
        let option = european_call(&settings);
        let err = option.setup_arguments(&mut OtherArguments).unwrap_err();
        assert_eq!(err.message(), "wrong argument type");
    }

    /// The C++ "slim engine" case: only the value is provided, greek
    /// accessors report what is missing.
    #[test]
    fn slim_engine_prices_but_reports_missing_greeks() {
        let settings = settings_at(Date::new(7, Month::July, 2026));
        let mut option = european_call(&settings);
        let (engine, _) = stub_engine(false);
        option.base_mut().set_pricing_engine(engine);

        assert_eq!(option.npv().unwrap(), 5.0);
        assert_eq!(option.delta().unwrap_err().message(), "delta not provided");
        assert_eq!(
            option.delta_forward().unwrap_err().message(),
            "forward delta not provided"
        );
        assert_eq!(
            option.theta_per_day().unwrap_err().message(),
            "theta per-day not provided"
        );
        assert_eq!(
            option.itm_cash_probability().unwrap_err().message(),
            "in-the-money cash probability not provided"
        );
    }

    struct GreeksFreeEngine {
        base: GenericEngine<OptionArguments, InstrumentResults>,
    }

    impl AsObservable for GreeksFreeEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for GreeksFreeEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }

        fn results(&self) -> &dyn Results {
            self.base.results()
        }

        fn reset(&mut self) {
            self.base.reset();
        }

        fn calculate(&mut self) -> QlResult<()> {
            self.base.results_mut().value = Some(1.0);
            Ok(())
        }
    }

    /// The C++ `QL_ENSURE(results != nullptr, "no greeks returned...")`.
    #[test]
    fn greeks_free_result_bundle_is_rejected() {
        let settings = settings_at(Date::new(7, Month::July, 2026));
        let mut option = european_call(&settings);
        let engine = shared_mut(GreeksFreeEngine {
            base: GenericEngine::new(OptionArguments::default(), InstrumentResults::default()),
        });
        option.base_mut().set_pricing_engine(engine);

        let err = option.npv().unwrap_err();
        assert_eq!(err.message(), "no greeks returned from pricing engine");
    }

    #[test]
    fn expired_option_zeroes_value_and_greeks_without_pricing() {
        let settings = settings_at(Date::new(7, Month::July, 2027));
        let mut option = european_call(&settings);
        let (engine, calculations) = stub_engine(true);
        option.base_mut().set_pricing_engine(engine);

        assert!(option.is_expired().unwrap(), "expiry day counts as expired");
        assert_eq!(option.npv().unwrap(), 0.0);
        assert_eq!(option.delta().unwrap(), 0.0);
        assert_eq!(option.strike_sensitivity().unwrap(), 0.0);
        assert_eq!(option.itm_cash_probability().unwrap(), 0.0);
        assert_eq!(calculations.get(), 0, "expired options never price");
    }

    #[test]
    fn include_reference_date_events_keeps_expiry_day_alive() {
        let settings = settings_at(Date::new(7, Month::July, 2027));
        settings.set_include_reference_date_events(true);
        let mut option = european_call(&settings);
        let (engine, calculations) = stub_engine(true);
        option.base_mut().set_pricing_engine(engine);

        assert!(!option.is_expired().unwrap());
        assert_eq!(option.npv().unwrap(), 5.0);
        assert_eq!(calculations.get(), 1);
    }

    /// QuantLib's singleton always supplies today's date; the port has no
    /// clock, so a floating evaluation date fails the expiry check and the
    /// calculation with it (D10: explicit error over silently-live).
    #[test]
    fn unset_evaluation_date_fails_the_expiry_check_and_pricing() {
        let settings = shared(Settings::new());
        let mut option = european_call(&settings);
        assert_eq!(
            option.is_expired().unwrap_err().message(),
            "evaluation date not set"
        );

        let (engine, calculations) = stub_engine(true);
        option.base_mut().set_pricing_engine(engine);
        assert_eq!(
            option.npv().unwrap_err().message(),
            "evaluation date not set"
        );
        assert_eq!(calculations.get(), 0, "pricing must not run blind");
    }

    /// The C++ `Instrument` constructor's `registerWith(evaluation date)`,
    /// wired through the settings handle the option is built with.
    #[test]
    fn evaluation_date_change_invalidates_cached_results() {
        let settings = settings_at(Date::new(7, Month::July, 2026));
        let mut option = european_call(&settings);
        let (engine, calculations) = stub_engine(true);
        option.base_mut().set_pricing_engine(engine);

        option.npv().unwrap();
        settings.set_evaluation_date(Date::new(8, Month::July, 2026));
        assert!(!option.base().is_calculated());
        option.npv().unwrap();
        assert_eq!(calculations.get(), 2);
    }
}
