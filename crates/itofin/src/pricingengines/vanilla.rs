//! Analytic pricing engines for vanilla options.
//!
//! Port of `ql/pricingengines/vanilla/analyticeuropeanengine.{hpp,cpp}`:
//! [`AnalyticEuropeanEngine`] wires a
//! [`GeneralizedBlackScholesProcess`] through the
//! [`BlackCalculator`] to fill a
//! [`VanillaOption`](crate::instruments::VanillaOption)'s results - the NPV
//! plus the full greek set. The engine's own inputs (the process) invalidate
//! the attached instrument through the usual observable chain.
//!
//! Deviations, documented per D10:
//! - The C++ second constructor taking a separate discounting curve is
//!   follow-up work; the risk-free curve embedded in the process is used for
//!   both forecasting and discounting, as with the C++ default constructor.
//! - The C++ engine accepts any `StrikedTypePayoff` and lets the calculator
//!   visit the concrete type; only [`PlainVanillaPayoff`] exists in the crate
//!   (and the calculator supports nothing else), so any other payoff is an
//!   explicit error instead of a silently wrong price.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::fail;
use crate::instruments::{
    Greeks, MoreGreeks, OneAssetOptionEngine, OneAssetOptionResults, OptionArguments,
    PlainVanillaPayoff, StrikedTypePayoff,
};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::BlackCalculator;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared};
use crate::types::Real;

/// Pricing engine for European vanilla options using analytical formulae.
pub struct AnalyticEuropeanEngine {
    base: OneAssetOptionEngine,
    process: Shared<GeneralizedBlackScholesProcess>,
}

impl AnalyticEuropeanEngine {
    /// Builds the engine on a Black-Scholes process whose risk-free rate is
    /// used for both forecasting and discounting.
    pub fn new(process: Shared<GeneralizedBlackScholesProcess>) -> AnalyticEuropeanEngine {
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(process.observable());
        AnalyticEuropeanEngine { base, process }
    }
}

impl AsObservable for AnalyticEuropeanEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for AnalyticEuropeanEngine {
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
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        if exercise.exercise_type() != ExerciseType::European {
            fail!("not an European option");
        }
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff: &dyn StrikedTypePayoff = &**payoff;
        let Some(payoff) = (payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("only plain-vanilla payoffs are supported");
        };
        let payoff = *payoff;
        let maturity_date = exercise.last_date();

        let risk_free = self.process.risk_free_rate().current_link()?;
        let dividend = self.process.dividend_yield().current_link()?;
        let black_vol = self.process.black_volatility().current_link()?;

        let variance = black_vol.black_variance_date(maturity_date, payoff.strike(), false)?;
        let dividend_discount = dividend.discount_date(maturity_date, false)?;
        let risk_free_discount = risk_free.discount_date(maturity_date, false)?;
        let df = risk_free_discount;
        let spot = self.process.state_variable().current_link()?.value()?;
        if spot.is_nan() || spot <= 0.0 {
            fail!("negative or null underlying given");
        }
        let forward_price = spot * dividend_discount / risk_free_discount;

        let black = BlackCalculator::with_payoff(&payoff, forward_price, variance.sqrt(), df)?;

        let value = black.value();
        let delta = black.delta(spot)?;
        let delta_forward = black.delta_forward();
        let elasticity = black.elasticity(spot)?;
        let gamma = black.gamma(spot)?;

        let rfdc = risk_free.require_day_counter()?;
        let divdc = dividend.require_day_counter()?;
        let voldc = black_vol.require_day_counter()?;

        let t = rfdc.year_fraction(risk_free.reference_date()?, maturity_date);
        let rho = black.rho(t)?;

        let t = divdc.year_fraction(dividend.reference_date()?, maturity_date);
        let dividend_rho = black.dividend_rho(t)?;

        let t = voldc.year_fraction(black_vol.reference_date()?, maturity_date);
        let vega = black.vega(t)?;
        let (theta, theta_per_day) = match black.theta(spot, t) {
            Ok(theta) => (Some(theta), Some(theta / 365.0)),
            Err(_) => (None, None),
        };

        let strike_sensitivity = black.strike_sensitivity();
        let itm_cash_probability = black.itm_cash_probability();

        let time_to_expiry = black_vol.time_from_reference(maturity_date)?;

        let results = self.base.results_mut();
        results.instrument.value = Some(value);
        results.greeks = Greeks {
            delta: Some(delta),
            gamma: Some(gamma),
            theta,
            vega: Some(vega),
            rho: Some(rho),
            dividend_rho: Some(dividend_rho),
        };
        results.more_greeks = MoreGreeks {
            itm_cash_probability: Some(itm_cash_probability),
            delta_forward: Some(delta_forward),
            elasticity: Some(elasticity),
            theta_per_day,
            strike_sensitivity: Some(strike_sensitivity),
        };
        let extras = &mut results.instrument.additional_results;
        let mut extra = |tag: &str, value: Real| {
            extras.insert(tag.to_string(), shared(value) as Shared<dyn Any>);
        };
        extra("spot", spot);
        extra("dividendDiscount", dividend_discount);
        extra("riskFreeDiscount", risk_free_discount);
        extra("forward", forward_price);
        extra("strike", payoff.strike());
        extra("volatility", (variance / time_to_expiry).sqrt());
        extra("timeToExpiry", time_to_expiry);
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod test_market {
    //! The flat market of `test-suite/europeanoption.cpp`: quote-backed flat
    //! curves on Actual360, shared by the engine oracle tests.

    use crate::exercise::EuropeanExercise;
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
    use crate::interestrate::Compounding;
    use crate::option::OptionType;
    use crate::pricingengine::PricingEngine;
    use crate::processes::BlackScholesMertonProcess;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Real, Time, Volatility};

    use super::AnalyticEuropeanEngine;
    use crate::processes::GeneralizedBlackScholesProcess;

    pub(crate) fn today() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// `timeToDays` from `test-suite/utilities.hpp`.
    pub(crate) fn time_to_days(t: Time) -> i32 {
        (t * 360.0).round() as i32
    }

    pub(crate) struct Market {
        pub(crate) settings: SharedMut<Settings<Date>>,
        pub(crate) spot: Shared<SimpleQuote>,
        pub(crate) q_rate: Shared<SimpleQuote>,
        pub(crate) r_rate: Shared<SimpleQuote>,
        pub(crate) vol: Shared<SimpleQuote>,
        pub(crate) process: Shared<GeneralizedBlackScholesProcess>,
    }

    impl Market {
        pub(crate) fn set(&self, spot: Real, q: Rate, r: Rate, vol: Volatility) {
            self.spot.set_value(spot);
            self.q_rate.set_value(q);
            self.r_rate.set_value(r);
            self.vol.set_value(vol);
        }

        pub(crate) fn option(
            &self,
            option_type: OptionType,
            strike: Real,
            expiry: Date,
        ) -> EuropeanOption {
            let payoff = shared(PlainVanillaPayoff::new(option_type, strike));
            let exercise = shared(EuropeanExercise::new(expiry));
            let mut option =
                EuropeanOption::new(payoff, exercise, SharedMut::clone(&self.settings)).unwrap();
            let engine = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&self.process)));
            option
                .base_mut()
                .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
            option
        }
    }

    fn quote_handle(quote: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
        Handle::new(Shared::clone(quote) as Shared<dyn Quote>)
    }

    fn flat_rate(reference: Date, quote: &Shared<SimpleQuote>) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::new(
            reference,
            quote_handle(quote),
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    fn flat_vol(reference: Date, quote: &Shared<SimpleQuote>) -> Shared<dyn BlackVolTermStructure> {
        shared(BlackConstantVol::with_quote(
            reference,
            None,
            quote_handle(quote),
            Actual360::new(),
        )) as Shared<dyn BlackVolTermStructure>
    }

    /// Fixed-reference flat market as built by `testValues` and
    /// `testGreekValues`.
    pub(crate) fn market() -> Market {
        let settings = shared_mut(Settings::new());
        settings.borrow_mut().set_evaluation_date(today());
        let spot = shared(SimpleQuote::new(0.0));
        let q_rate = shared(SimpleQuote::new(0.0));
        let r_rate = shared(SimpleQuote::new(0.0));
        let vol = shared(SimpleQuote::new(0.0));
        let process = shared(BlackScholesMertonProcess::new(
            quote_handle(&spot),
            Handle::new(flat_rate(today(), &q_rate)),
            Handle::new(flat_rate(today(), &r_rate)),
            Handle::new(flat_vol(today(), &vol)),
        ));
        Market {
            settings,
            spot,
            q_rate,
            r_rate,
            vol,
            process,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_market::{market, time_to_days, today};
    use crate::exercise::{Exercise, ExerciseType};
    use crate::instrument::Instrument;
    use crate::instruments::{StrikedTypePayoff, TypePayoff};
    use crate::option::OptionType;
    use crate::payoff::Payoff;
    use crate::time::date::Date;
    use crate::types::Real;

    #[test]
    fn call_and_put_satisfy_put_call_parity() {
        let market = market();
        market.set(100.0, 0.04, 0.06, 0.20);
        let expiry = today() + time_to_days(1.0);
        let mut call = market.option(OptionType::Call, 100.0, expiry);
        let mut put = market.option(OptionType::Put, 100.0, expiry);

        let call_value = call.npv().unwrap();
        let put_value = put.npv().unwrap();
        assert!(call_value > 0.0 && put_value > 0.0);

        let t: Real = 1.0;
        let parity = 100.0 * (-0.04_f64 * t).exp() - 100.0 * (-0.06_f64 * t).exp();
        assert!((call_value - put_value - parity).abs() < 1e-14);
    }

    #[test]
    fn additional_results_expose_the_market_snapshot() {
        let market = market();
        market.set(100.0, 0.04, 0.06, 0.20);
        let mut option = market.option(OptionType::Call, 100.0, today() + 360);

        let forward: Real = option.result("forward").unwrap();
        assert!((forward - 100.0 * (0.02_f64).exp()).abs() < 1e-12);
        let time_to_expiry: Real = option.result("timeToExpiry").unwrap();
        assert_eq!(time_to_expiry, 1.0);
        let volatility: Real = option.result("volatility").unwrap();
        assert!((volatility - 0.20).abs() < 1e-15);
        let strike: Real = option.result("strike").unwrap();
        assert_eq!(strike, 100.0);
    }

    #[test]
    fn quote_changes_invalidate_and_reprice() {
        let market = market();
        market.set(100.0, 0.04, 0.06, 0.20);
        let mut option = market.option(OptionType::Call, 100.0, today() + 360);

        let before = option.npv().unwrap();
        market.vol.set_value(0.30);
        assert!(!option.base().is_calculated());
        let after = option.npv().unwrap();
        assert!(after > before, "higher vol must raise the option value");
    }

    #[test]
    fn non_positive_spot_is_rejected() {
        let market = market();
        market.set(0.0, 0.04, 0.06, 0.20);
        let mut option = market.option(OptionType::Call, 100.0, today() + 360);
        assert_eq!(
            option.npv().unwrap_err().message(),
            "negative or null underlying given"
        );
    }

    #[test]
    fn non_european_exercise_is_rejected() {
        struct AmericanStub {
            dates: [Date; 1],
        }
        impl Exercise for AmericanStub {
            fn exercise_type(&self) -> ExerciseType {
                ExerciseType::American
            }
            fn dates(&self) -> &[Date] {
                &self.dates
            }
        }

        let market = market();
        market.set(100.0, 0.04, 0.06, 0.20);
        let option = market.option(OptionType::Call, 100.0, today() + 360);
        let payoff = option.payoff().clone();
        let exercise = crate::shared::shared(AmericanStub {
            dates: [today() + 360],
        }) as crate::shared::Shared<dyn Exercise>;
        let mut american = crate::instruments::OneAssetOption::new(
            payoff,
            exercise,
            crate::shared::SharedMut::clone(&market.settings),
        )
        .unwrap();
        american
            .base_mut()
            .set_pricing_engine(option.base().pricing_engine().unwrap().clone());
        assert_eq!(
            american.npv().unwrap_err().message(),
            "not an European option"
        );
    }

    #[test]
    fn non_plain_vanilla_payoffs_are_rejected() {
        struct CashOrNothingStub;
        impl Payoff for CashOrNothingStub {
            fn name(&self) -> String {
                "CashOrNothing".to_string()
            }
            fn description(&self) -> String {
                "stub".to_string()
            }
            fn value(&self, _price: Real) -> Real {
                0.0
            }
        }
        impl TypePayoff for CashOrNothingStub {
            fn option_type(&self) -> OptionType {
                OptionType::Call
            }
        }
        impl StrikedTypePayoff for CashOrNothingStub {
            fn strike(&self) -> Real {
                100.0
            }
        }

        let market = market();
        market.set(100.0, 0.04, 0.06, 0.20);
        let template = market.option(OptionType::Call, 100.0, today() + 360);
        let mut option = crate::instruments::OneAssetOption::new(
            crate::shared::shared(CashOrNothingStub),
            template.exercise().clone(),
            crate::shared::SharedMut::clone(&market.settings),
        )
        .unwrap();
        option
            .base_mut()
            .set_pricing_engine(template.base().pricing_engine().unwrap().clone());
        assert_eq!(
            option.npv().unwrap_err().message(),
            "only plain-vanilla payoffs are supported"
        );
    }
}
