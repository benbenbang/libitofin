//! Gatheral two-probability European Bates pricing with constant lognormal jumps.

use super::analytichestonengine::{HestonChf, Integration};
use super::hestonmarket::HestonMarket;
use crate::errors::QlResult;
use crate::instruments::{OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::models::{BatesModel, CalibratedModelHolder};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::processes::BatesProcess;
use crate::require;
use crate::shared::SharedMut;
use crate::types::{Complex, Size};

/// Analytic European plain-vanilla NPV engine. Unsupported Greeks remain unavailable.
pub struct BatesEngine {
    base: OneAssetOptionEngine,
    model: SharedMut<BatesModel>,
    integration: Integration,
}

impl BatesEngine {
    /// Observes the retained Bates model using fixed-order Gauss-Laguerre quadrature.
    ///
    /// # Errors
    /// Rejects orders outside 1..=192 or quadrature construction failures.
    pub fn new(model: SharedMut<BatesModel>, integration_order: Size) -> QlResult<Self> {
        require!(
            (1..=192).contains(&integration_order),
            "Bates integration order must be in 1..=192"
        );
        let integration = Integration::gauss_laguerre(integration_order)?;
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Ok(Self {
            base,
            model,
            integration,
        })
    }

    /// Uses the default quadrature order 144.
    ///
    /// # Errors
    /// Propagates quadrature construction failures.
    pub fn with_default_order(model: SharedMut<BatesModel>) -> QlResult<Self> {
        Self::new(model, 144)
    }
}

impl AsObservable for BatesEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for BatesEngine {
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
        let model = self.model.borrow();
        BatesProcess::validate_parameters(&model.calibrated_model().params())?;
        let process = model.process();
        let mut market =
            HestonMarket::from_process(self.base.arguments(), &process.heston_process())?;
        market.chf = HestonChf::new(
            model.kappa(),
            model.theta(),
            model.sigma(),
            model.rho(),
            model.v0(),
        );
        let lambda = model.lambda();
        let nu = model.nu();
        let delta2 = 0.5 * model.delta() * model.delta();
        drop(model);
        let compensator = (nu + delta2).exp_m1();
        let log_moneyness = market.forward.ln() - market.strike.ln();
        let probability = |first: bool| {
            0.5 + self.integration.calculate(0.0, |frequency| {
                let z = Complex::new(frequency, if first { -1.0 } else { 0.0 });
                let g = Complex::new(if first { 1.0 } else { 0.0 }, frequency);
                let jump = if lambda == 0.0 {
                    Complex::new(0.0, 0.0)
                } else {
                    market.time
                        * lambda
                        * ((nu * g + delta2 * g * g).exp()
                            - Complex::new(1.0, 0.0)
                            - g * compensator)
                };
                (market.chf.chf(z, market.time)
                    * (jump + Complex::new(0.0, frequency * log_moneyness)).exp())
                .im / frequency
            }) / std::f64::consts::PI
        };
        let p1 = probability(true);
        let p2 = probability(false);
        let undiscounted = match market.option_type {
            OptionType::Call => market.forward * p1 - market.strike * p2,
            OptionType::Put => market.strike * (1.0 - p2) - market.forward * (1.0 - p1),
        };
        let value = market.discount * undiscounted;
        require!(
            p1.is_finite() && p2.is_finite() && value.is_finite(),
            "nonfinite Bates pricing result"
        );
        self.base.results_mut().instrument.value = Some(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
