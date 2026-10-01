//! Merton's European plain-vanilla Poisson mixture.
//!
//! Conditional Black prices use QuantLib's adjusted intensity and theta
//! correction. The evaluation budget is a hard bound, unlike the upstream
//! loop, which can overrun it before reaching the Poisson mode. Logarithmic
//! weights retain QuantLib's factorial convention and avoid losing the distribution
//! when its zero-count mass underflows. Asset- and strike-weighted tail bounds
//! prevent zero early conditional payoffs from falsely proving convergence.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{
    Greeks, MoreGreeks, OneAssetOptionEngine, OneAssetOptionResults, OptionArguments,
    PlainVanillaPayoff, StrikedTypePayoff,
};
use crate::math::gammafunction::log_gamma;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::BlackCalculator;
use crate::processes::Merton76Process;
use crate::shared::Shared;
use crate::types::Real;
use crate::{fail, require};

/// European plain-vanilla jump-diffusion engine with live market inputs.
pub struct JumpDiffusionEngine {
    base: OneAssetOptionEngine,
    process: Shared<Merton76Process>,
    relative_accuracy: Real,
    max_iterations: usize,
}

impl JumpDiffusionEngine {
    /// Builds an engine; at most `max_iterations` conditional prices are evaluated.
    ///
    /// # Errors
    /// Requires a finite positive tolerance and a budget in `1..=100_000`.
    /// Current process parameters are validated again at each calculation.
    pub fn new(
        process: Shared<Merton76Process>,
        relative_accuracy: Real,
        max_iterations: usize,
    ) -> QlResult<Self> {
        require!(
            relative_accuracy.is_finite() && relative_accuracy > 0.0,
            "relative accuracy must be finite and positive"
        );
        require!(
            (1..=100_000).contains(&max_iterations),
            "max iterations must be between 1 and 100000"
        );
        process.x0()?;
        process.jump_parameters()?;
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(process.observable());
        Ok(Self {
            base,
            process,
            relative_accuracy,
            max_iterations,
        })
    }
}

impl AsObservable for JumpDiffusionEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for JumpDiffusionEngine {
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
            "JumpDiffusionEngine requires European exercise"
        );
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let Some(payoff) = (&**payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("JumpDiffusionEngine requires plain-vanilla payoff");
        };
        let maturity = exercise.last_date();
        let spot = self.process.x0()?;
        let (intensity, jump_variance, jump_exponent, compensation, lambda) =
            self.process.jump_parameters()?;
        let vol = self.process.black_volatility().current_link()?;
        let risk_free = self.process.risk_free_rate().current_link()?;
        let dividend = self.process.dividend_yield().current_link()?;
        let current_vol = vol.black_vol_date(maturity, payoff.strike(), false)?;
        require!(
            current_vol.is_finite() && current_vol >= 0.0,
            "Black volatility must be finite and non-negative"
        );
        let variance = vol.black_variance_date(maturity, payoff.strike(), false)?;
        let t = vol.time_from_reference(maturity)?;
        require!(
            variance.is_finite() && variance >= 0.0,
            "Black variance must be finite and non-negative"
        );
        require!(
            t.is_finite() && t > 0.0,
            "volatility time to maturity must be finite and positive"
        );
        let discount = risk_free.discount_date(maturity, false)?;
        let dividend_discount = dividend.discount_date(maturity, false)?;
        require!(
            discount.is_finite()
                && discount > 0.0
                && dividend_discount.is_finite()
                && dividend_discount > 0.0,
            "market discounts must be finite and positive"
        );
        let vol_dc = vol.require_day_counter()?;
        let u = vol_dc.year_fraction(risk_free.reference_date()?, maturity);
        let dividend_time = dividend
            .require_day_counter()?
            .year_fraction(dividend.reference_date()?, maturity);
        require!(
            u.is_finite() && u > 0.0 && dividend_time.is_finite() && dividend_time >= 0.0,
            "conditional pricing clocks must be finite and positive"
        );
        let rate = -discount.ln() / t;
        let diffusion_vol = (variance / t).sqrt();
        let poisson_mean = lambda * t;
        require!(
            intensity == 0.0 || poisson_mean > 0.0,
            "adjusted Poisson mean is not representable"
        );
        require!(
            poisson_mean.is_finite() && poisson_mean < self.max_iterations as Real,
            "Poisson mode exceeds max iterations"
        );
        let log_mean = if poisson_mean > 0.0 {
            poisson_mean.ln()
        } else {
            0.0
        };
        let strike_mean = if poisson_mean > 0.0 {
            (log_mean - jump_exponent * u / t).exp()
        } else {
            0.0
        };
        require!(
            strike_mean.is_finite() && strike_mean < self.max_iterations as Real,
            "strike-weighted Poisson mode exceeds max iterations"
        );
        let mut factorial: Real = 1.0;
        let mut previous_weight = 0.0;
        let mut sums = [0.0; 7];
        let mut converged = false;
        let mut evaluations = 0;
        for i in 0..self.max_iterations {
            let n = i as Real;
            let log_factorial = if i <= 27 {
                if i > 0 {
                    factorial *= n;
                }
                factorial.ln()
            } else {
                log_gamma(n + 1.0)?
            };
            let log_weight = n * log_mean - log_factorial - poisson_mean;
            let weight = log_weight.exp();
            let conditional_vol = ((variance + n * jump_variance) / t).sqrt();
            let conditional_rate = rate - compensation + n * jump_exponent / t;
            let conditional_discount = (-conditional_rate * u).exp();
            let forward = spot * dividend_discount / conditional_discount;
            let black = BlackCalculator::with_payoff(
                payoff,
                forward,
                conditional_vol * u.sqrt(),
                conditional_discount,
            )?;
            let value = black.value();
            let delta = black.delta(spot)?;
            let zero_strike = payoff.strike() == 0.0;
            let gamma = if zero_strike { 0.0 } else { black.gamma(spot)? };
            let base_vega = if zero_strike { 0.0 } else { black.vega(u)? };
            let rho = black.rho(u)?;
            let dividend_rho = black.dividend_rho(dividend_time)?;
            let theta = -(conditional_discount.ln() * value
                + (forward / spot).ln() * spot * delta
                + 0.5 * conditional_vol * conditional_vol * u * spot * spot * gamma)
                / u;
            let vega = if conditional_vol > 0.0 {
                diffusion_vol / conditional_vol * base_vega
            } else {
                0.0
            };
            let vol_correction = if conditional_vol > 0.0 {
                base_vega * n * jump_variance / (2.0 * conditional_vol * t * t)
            } else {
                0.0
            };
            let theta_correction = vol_correction + rho * n * jump_exponent / (t * t);
            let terms = [value, delta, gamma, theta, base_vega, rho, dividend_rho];
            require!(
                terms.iter().all(|x| x.is_finite())
                    && vega.is_finite()
                    && theta_correction.is_finite(),
                "conditional Merton price or Greeks are not finite"
            );
            sums[0] += weight * value;
            sums[1] += weight * delta;
            sums[2] += weight * gamma;
            sums[3] += weight * (theta + theta_correction + lambda * value)
                - previous_weight * lambda * value;
            sums[4] += weight * vega;
            sums[5] += weight * rho;
            sums[6] += weight * dividend_rho;
            require!(
                sums.iter().all(|x| x.is_finite()),
                "Merton price or Greeks are not finite"
            );
            evaluations = i + 1;
            let last_contribution = terms
                .iter()
                .zip(sums)
                .map(|(term, sum)| {
                    (term / if sum.abs() > Real::EPSILON { sum } else { 1.0 }).abs() * weight
                })
                .fold(0.0, Real::max);
            let asset_tail =
                spot * dividend_discount * poisson_tail_bound(log_weight, poisson_mean, n);
            let strike_tail = payoff.strike()
                * poisson_tail_bound(log_weight + conditional_discount.ln(), strike_mean, n);
            let scale = if sums[0].abs() > Real::EPSILON {
                sums[0].abs()
            } else {
                1.0
            };
            if poisson_mean == 0.0
                || (n >= poisson_mean.max(strike_mean)
                    && asset_tail.max(strike_tail) <= self.relative_accuracy * scale
                    && last_contribution <= self.relative_accuracy)
            {
                converged = true;
                break;
            }
            previous_weight = weight;
        }
        require!(
            converged,
            "max iterations ({}) exhausted before reaching relative accuracy {}",
            self.max_iterations,
            self.relative_accuracy
        );
        let result = self.base.results_mut();
        result.instrument.value = Some(sums[0]);
        result.greeks = Greeks {
            delta: Some(sums[1]),
            gamma: Some(sums[2]),
            theta: Some(sums[3]),
            vega: Some(sums[4]),
            rho: Some(sums[5]),
            dividend_rho: Some(sums[6]),
        };
        result.more_greeks = MoreGreeks {
            theta_per_day: Some(sums[3] / 365.0),
            ..MoreGreeks::default()
        };
        result.instrument.additional_results.insert(
            "evaluations".into(),
            crate::shared::shared(evaluations) as Shared<dyn Any>,
        );
        Ok(())
    }
}

fn poisson_tail_bound(log_mass: Real, mean: Real, count: Real) -> Real {
    if mean == 0.0 {
        return 0.0;
    }
    let ratio = mean / (count + 2.0);
    if ratio >= 1.0 {
        return Real::INFINITY;
    }
    (log_mass + mean.ln() - (count + 1.0).ln()).exp() / (1.0 - ratio)
}

#[cfg(test)]
mod tests;
