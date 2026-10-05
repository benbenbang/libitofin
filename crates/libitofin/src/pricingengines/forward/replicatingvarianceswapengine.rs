//! Pinned native finite-strip log-payoff replication, with live-process observation.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::EuropeanExercise;
use crate::instruments::{
    OptionArguments, PlainVanillaPayoff, VarianceSwapArguments, VarianceSwapEngine,
    VarianceSwapOptionWeight, VarianceSwapResults,
};
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable};
use crate::position::Position;
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::AnalyticEuropeanEngine;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::{fail, require};

use super::varianceswapstrip::VarianceSwapStrip;

/// Finite option-strip variance replication using the native piecewise-linear rule.
///
/// Spot start must match Settings and every market reference date at pricing.
/// Nonzero dividends retain the native boundary-dependent limitation, not a
/// volatility-squared guarantee. Finite signed weights and variance are preserved.
pub struct ReplicatingVarianceSwapEngine {
    base: VarianceSwapEngine,
    process: Shared<GeneralizedBlackScholesProcess>,
    strip: VarianceSwapStrip,
}

impl ReplicatingVarianceSwapEngine {
    /// Validates, sorts and deduplicates copied call/put strips.
    ///
    /// # Errors
    /// Requires two distinct positive strikes per side, an exact shared boundary,
    /// at most 4096 raw entries per side and positive representable finite tails.
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        dk: f64,
        call_strikes: &[f64],
        put_strikes: &[f64],
    ) -> QlResult<Self> {
        let strip = VarianceSwapStrip::new(dk, call_strikes, put_strikes)?;
        let base = VarianceSwapEngine::new(
            VarianceSwapArguments::default(),
            VarianceSwapResults::default(),
        );
        base.register_with(process.observable());
        Ok(Self {
            base,
            process,
            strip,
        })
    }

    fn option_value(
        &self,
        engine: &mut AnalyticEuropeanEngine,
        maturity: Date,
        weight: &VarianceSwapOptionWeight,
    ) -> QlResult<f64> {
        let volatility = self.process.black_volatility().current_link()?;
        let variance = volatility.black_variance_date(maturity, weight.strike, false)?;
        require!(
            variance.is_finite() && variance >= 0.0,
            "option variance must be finite and nonnegative"
        );
        engine.reset();
        let Some(arguments) =
            (engine.arguments_mut() as &mut dyn Any).downcast_mut::<OptionArguments>()
        else {
            fail!("wrong analytic option arguments");
        };
        arguments.payoff = Some(shared(PlainVanillaPayoff::new(
            weight.option_type,
            weight.strike,
        )));
        arguments.exercise = Some(shared(EuropeanExercise::new(maturity)));
        arguments.validate()?;
        engine.calculate()?;
        let Some(value) = engine
            .results()
            .as_instrument_results()
            .and_then(|r| r.value)
        else {
            fail!("replicating option value missing");
        };
        require!(value.is_finite(), "nonfinite replicating option value");
        Ok(value)
    }
}

impl AsObservable for ReplicatingVarianceSwapEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for ReplicatingVarianceSwapEngine {
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
        arguments.validate()?;
        let Some((start, maturity)) = arguments.start_date.zip(arguments.maturity_date) else {
            fail!("variance swap dates missing");
        };
        let Some(settings) = &arguments.settings else {
            fail!("variance swap settings missing");
        };
        require!(
            settings.evaluation_date() == Some(start),
            "variance swap requires current spot start; forward/seasoned starts unsupported"
        );
        let risk_free = self.process.risk_free_rate().current_link()?;
        let dividend = self.process.dividend_yield().current_link()?;
        let volatility = self.process.black_volatility().current_link()?;
        require!(
            risk_free.reference_date()? == start
                && dividend.reference_date()? == start
                && volatility.reference_date()? == start,
            "variance swap start must match all market reference dates"
        );
        let time = self.process.time(&maturity)?;
        require!(
            time.is_finite() && time > 0.0,
            "variance swap time must be finite and positive"
        );
        let vol_time = volatility.time_from_reference(maturity)?;
        let dividend_time = dividend.time_from_reference(maturity)?;
        require!(
            vol_time.is_finite()
                && vol_time > 0.0
                && dividend_time.is_finite()
                && dividend_time > 0.0,
            "invalid variance swap market times"
        );
        let spot = self.process.state_variable().current_link()?.value()?;
        require!(
            spot.is_finite() && spot > 0.0,
            "variance swap spot must be finite and positive"
        );
        let discount = risk_free.discount(time, false)?;
        let settlement_discount = risk_free.discount_date(maturity, false)?;
        let dividend_discount = dividend.discount_date(maturity, false)?;
        require!(
            [discount, settlement_discount, dividend_discount]
                .iter()
                .all(|v| v.is_finite() && *v > 0.0),
            "variance swap discounts must be finite and positive"
        );
        let inverse_discount = 1.0 / discount;
        let rate = risk_free
            .zero_rate(time, Compounding::Continuous, Frequency::NoFrequency, true)?
            .rate();
        let dividend_rate = dividend
            .zero_rate(
                dividend_time,
                Compounding::Continuous,
                Frequency::NoFrequency,
                true,
            )?
            .rate();
        require!(
            inverse_discount.is_finite() && rate.is_finite() && dividend_rate.is_finite(),
            "nonfinite variance swap market rate"
        );
        let forward_numerator = spot * dividend_discount;
        let forward = forward_numerator / settlement_discount;
        require!(
            forward_numerator.is_finite() && forward.is_finite() && forward > 0.0,
            "variance swap option forward must be finite and positive"
        );
        let weights = self.strip.weights(time)?;
        let mut option_engine = AnalyticEuropeanEngine::new(self.process.clone());
        let mut options_value = 0.0;
        for weight in &weights {
            let option_value = self.option_value(&mut option_engine, maturity, weight)?;
            let weighted_value = option_value * weight.weight;
            options_value += weighted_value;
            require!(
                weighted_value.is_finite() && options_value.is_finite(),
                "nonfinite variance replicating portfolio"
            );
        }
        let boundary = self.strip.boundary();
        let no_dividend_forward = spot / discount;
        let forward_difference = no_dividend_forward - boundary;
        let linear = forward_difference / boundary;
        let log_ratio = boundary / spot;
        let scale = 2.0 / time;
        require!(
            no_dividend_forward.is_finite()
                && forward_difference.is_finite()
                && linear.is_finite()
                && log_ratio.is_finite()
                && log_ratio > 0.0
                && scale.is_finite(),
            "nonfinite variance swap replication inputs"
        );
        let correction = linear + log_ratio.ln();
        let rate_term = 2.0 * rate;
        let correction_term = scale * correction;
        let portfolio_term = options_value / discount;
        let variance = rate_term - correction_term + portfolio_term;
        require!(
            [
                correction,
                rate_term,
                correction_term,
                portfolio_term,
                variance
            ]
            .iter()
            .all(|v| v.is_finite()),
            "nonfinite replicated variance"
        );
        let Some((position, (notional, strike))) = arguments
            .position
            .zip(arguments.notional.zip(arguments.strike))
        else {
            fail!("variance swap terms missing");
        };
        let multiplier = match position {
            Position::Long => 1.0,
            Position::Short => -1.0,
        };
        let discounted_notional = multiplier * settlement_discount * notional;
        let variance_difference = variance - strike;
        let value = discounted_notional * variance_difference;
        require!(
            discounted_notional.is_finite() && variance_difference.is_finite() && value.is_finite(),
            "nonfinite variance swap NPV"
        );
        let results = self.base.results_mut();
        results.variance = Some(variance);
        results.option_weights = weights;
        results.instrument.value = Some(value);
        Ok(())
    }
}
