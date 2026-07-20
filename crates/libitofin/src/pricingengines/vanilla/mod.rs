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

pub mod analytichestonengine;

pub use analytichestonengine::HestonChf;

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

        let t = rfdc.year_fraction(risk_free.reference_date()?, maturity_date);
        let rho = black.rho(t)?;

        let t = divdc.year_fraction(dividend.reference_date()?, maturity_date);
        let dividend_rho = black.dividend_rho(t)?;

        let time_to_expiry = black_vol.time_from_reference(maturity_date)?;
        let vega = black.vega(time_to_expiry)?;
        let (theta, theta_per_day) = match black.theta(spot, time_to_expiry) {
            Ok(theta) => (Some(theta), Some(theta / 365.0)),
            Err(_) => (None, None),
        };

        let strike_sensitivity = black.strike_sensitivity();
        let itm_cash_probability = black.itm_cash_probability();

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
        pub(crate) settings: Shared<Settings<Date>>,
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
            let mut option = EuropeanOption::new(payoff, exercise, Shared::clone(&self.settings));
            let engine = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&self.process)));
            option
                .base_mut()
                .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
            option
        }
    }

    pub(crate) fn quote_handle(quote: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
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
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
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
            crate::shared::Shared::clone(&market.settings),
        );
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
            crate::shared::Shared::clone(&market.settings),
        );
        option
            .base_mut()
            .set_pricing_engine(template.base().pricing_engine().unwrap().clone());
        assert_eq!(
            option.npv().unwrap_err().message(),
            "only plain-vanilla payoffs are supported"
        );
    }
}

#[cfg(test)]
mod test_values {
    //! The `testValues` oracle of `test-suite/europeanoption.cpp`.
    //!
    //! Each row carries the published Haug value asserted at the C++ table
    //! tolerance of 1e-4, plus a full-precision reference computed with an
    //! independent double-precision Black-Scholes transliteration of the
    //! engine's formula path (times as `round(t * 360) / 360` on Actual360)
    //! and asserted at the milestone-gate tolerance of 1e-10.

    use super::test_market::{market, time_to_days, today};
    use crate::instrument::Instrument;
    use crate::option::OptionType::{self, Call, Put};
    use crate::types::{Rate, Real, Time, Volatility};

    type Row = (
        OptionType,
        Real,
        Real,
        Rate,
        Rate,
        Time,
        Volatility,
        Real,
        Real,
    );

    #[rustfmt::skip]
    const HAUG_VALUES: &[Row] = &[
        (Call, 65.00, 60.00, 0.00, 0.08, 0.25, 0.30, 2.1334, 2.1333684449161985),
        (Put, 95.00, 100.00, 0.05, 0.10, 0.50, 0.20, 2.4648, 2.4647876467558127),
        (Put, 19.00, 19.00, 0.10, 0.10, 0.75, 0.28, 1.7011, 1.701050725236268),
        (Call, 19.00, 19.00, 0.10, 0.10, 0.75, 0.28, 1.7011, 1.701050725236268),
        (Call, 1.60, 1.56, 0.08, 0.06, 0.50, 0.12, 0.0291, 0.029099253149439758),
        (Put, 70.00, 75.00, 0.05, 0.10, 0.50, 0.35, 4.0870, 4.086953828635346),
        (Call, 100.00, 90.00, 0.10, 0.10, 0.10, 0.15, 0.0205, 0.020490148536478747),
        (Call, 100.00, 100.00, 0.10, 0.10, 0.10, 0.15, 1.8734, 1.8733445727649416),
        (Call, 100.00, 110.00, 0.10, 0.10, 0.10, 0.15, 9.9413, 9.941277395489555),
        (Call, 100.00, 90.00, 0.10, 0.10, 0.10, 0.25, 0.3150, 0.31504580077736855),
        (Call, 100.00, 100.00, 0.10, 0.10, 0.10, 0.25, 3.1217, 3.121720698181133),
        (Call, 100.00, 110.00, 0.10, 0.10, 0.10, 0.25, 10.3556, 10.355552136523041),
        (Call, 100.00, 90.00, 0.10, 0.10, 0.10, 0.35, 0.9474, 0.9474175344225533),
        (Call, 100.00, 100.00, 0.10, 0.10, 0.10, 0.35, 4.3693, 4.369316848536221),
        (Call, 100.00, 110.00, 0.10, 0.10, 0.10, 0.35, 11.1381, 11.138125160603492),
        (Call, 100.00, 90.00, 0.10, 0.10, 0.50, 0.15, 0.8069, 0.8068924136759361),
        (Call, 100.00, 100.00, 0.10, 0.10, 0.50, 0.15, 4.0232, 4.0231670486171165),
        (Call, 100.00, 110.00, 0.10, 0.10, 0.50, 0.15, 10.5769, 10.576857786819106),
        (Call, 100.00, 90.00, 0.10, 0.10, 0.50, 0.25, 2.7026, 2.7025937303547045),
        (Call, 100.00, 100.00, 0.10, 0.10, 0.50, 0.25, 6.6997, 6.699696963531342),
        (Call, 100.00, 110.00, 0.10, 0.10, 0.50, 0.25, 12.7857, 12.785678929758706),
        (Call, 100.00, 90.00, 0.10, 0.10, 0.50, 0.35, 4.9329, 4.932877430523136),
        (Call, 100.00, 100.00, 0.10, 0.10, 0.50, 0.35, 9.3679, 9.36787664636385),
        (Call, 100.00, 110.00, 0.10, 0.10, 0.50, 0.35, 15.3086, 15.308599761080432),
        (Put, 100.00, 90.00, 0.10, 0.10, 0.10, 0.15, 9.9210, 9.92098848602816),
        (Put, 100.00, 100.00, 0.10, 0.10, 0.10, 0.15, 1.8734, 1.8733445727649416),
        (Put, 100.00, 110.00, 0.10, 0.10, 0.10, 0.15, 0.0408, 0.04077905799786819),
        (Put, 100.00, 90.00, 0.10, 0.10, 0.10, 0.25, 10.2155, 10.215544138269044),
        (Put, 100.00, 100.00, 0.10, 0.10, 0.10, 0.25, 3.1217, 3.121720698181133),
        (Put, 100.00, 110.00, 0.10, 0.10, 0.10, 0.25, 0.4551, 0.4550537990313475),
        (Put, 100.00, 90.00, 0.10, 0.10, 0.10, 0.35, 10.8479, 10.847915871914248),
        (Put, 100.00, 100.00, 0.10, 0.10, 0.10, 0.35, 4.3693, 4.369316848536221),
        (Put, 100.00, 110.00, 0.10, 0.10, 0.10, 0.35, 1.2376, 1.2376268231118066),
        (Put, 100.00, 90.00, 0.10, 0.10, 0.50, 0.15, 10.3192, 10.319186658683082),
        (Put, 100.00, 100.00, 0.10, 0.10, 0.50, 0.15, 4.0232, 4.0231670486171165),
        (Put, 100.00, 110.00, 0.10, 0.10, 0.50, 0.15, 1.0646, 1.064563541811972),
        (Put, 100.00, 90.00, 0.10, 0.10, 0.50, 0.25, 12.2149, 12.214887975361835),
        (Put, 100.00, 100.00, 0.10, 0.10, 0.50, 0.25, 6.6997, 6.699696963531342),
        (Put, 100.00, 110.00, 0.10, 0.10, 0.50, 0.25, 3.2734, 3.273384684751556),
        (Put, 100.00, 90.00, 0.10, 0.10, 0.50, 0.35, 14.4452, 14.445171675530275),
        (Put, 100.00, 100.00, 0.10, 0.10, 0.50, 0.35, 9.3679, 9.367876646363843),
        (Put, 100.00, 110.00, 0.10, 0.10, 0.50, 0.35, 5.7963, 5.796305516073297),
        (Call, 40.00, 42.00, 0.08, 0.04, 0.75, 0.35, 5.0975, 5.097547717726163),
    ];

    #[test]
    fn values_match_haug_at_1e_4_and_the_reference_at_1e_10() {
        let market = market();
        for &(option_type, strike, spot, q, r, t, vol, haug, precise) in HAUG_VALUES {
            market.set(spot, q, r, vol);
            let expiry = today() + time_to_days(t);
            let mut option = market.option(option_type, strike, expiry);
            let value = option.npv().unwrap();
            assert!(
                (value - haug).abs() <= 1.0e-4,
                "{option_type:?} K={strike} S={spot} q={q} r={r} t={t} v={vol}: \
                 value {value} vs Haug {haug}"
            );
            assert!(
                (value - precise).abs() <= 1.0e-10,
                "{option_type:?} K={strike} S={spot} q={q} r={r} t={t} v={vol}: \
                 value {value} vs reference {precise} (error {})",
                (value - precise).abs()
            );
        }
    }
}

#[cfg(test)]
mod greek_gate {
    //! The milestone gate on the greeks: every value the engine fills is
    //! asserted at 1e-10 against a full-precision reference computed with the
    //! same independent transliteration as the value table.

    use super::test_market::{market, time_to_days, today};
    use crate::instrument::Instrument;
    use crate::instruments::EuropeanOption;
    use crate::option::OptionType::{self, Call, Put};
    use crate::types::{Rate, Real, Time, Volatility};

    struct GateRow {
        option_type: OptionType,
        strike: Real,
        spot: Real,
        q: Rate,
        r: Rate,
        t: Time,
        vol: Volatility,
        value: Real,
        delta: Real,
        gamma: Real,
        theta: Real,
        vega: Real,
        rho: Real,
        dividend_rho: Real,
        strike_sensitivity: Real,
        itm_cash: Real,
    }

    #[rustfmt::skip]
    const GATE: &[GateRow] = &[
        GateRow { option_type: Call, strike: 65.0, spot: 60.0, q: 0.0, r: 0.08, t: 0.25, vol: 0.30,
            value: 2.1333684449161985, delta: 0.3724827979619727, gamma: 0.042042755753785174, theta: -8.428174386737366,
            vega: 11.351544053521998, rho: 5.053899858200554, dividend_rho: -5.587241969429603, strike_sensitivity: -0.3110092220431104, itm_cash: 0.31729202508906007 },
        GateRow { option_type: Put, strike: 95.0, spot: 100.0, q: 0.05, r: 0.10, t: 0.50, vol: 0.20,
            value: 2.4647876467558127, delta: -0.2641815996360725, gamma: 0.02283957429626998, theta: -3.0005280963980523,
            vega: 22.839574296270005, rho: -14.441473805181547, dividend_rho: 13.20907998180364, strike_sensitivity: 0.3040310274775061, itm_cash: 0.6803809684113931 },
        GateRow { option_type: Call, strike: 1.6, spot: 1.56, q: 0.08, r: 0.06, t: 0.50, vol: 0.12,
            value: 0.029099253149439758, delta: 0.34038590923214296, gamma: 2.700266083546169, theta: -0.03494785073760011,
            vega: 0.39428205245507725, rho: 0.2509513826263517, dividend_rho: -0.26550100920107156, strike_sensitivity: -0.31368922828293955, itm_cash: 0.32324248753653484 },
        GateRow { option_type: Call, strike: 100.0, spot: 100.0, q: 0.10, r: 0.10, t: 0.10, vol: 0.15,
            value: 1.8733445727649416, delta: 0.5043916397384094, gamma: 0.08324414875558867, theta: -9.177632277727229,
            vega: 12.486622313338298, rho: 4.856581940107595, dividend_rho: -5.043916397384089, strike_sensitivity: -0.4856581940107594, itm_cash: 0.4905391400063628 },
        GateRow { option_type: Put, strike: 100.0, spot: 100.0, q: 0.10, r: 0.10, t: 0.10, vol: 0.15,
            value: 1.8733445727649416, delta: -0.4856581940107596, gamma: 0.08324414875558867, theta: -9.177632277727229,
            vega: 12.486622313338298, rho: -5.043916397384088, dividend_rho: 4.856581940107593, strike_sensitivity: 0.5043916397384087, itm_cash: 0.4905391400063628 },
        GateRow { option_type: Call, strike: 40.0, spot: 42.0, q: 0.08, r: 0.04, t: 0.75, vol: 0.35,
            value: 5.097547717726163, delta: 0.5505079008346857, gamma: 0.028847096893412225, theta: -1.9880294017374107,
            vega: 13.357648216494526, rho: 13.517838087997976, dividend_rho: -17.3409988762926, strike_sensitivity: -0.4505946029332657, itm_cash: 0.46431725156756853 },
    ];

    const GATE_TOLERANCE: Real = 1.0e-10;

    #[test]
    fn all_engine_greeks_match_the_reference_at_1e_10() {
        let market = market();
        for row in GATE {
            market.set(row.spot, row.q, row.r, row.vol);
            let expiry = today() + time_to_days(row.t);
            let mut option = market.option(row.option_type, row.strike, expiry);

            let check = |name: &str, calculated: Real, expected: Real| {
                assert!(
                    (calculated - expected).abs() <= GATE_TOLERANCE,
                    "{:?} K={} t={}: {name} {calculated} vs reference {expected} (error {})",
                    row.option_type,
                    row.strike,
                    row.t,
                    (calculated - expected).abs()
                );
            };

            check("value", option.npv().unwrap(), row.value);
            check("delta", option.delta().unwrap(), row.delta);
            check("gamma", option.gamma().unwrap(), row.gamma);
            check("theta", option.theta().unwrap(), row.theta);
            check(
                "thetaPerDay",
                option.theta_per_day().unwrap(),
                row.theta / 365.0,
            );
            check("vega", option.vega().unwrap(), row.vega);
            check("rho", option.rho().unwrap(), row.rho);
            check(
                "dividendRho",
                option.dividend_rho().unwrap(),
                row.dividend_rho,
            );
            check(
                "strikeSensitivity",
                option.strike_sensitivity().unwrap(),
                row.strike_sensitivity,
            );
            check(
                "itmCashProbability",
                option.itm_cash_probability().unwrap(),
                row.itm_cash,
            );
        }
    }

    /// The ten quantities the greek gate locks: NPV plus the nine greeks.
    fn snapshot(option: &mut EuropeanOption) -> [Real; 10] {
        [
            option.npv().unwrap(),
            option.delta().unwrap(),
            option.gamma().unwrap(),
            option.theta().unwrap(),
            option.vega().unwrap(),
            option.rho().unwrap(),
            option.dividend_rho().unwrap(),
            option.strike_sensitivity().unwrap(),
            option.itm_cash_probability().unwrap(),
            option.theta_per_day().unwrap(),
        ]
    }

    /// The recursive-notification suppression of the D1 lazy-object change,
    /// exercised on the real Milestone 1 pricing path rather than the unit
    /// mock in `instrument.rs`.
    ///
    /// An observer on the option writes the spot back to its primed value from
    /// inside the option's own notification, while the option's lazy object is
    /// mid-update. Without the guard this re-entrant path double-borrows the
    /// lazy object and panics (the original bug); with it, the write is safe
    /// and the net input state is unchanged, so every greek must be
    /// byte-identical to the primed snapshot. A dropped recalculation would
    /// leave the cache stale, and a recompute against the transient spot would
    /// perturb the greeks; the exact compare catches both.
    #[test]
    fn recursive_input_writeback_leaves_the_m1_greeks_unchanged() {
        use crate::patterns::observable::Observer;
        use crate::quotes::SimpleQuote;
        use crate::shared::{Shared, SharedMut, shared_mut};

        let spot0 = 100.0;
        let market = market();
        market.set(spot0, 0.10, 0.10, 0.15);
        let expiry = today() + time_to_days(0.10);
        let mut option = market.option(Call, 100.0, expiry);

        let primed = snapshot(&mut option);

        struct Restore {
            spot: Shared<SimpleQuote>,
            to: Real,
            fired: usize,
        }
        impl Observer for Restore {
            fn update(&mut self) {
                self.fired += 1;
                if self.fired == 1 {
                    self.spot.set_value(self.to);
                }
            }
        }

        let restore = shared_mut(Restore {
            spot: Shared::clone(&market.spot),
            to: spot0,
            fired: 0,
        });
        option
            .base()
            .register_observer(&(SharedMut::clone(&restore) as SharedMut<dyn Observer>));

        market.spot.set_value(105.0);

        assert!(
            restore.borrow().fired >= 1,
            "the perturbation must actually reach the option's observers",
        );
        assert!(
            !option.base().is_calculated(),
            "the perturbation still invalidates the cached greeks",
        );

        let restored = snapshot(&mut option);
        assert_eq!(
            primed, restored,
            "the suppressed write-back must leave every greek byte-identical",
        );
    }
}

#[cfg(test)]
mod mixed_day_counters {
    //! Locks the curve-to-greek scaling of the result assembly: with a
    //! DIFFERENT day counter on each curve, rho must scale by the risk-free
    //! curve's time, dividendRho by the dividend curve's, and vega/theta by
    //! the vol curve's - a swap is invisible to the flat-market oracles
    //! (which share Actual360 across all three) but fails here.

    use super::AnalyticEuropeanEngine;
    use super::test_market::today;
    use crate::exercise::EuropeanExercise;
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
    use crate::interestrate::Compounding;
    use crate::option::OptionType;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::BlackCalculator;
    use crate::processes::BlackScholesMertonProcess;
    use crate::quotes::SimpleQuote;
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::types::Real;

    #[test]
    fn each_greek_scales_by_its_own_curve_time() {
        let spot: Real = 100.0;
        let strike: Real = 105.0;
        let (q, r, vol): (Real, Real, Real) = (0.04, 0.06, 0.20);
        let expiry = today() + 146;

        let rfdc = Actual360::new();
        let divdc = Actual365Fixed::new();
        let voldc = Thirty360::with_convention(Convention::BondBasis);
        let t_rf = rfdc.year_fraction(today(), expiry);
        let t_div = divdc.year_fraction(today(), expiry);
        let t_vol = voldc.year_fraction(today(), expiry);
        assert!(t_rf != t_div && t_div != t_vol && t_rf != t_vol);

        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let process = shared(BlackScholesMertonProcess::new(
            Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn crate::quotes::Quote>),
            Handle::new(shared(FlatForward::with_rate(
                today(),
                q,
                divdc.clone(),
                Compounding::Continuous,
                crate::time::frequency::Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>),
            Handle::new(shared(FlatForward::with_rate(
                today(),
                r,
                rfdc.clone(),
                Compounding::Continuous,
                crate::time::frequency::Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>),
            Handle::new(
                shared(BlackConstantVol::new(today(), None, vol, voldc.clone()))
                    as Shared<dyn BlackVolTermStructure>,
            ),
        ));

        let payoff = PlainVanillaPayoff::new(OptionType::Call, strike);
        let mut option = EuropeanOption::new(
            shared(payoff),
            shared(EuropeanExercise::new(expiry)),
            Shared::clone(&settings),
        );
        let engine = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&process)));
        option
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);

        let q_discount = (-q * t_div).exp();
        let r_discount = (-r * t_rf).exp();
        let forward = spot * q_discount / r_discount;
        let variance = vol * vol * t_vol;
        let black =
            BlackCalculator::with_payoff(&payoff, forward, variance.sqrt(), r_discount).unwrap();

        let check = |name: &str, calculated: Real, expected: Real| {
            assert!(
                (calculated - expected).abs() <= 1.0e-12,
                "{name}: engine {calculated} vs curve-scaled reference {expected}"
            );
        };
        check("value", option.npv().unwrap(), black.value());
        check("delta", option.delta().unwrap(), black.delta(spot).unwrap());
        check("gamma", option.gamma().unwrap(), black.gamma(spot).unwrap());
        check("rho", option.rho().unwrap(), black.rho(t_rf).unwrap());
        check(
            "dividendRho",
            option.dividend_rho().unwrap(),
            black.dividend_rho(t_div).unwrap(),
        );
        check("vega", option.vega().unwrap(), black.vega(t_vol).unwrap());
        check(
            "theta",
            option.theta().unwrap(),
            black.theta(spot, t_vol).unwrap(),
        );
    }
}

#[cfg(test)]
mod test_greek_values {
    //! The `testGreekValues` oracle of `test-suite/europeanoption.cpp`:
    //! published Haug greek values, asserted at the C++ tolerance of 1e-4.

    use super::test_market::{market, time_to_days, today};
    use crate::errors::QlResult;
    use crate::instruments::EuropeanOption;
    use crate::option::OptionType::{self, Call, Put};
    use crate::types::{Rate, Real, Time, Volatility};

    type Greek = fn(&mut EuropeanOption) -> QlResult<Real>;

    /// name, greek accessor, type, strike, spot, q, r, t, vol, Haug value.
    type Case = (
        &'static str,
        Greek,
        OptionType,
        Real,
        Real,
        Rate,
        Rate,
        Time,
        Volatility,
        Real,
    );

    #[test]
    fn greek_values_match_haug_at_1e_4() {
        #[rustfmt::skip]
        let cases: &[Case] = &[
            ("delta", |o| o.delta(), Call, 100.0, 105.0, 0.10, 0.10, 0.5, 0.36, 0.5946),
            ("delta", |o| o.delta(), Put, 100.0, 105.0, 0.10, 0.10, 0.5, 0.36, -0.3566),
            ("elasticity", |o| o.elasticity(), Put, 100.0, 105.0, 0.10, 0.10, 0.5, 0.36, -4.8775),
            ("gamma", |o| o.gamma(), Call, 60.0, 55.0, 0.00, 0.10, 0.75, 0.30, 0.0278),
            ("gamma", |o| o.gamma(), Put, 60.0, 55.0, 0.00, 0.10, 0.75, 0.30, 0.0278),
            ("vega", |o| o.vega(), Call, 60.0, 55.0, 0.00, 0.10, 0.75, 0.30, 18.9358),
            ("vega", |o| o.vega(), Put, 60.0, 55.0, 0.00, 0.10, 0.75, 0.30, 18.9358),
            ("theta", |o| o.theta(), Put, 405.0, 430.0, 0.05, 0.07, 1.0 / 12.0, 0.20, -31.1924),
            ("thetaPerDay", |o| o.theta_per_day(), Put, 405.0, 430.0, 0.05, 0.07, 1.0 / 12.0, 0.20, -0.0855),
            ("rho", |o| o.rho(), Call, 75.0, 72.0, 0.00, 0.09, 1.0, 0.19, 38.7325),
            ("dividendRho", |o| o.dividend_rho(), Put, 490.0, 500.0, 0.05, 0.08, 0.25, 0.15, 42.2254),
        ];

        let market = market();
        for &(name, greek, option_type, strike, spot, q, r, t, vol, expected) in cases {
            market.set(spot, q, r, vol);
            let expiry = today() + time_to_days(t);
            let mut option = market.option(option_type, strike, expiry);
            let calculated = greek(&mut option).unwrap();
            assert!(
                (calculated - expected).abs() <= 1.0e-4,
                "{name} of {option_type:?} K={strike} S={spot}: {calculated} vs Haug {expected}"
            );
        }
    }
}

#[cfg(test)]
mod test_greeks {
    //! The `testGreeks` oracle of `test-suite/europeanoption.cpp`: analytic
    //! greeks against central finite differences over the full plain-vanilla
    //! grid, on curves moving off the evaluation date. The C++ test also
    //! sweeps cash-or-nothing, asset-or-nothing and gap payoffs; those
    //! payoffs are follow-up work and their sweeps come with them.

    use super::AnalyticEuropeanEngine;
    use super::test_market::{quote_handle, time_to_days, today};
    use crate::exercise::EuropeanExercise;
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
    use crate::interestrate::Compounding;
    use crate::option::OptionType;
    use crate::pricingengine::PricingEngine;
    use crate::processes::BlackScholesMertonProcess;
    use crate::quotes::SimpleQuote;
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::BlackConstantVol;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::NullCalendar;
    use crate::time::date::Date;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::Real;

    const TOLERANCE: Real = 1.0e-5;
    const UNDERLYING: Real = 100.0;

    struct MovingMarket {
        settings: Shared<Settings<Date>>,
        spot: Shared<SimpleQuote>,
        q_rate: Shared<SimpleQuote>,
        r_rate: Shared<SimpleQuote>,
        vol: Shared<SimpleQuote>,
        process: Shared<BlackScholesMertonProcess>,
    }

    fn moving_market() -> MovingMarket {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let spot = shared(SimpleQuote::new(0.0));
        let q_rate = shared(SimpleQuote::new(0.0));
        let r_rate = shared(SimpleQuote::new(0.0));
        let vol = shared(SimpleQuote::new(0.0));
        let flat = |quote: &Shared<SimpleQuote>| {
            shared(FlatForward::moving(
                0,
                NullCalendar::new(),
                quote_handle(quote),
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
                Shared::clone(&settings),
            )) as Shared<dyn YieldTermStructure>
        };
        let process = shared(BlackScholesMertonProcess::new(
            quote_handle(&spot),
            Handle::new(flat(&q_rate)),
            Handle::new(flat(&r_rate)),
            Handle::new(shared(BlackConstantVol::moving_with_quote(
                0,
                NullCalendar::new(),
                quote_handle(&vol),
                Actual360::new(),
                Shared::clone(&settings),
            ))
                as Shared<
                    dyn crate::termstructures::volatility::BlackVolTermStructure,
                >),
        ));
        MovingMarket {
            settings,
            spot,
            q_rate,
            r_rate,
            vol,
            process,
        }
    }

    fn relative_error(x1: Real, x2: Real, reference: Real) -> Real {
        if reference != 0.0 {
            (x1 - x2).abs() / reference
        } else {
            (x1 - x2).abs()
        }
    }

    #[test]
    fn analytic_greeks_are_consistent_with_finite_differences() {
        let market = moving_market();
        let types = [OptionType::Call, OptionType::Put];
        let strikes = [50.0, 99.5, 100.0, 100.5, 150.0];
        let q_rates = [0.04, 0.05, 0.06];
        let r_rates = [0.01, 0.05, 0.15];
        let residual_times = [1.0, 2.0];
        let vols = [0.11, 0.50, 1.20];
        let day_counter = Actual360::new();

        for option_type in types {
            for strike in strikes {
                for residual_time in residual_times {
                    let expiry = today() + time_to_days(residual_time);
                    let payoff = shared(PlainVanillaPayoff::new(option_type, strike));
                    let exercise = shared(EuropeanExercise::new(expiry));
                    let mut option =
                        EuropeanOption::new(payoff, exercise, Shared::clone(&market.settings));
                    let engine =
                        shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&market.process)));
                    option
                        .base_mut()
                        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);

                    for q in q_rates {
                        for r in r_rates {
                            for vol in vols {
                                let u = UNDERLYING;
                                market.spot.set_value(u);
                                market.q_rate.set_value(q);
                                market.r_rate.set_value(r);
                                market.vol.set_value(vol);

                                let value = option.npv().unwrap();
                                let delta = option.delta().unwrap();
                                let gamma = option.gamma().unwrap();
                                let theta = option.theta().unwrap();
                                let rho = option.rho().unwrap();
                                let dividend_rho = option.dividend_rho().unwrap();
                                let vega = option.vega().unwrap();

                                if value <= u * 1.0e-5 {
                                    continue;
                                }

                                let du = u * 1.0e-4;
                                market.spot.set_value(u + du);
                                let value_p = option.npv().unwrap();
                                let delta_p = option.delta().unwrap();
                                market.spot.set_value(u - du);
                                let value_m = option.npv().unwrap();
                                let delta_m = option.delta().unwrap();
                                market.spot.set_value(u);
                                let expected_delta = (value_p - value_m) / (2.0 * du);
                                let expected_gamma = (delta_p - delta_m) / (2.0 * du);

                                let dr = r * 1.0e-4;
                                market.r_rate.set_value(r + dr);
                                let value_p = option.npv().unwrap();
                                market.r_rate.set_value(r - dr);
                                let value_m = option.npv().unwrap();
                                market.r_rate.set_value(r);
                                let expected_rho = (value_p - value_m) / (2.0 * dr);

                                let dq = q * 1.0e-4;
                                market.q_rate.set_value(q + dq);
                                let value_p = option.npv().unwrap();
                                market.q_rate.set_value(q - dq);
                                let value_m = option.npv().unwrap();
                                market.q_rate.set_value(q);
                                let expected_dividend_rho = (value_p - value_m) / (2.0 * dq);

                                let dv = vol * 1.0e-4;
                                market.vol.set_value(vol + dv);
                                let value_p = option.npv().unwrap();
                                market.vol.set_value(vol - dv);
                                let value_m = option.npv().unwrap();
                                market.vol.set_value(vol);
                                let expected_vega = (value_p - value_m) / (2.0 * dv);

                                let dt = day_counter.year_fraction(today() - 1, today() + 1);
                                market.settings.set_evaluation_date(today() - 1);
                                let value_m = option.npv().unwrap();
                                market.settings.set_evaluation_date(today() + 1);
                                let value_p = option.npv().unwrap();
                                market.settings.set_evaluation_date(today());
                                let expected_theta = (value_p - value_m) / dt;

                                let checks = [
                                    ("delta", expected_delta, delta),
                                    ("gamma", expected_gamma, gamma),
                                    ("theta", expected_theta, theta),
                                    ("rho", expected_rho, rho),
                                    ("divRho", expected_dividend_rho, dividend_rho),
                                    ("vega", expected_vega, vega),
                                ];
                                for (name, expected, calculated) in checks {
                                    let error = relative_error(expected, calculated, u);
                                    assert!(
                                        error <= TOLERANCE,
                                        "{name} of {option_type:?} K={strike} T={residual_time} \
                                         q={q} r={r} v={vol}: analytic {calculated} vs finite \
                                         difference {expected} (relative error {error})"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
