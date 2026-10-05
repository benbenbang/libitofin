//! Spot-start annualized variance swap with inspectable option-strip replication.

use std::any::Any;

use crate::errors::QlResult;
use crate::event::event_has_occurred;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::option::OptionType;
use crate::position::Position;
use crate::pricingengine::{Arguments, GenericEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::{fail, require};

/// Purchased European option and its discrete log-payoff replication weight.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VarianceSwapOptionWeight {
    /// Call or put.
    pub option_type: OptionType,
    /// Positive option strike.
    pub strike: f64,
    /// Finite signed replication weight.
    pub weight: f64,
}

/// Typed variance-swap engine inputs.
#[derive(Default)]
pub struct VarianceSwapArguments {
    /// Long or short variance exposure.
    pub position: Option<Position>,
    /// Annualized variance strike, not volatility.
    pub strike: Option<f64>,
    /// Cash amount per whole variance unit.
    pub notional: Option<f64>,
    /// Contract start date.
    pub start_date: Option<Date>,
    /// Contract maturity date.
    pub maturity_date: Option<Date>,
    /// Explicit evaluation-date settings.
    pub settings: Option<Shared<Settings<Date>>>,
}

impl Arguments for VarianceSwapArguments {
    fn validate(&self) -> QlResult<()> {
        require!(self.position.is_some(), "variance swap position missing");
        require!(
            self.strike.is_some_and(|v| v.is_finite() && v > 0.0),
            "variance strike must be finite and positive"
        );
        require!(
            self.notional.is_some_and(|v| v.is_finite() && v > 0.0),
            "variance notional must be finite and positive"
        );
        let Some((start, maturity)) = self.start_date.zip(self.maturity_date) else {
            fail!("variance swap dates missing");
        };
        require!(
            start != Date::default() && maturity != Date::default() && start < maturity,
            "variance swap requires valid start before maturity"
        );
        require!(self.settings.is_some(), "variance swap settings missing");
        Ok(())
    }
}

/// Typed variance-swap outputs; cleared together before a calculation.
#[derive(Default)]
pub struct VarianceSwapResults {
    /// Common instrument outputs.
    pub instrument: InstrumentResults,
    /// Annualized finite signed variance.
    pub variance: Option<f64>,
    /// Calls ascending followed by puts descending.
    pub option_weights: Vec<VarianceSwapOptionWeight>,
}

impl Results for VarianceSwapResults {
    fn reset(&mut self) {
        self.instrument.reset();
        self.variance = None;
        self.option_weights.clear();
    }

    fn as_instrument_results(&self) -> Option<&InstrumentResults> {
        Some(&self.instrument)
    }
}

/// Generic engine base for variance-swap pricing.
pub type VarianceSwapEngine = GenericEngine<VarianceSwapArguments, VarianceSwapResults>;

/// Spot-start variance swap, without realized observations or sampling corrections.
pub struct VarianceSwap {
    base: InstrumentBase,
    position: Position,
    strike: f64,
    notional: f64,
    start_date: Date,
    maturity_date: Date,
    settings: Shared<Settings<Date>>,
    variance: Option<f64>,
    option_weights: Vec<VarianceSwapOptionWeight>,
}

impl VarianceSwap {
    /// Creates immutable terms. Live start/reference-date support is checked at pricing.
    pub fn new(
        position: Position,
        strike: f64,
        notional: f64,
        start_date: Date,
        maturity_date: Date,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Self> {
        let arguments = VarianceSwapArguments {
            position: Some(position),
            strike: Some(strike),
            notional: Some(notional),
            start_date: Some(start_date),
            maturity_date: Some(maturity_date),
            settings: Some(settings.clone()),
        };
        arguments.validate()?;
        let base = InstrumentBase::new();
        settings.register_eval_date_observer(&base.observer());
        Ok(Self {
            base,
            position,
            strike,
            notional,
            start_date,
            maturity_date,
            settings,
            variance: None,
            option_weights: Vec::new(),
        })
    }

    /// Exposure direction.
    pub fn position(&self) -> Position {
        self.position
    }
    /// Annualized variance strike.
    pub fn strike(&self) -> f64 {
        self.strike
    }
    /// Cash amount per whole variance unit.
    pub fn notional(&self) -> f64 {
        self.notional
    }
    /// Immutable contract start.
    pub fn start_date(&self) -> Date {
        self.start_date
    }
    /// Immutable contract maturity.
    pub fn maturity_date(&self) -> Date {
        self.maturity_date
    }

    /// Lazily prices and returns annualized variance; unavailable after expiry.
    pub fn variance(&mut self) -> QlResult<f64> {
        self.calculate()?;
        let Some(variance) = self.variance else {
            fail!("variance not provided");
        };
        Ok(variance)
    }

    /// Fresh owned copy of purchased option weights; unavailable after expiry.
    pub fn option_weights(&mut self) -> QlResult<Vec<VarianceSwapOptionWeight>> {
        self.calculate()?;
        require!(
            self.variance.is_some(),
            "variance option weights not provided"
        );
        Ok(self.option_weights.clone())
    }
}

impl Instrument for VarianceSwap {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }
    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }
    fn is_expired(&self) -> QlResult<bool> {
        event_has_occurred(self.maturity_date, &self.settings, None, None)
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(arguments) = (arguments as &mut dyn Any).downcast_mut::<VarianceSwapArguments>()
        else {
            fail!("wrong variance swap arguments");
        };
        *arguments = VarianceSwapArguments {
            position: Some(self.position),
            strike: Some(self.strike),
            notional: Some(self.notional),
            start_date: Some(self.start_date),
            maturity_date: Some(self.maturity_date),
            settings: Some(self.settings.clone()),
        };
        Ok(())
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let Some(results) = (results as &dyn Any).downcast_ref::<VarianceSwapResults>() else {
            fail!("wrong variance swap results");
        };
        require!(
            results.variance.is_some_and(f64::is_finite)
                && results.instrument.value.is_some_and(f64::is_finite),
            "invalid variance swap results"
        );
        self.base.store_results(&results.instrument);
        self.variance = results.variance;
        self.option_weights.clone_from(&results.option_weights);
        Ok(())
    }

    fn setup_expired(&mut self) {
        let results = InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        };
        self.base.store_results(&results);
        self.variance = None;
        self.option_weights.clear();
    }
}

#[cfg(test)]
#[path = "varianceswap_tests.rs"]
mod tests;
