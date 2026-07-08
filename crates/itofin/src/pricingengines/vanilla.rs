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
