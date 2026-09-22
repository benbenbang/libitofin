//! Exponentially fitted Gauss-Laguerre Heston pricing, adapted from QuantLib 1.43.
//! Copyright (C) 2020 Klaus Spanderen; QuantLib license in THIRD_PARTY_NOTICES.md.

use super::analytichestonengine::{AnalyticHestonEngine, ApHelper, ComplexLogFormula};
use super::heston_fitting_table::FITTING_TABLE;
use super::hestonmarket::HestonMarket;
use crate::errors::QlResult;
use crate::instruments::{OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::models::{HestonModel, model::CalibratedModelHolder};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::require;
use crate::shared::SharedMut;
use crate::types::Real;

/// Control variates supported by exponentially fitted Heston integration.
/// Gatheral and BranchCorrection are not control variates and are not supported.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ExponentialFittingControlVariate {
    /// Select the angled-contour or asymptotic variate from the model parameters.
    #[default]
    Optimal,
    /// Black control variate using average integrated variance.
    AndersenPiterbarg,
    /// Black control variate using characteristic-function matching.
    AndersenPiterbargOptCV,
    /// Asymptotic characteristic-function control variate; requires alpha = -0.5.
    AsymptoticChF,
    /// Angled contour with Black control variate.
    AngledContour,
    /// Angled contour without a control variate.
    AngledContourNoCV,
}

/// European plain-vanilla prices using 64-point exponentially fitted quadrature.
pub struct ExponentialFittingHestonEngine {
    base: OneAssetOptionEngine,
    model: SharedMut<HestonModel>,
    cv: ExponentialFittingControlVariate,
    scaling: Option<Real>,
    alpha: Real,
}
impl ExponentialFittingHestonEngine {
    /// Builds an engine; `None` selects QuantLib's automatic integration scaling.
    /// Scaling must be positive and finite. Alpha must be finite; asymptotic
    /// control variates require alpha equal to minus one half.
    pub fn new(
        model: SharedMut<HestonModel>,
        cv: ExponentialFittingControlVariate,
        scaling: Option<Real>,
        alpha: Real,
    ) -> QlResult<Self> {
        require!(
            scaling.is_none_or(|s| s.is_finite() && s > 0.0),
            "positive finite integration scaling required"
        );
        require!(alpha.is_finite(), "finite alpha required");
        require!(
            cv != ExponentialFittingControlVariate::AsymptoticChF || alpha == -0.5,
            "asymptotic control variate requires alpha = -0.5"
        );
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Ok(Self {
            base,
            model,
            cv,
            scaling,
            alpha,
        })
    }
}
impl AsObservable for ExponentialFittingHestonEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl PricingEngine for ExponentialFittingHestonEngine {
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
        let m = HestonMarket::new(self.base.arguments(), &self.model.borrow())?;
        let p = m.chf;
        let cv = match self.cv {
            ExponentialFittingControlVariate::Optimal => {
                AnalyticHestonEngine::optimal_control_variate(
                    m.time,
                    p.v0(),
                    p.kappa(),
                    p.theta(),
                    p.sigma(),
                    p.rho(),
                )
            }
            ExponentialFittingControlVariate::AndersenPiterbarg => {
                ComplexLogFormula::AndersenPiterbarg
            }
            ExponentialFittingControlVariate::AndersenPiterbargOptCV => {
                ComplexLogFormula::AndersenPiterbargOptCV
            }
            ExponentialFittingControlVariate::AsymptoticChF => ComplexLogFormula::AsymptoticChF,
            ExponentialFittingControlVariate::AngledContour => ComplexLogFormula::AngledContour,
            ExponentialFittingControlVariate::AngledContourNoCV => {
                ComplexLogFormula::AngledContourNoCV
            }
        };
        let helper = ApHelper::new(m.time, m.forward, m.strike, cv, p, self.alpha)?;
        let v_avg = (1.0 - (-p.kappa() * m.time).exp()) * (p.v0() - p.theta())
            / (p.kappa() * m.time)
            + p.theta();
        let scaling = self.scaling.unwrap_or_else(|| {
            if cv == ComplexLogFormula::AsymptoticChF {
                1.0
            } else {
                (0.25 / (0.5 * v_avg * m.time).sqrt()).clamp(0.25, 1000.0)
            }
        });
        let freq = m.forward.ln() - m.strike.ln();
        let (row, u) = if freq.abs() < 0.1 {
            (&FITTING_TABLE[0], scaling)
        } else {
            let lookup = (scaling * freq).abs();
            let mut n = FITTING_TABLE
                .partition_point(|r| r[0] < lookup)
                .min(FITTING_TABLE.len() - 1);
            if n > 0
                && (lookup - FITTING_TABLE[n][0]).abs() > (lookup - FITTING_TABLE[n - 1][0]).abs()
            {
                n -= 1;
            }
            (&FITTING_TABLE[n], (FITTING_TABLE[n][0] / freq).abs())
        };
        let sum: Real = (0..64)
            .map(|i| row[65 + i] * u * helper.evaluate(u * row[1 + i]))
            .sum();
        let call = helper.control_variate_value()? + sum * m.forward / std::f64::consts::PI;
        let value = m.discount
            * match m.option_type {
                OptionType::Call => call,
                OptionType::Put => call - (m.forward - m.strike),
            };
        require!(
            value.is_finite(),
            "nonfinite exponentially fitted Heston price"
        );
        self.base.results_mut().instrument.value = Some(value);
        Ok(())
    }
}
