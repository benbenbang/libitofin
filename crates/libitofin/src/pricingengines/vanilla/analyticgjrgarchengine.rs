//! QuantLib's GJR-GARCH European Edgeworth approximation.
//!
//! Ports the moment expansion of Duan, Gauthier, Simonato and Sasseville
//! implemented in QuantLib's `analyticgjrgarchengine.cpp` (Copyright 2008
//! Yee Man Chan, QuantLib license). This is an approximation, not an exact
//! valuation of the diffusion process used by the Monte Carlo engine.

mod coefficients;
mod moments;
#[cfg(test)]
mod oracle_values;
#[cfg(test)]
mod tests;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{
    OneAssetOptionEngine, OneAssetOptionResults, OptionArguments, PlainVanillaPayoff,
    StrikedTypePayoff, TypePayoff,
};
use crate::math::distributions::normal::CumulativeNormalDistribution;
use crate::models::GjrGarchModel;
use crate::models::model::CalibratedModelHolder;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::shared::SharedMut;
use crate::types::Real;
use crate::{fail, require};
use coefficients::Coefficients;
use moments::Moments;
use std::any::Any;

/// Live-model European plain-vanilla Edgeworth approximation, NPV only.
///
/// Preserves QuantLib's pricing convention: the call is not multiplied by
/// the dividend discount, and put-call parity is `put - call = strike *
/// risk_free_discount / dividend_discount - spot`. These are source-parity
/// values, not a correction to conventional discounted option pricing.
/// The truncated expansion can produce negative values or violate arbitrage
/// bounds; finite outputs are deliberately not clamped or replaced.
///
/// Calculation rejects nonpositive strikes, nonpositive time, a rounded
/// daily horizon outside `1..=1000`, ill-conditioned moment denominators,
/// zero expected variance and nonfinite moments or prices. Denominators
/// within `64 * f64::EPSILON * max(1, |a|, |b|)` of a pole are rejected. The direct source
/// summation is cubic in the daily horizon and uses linear temporary storage.
/// No Greeks are supplied.
pub struct AnalyticGjrGarchEngine {
    base: OneAssetOptionEngine,
    model: SharedMut<GjrGarchModel>,
    moments: Option<(MomentKey, Moments)>,
}

#[derive(Clone, Copy, PartialEq)]
struct MomentKey {
    parameters: [Real; 6],
    days: usize,
    rate: Real,
}

impl AnalyticGjrGarchEngine {
    /// Observes the model and retains it for live parameter and market reads.
    pub fn new(model: SharedMut<GjrGarchModel>) -> Self {
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        Self {
            base,
            model,
            moments: None,
        }
    }
}

impl AsObservable for AnalyticGjrGarchEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for AnalyticGjrGarchEngine {
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
        require!(
            exercise.exercise_type() == ExerciseType::European,
            "AnalyticGjrGarchEngine requires European exercise"
        );
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let Some(payoff) = (&**payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("AnalyticGjrGarchEngine requires plain-vanilla payoff");
        };
        let strike = payoff.strike();
        let option_type = payoff.option_type();
        let maturity = exercise.last_date();
        require!(
            strike.is_finite() && strike > 0.0,
            "GJR-GARCH analytic strike must be finite and positive"
        );
        let process = self.model.borrow().process();
        let spot = process.s0().current_link()?.value()?;
        require!(
            spot.is_finite() && spot > 0.0,
            "GJR-GARCH analytic spot must be finite and positive"
        );
        let risk_free_discount = process
            .risk_free_rate()
            .current_link()?
            .discount_date(maturity, false)?;
        let dividend_discount = process
            .dividend_yield()
            .current_link()?
            .discount_date(maturity, false)?;
        require!(
            risk_free_discount.is_finite()
                && risk_free_discount > 0.0
                && dividend_discount.is_finite()
                && dividend_discount > 0.0,
            "GJR-GARCH analytic discounts must be finite and positive"
        );
        let term = process.time(&maturity)?;
        let model_days = process.days_per_year() * term;
        let rounded_days = model_days.round();
        require!(
            term.is_finite()
                && term > 0.0
                && model_days.is_finite()
                && (1.0..=1000.0).contains(&rounded_days),
            "GJR-GARCH analytic rounded daily horizon must be between 1 and 1000"
        );
        let days = rounded_days as usize;
        let discount_ratio = risk_free_discount / dividend_discount;
        let rate = -discount_ratio.ln() / model_days;
        require!(
            discount_ratio.is_finite() && discount_ratio > 0.0 && rate.is_finite(),
            "GJR-GARCH analytic daily rate is not finite"
        );
        let parameters = [
            process.omega(),
            process.alpha(),
            process.beta(),
            process.gamma(),
            process.lambda(),
            process.v0(),
        ];
        let key = MomentKey {
            parameters,
            days,
            rate,
        };
        let moments = match self.moments {
            Some((cached_key, cached_moments)) if cached_key == key => cached_moments,
            _ => {
                let c =
                    Coefficients::new(parameters[2], parameters[1], parameters[3], parameters[4])?;
                let moments = Moments::new(c, parameters[0], parameters[5], days, rate)?;
                self.moments = Some((key, moments));
                moments
            }
        };
        let call = approximate_call(moments, spot, strike, rate * days as Real)?;
        let value = match option_type {
            OptionType::Call => call,
            OptionType::Put => call + strike * discount_ratio - spot,
        };
        require!(value.is_finite(), "GJR-GARCH analytic price is not finite");
        self.base.results_mut().instrument.value = Some(value);
        Ok(())
    }
}

fn approximate_call(m: Moments, spot: Real, strike: Real, drift: Real) -> QlResult<Real> {
    let stdev = m.variance.sqrt();
    let del = (m.mean - drift + m.variance / 2.0) / stdev;
    let d = ((spot / strike).ln() + drift + m.variance / 2.0) / stdev;
    let shifted_d = d + del;
    let cdf = CumulativeNormalDistribution::standard();
    let density = (-shifted_d * shifted_d / 2.0).exp() / (2.0 * std::f64::consts::PI).sqrt();
    let asset = spot * (del * stdev).exp();
    let call =
        asset * cdf.value(shifted_d) - strike * (-drift).exp() * cdf.value(shifted_d - stdev);
    let a3 =
        asset * stdev * ((2.0 * stdev - shifted_d) * density + m.variance * cdf.value(shifted_d))
            / 6.0;
    let a4 = asset
        * stdev
        * ((shifted_d * shifted_d - 1.0 - 3.0 * stdev * (shifted_d - stdev)) * density
            - m.variance * stdev * cdf.value(shifted_d))
        / 24.0;
    let value = call + m.skewness * a3 + (m.kurtosis - 3.0) * a4;
    require!(
        [del, d, shifted_d, asset, call, a3, a4, value]
            .iter()
            .all(|x| x.is_finite()),
        "GJR-GARCH analytic expansion is not finite"
    );
    Ok(value)
}
