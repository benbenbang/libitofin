//! Fixed-parameter GARCH(1,1) filtering for a series of returns.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::chart::ChartSeries;
use crate::math::optimization::constraint::NoConstraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::math::optimization::simplex::Simplex;
use crate::require;
use crate::types::Real;

/// A stationary GARCH(1,1) model parameterized by long-run variance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Garch11 {
    alpha: Real,
    beta: Real,
    omega: Real,
}

/// Conditional volatility at each return and the variance forecast after the last return.
#[derive(Clone, Debug, PartialEq)]
pub struct Garch11Filter {
    pub conditional_volatility: ChartSeries,
    pub next_variance: Real,
}

/// Gaussian maximum-likelihood GARCH(1,1) parameters and the variance forecast.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Garch11Fit {
    /// Weight of the preceding squared return.
    pub alpha: Real,
    /// Weight of the preceding conditional variance.
    pub beta: Real,
    /// Positive variance intercept.
    pub omega: Real,
    /// Mean Gaussian log likelihood without the parameter-independent constant.
    pub log_likelihood: Real,
    /// Variance forecast using the same initialization as [`Garch11::filter`].
    pub next_variance: Real,
}

const FIT_MARGIN: Real = 1.0e-8;
const MAX_FIT_RETURNS: usize = 100_000;

struct GarchFitCost<'a> {
    squared: &'a [Real],
    scale: Real,
    lower: Real,
}

impl CostFunction for GarchFitCost<'_> {
    fn values(&self, x: &Array) -> Array {
        Array::from([self.value(x)])
    }

    fn value(&self, x: &Array) -> Real {
        let (alpha, beta, omega) = fit_decode(x, self.scale, self.lower);
        fit_likelihood(self.squared, alpha, beta, omega).map_or(Real::INFINITY, |(cost, _)| cost)
    }
}

fn fit_sample(returns: &[Real]) -> QlResult<(Vec<Real>, Real)> {
    require!(
        returns.len() >= 4,
        "Data series is too short to fit GARCH model"
    );
    require!(
        returns.len() <= MAX_FIT_RETURNS,
        "GARCH fit supports at most {MAX_FIT_RETURNS} returns"
    );
    let mut squared = Vec::with_capacity(returns.len());
    for (index, &value) in returns.iter().enumerate() {
        require!(value.is_finite(), "nonfinite GARCH return at index {index}");
        let square = value * value;
        require!(
            square.is_finite(),
            "GARCH return square overflow at index {index}"
        );
        squared.push(square);
    }
    let mean = squared.iter().sum::<Real>() / squared.len() as Real;
    require!(
        mean.is_finite() && mean > 0.0,
        "Data series is constant or has nonfinite variance"
    );
    Ok((squared, mean))
}

fn fit_acf(squared: &[Real], mean: Real) -> QlResult<Vec<Real>> {
    let centered: Vec<Real> = squared.iter().map(|&value| value - mean).collect();
    let max_lag = (squared.len() as Real).sqrt() as usize;
    let acf: Vec<Real> = (0..=max_lag)
        .map(|lag| {
            centered[lag..]
                .iter()
                .zip(&centered[..centered.len() - lag])
                .map(|(a, b)| a * b)
                .sum::<Real>()
                / (squared.len() - lag) as Real
        })
        .collect();
    require!(
        acf.iter().all(|v| v.is_finite()) && acf[0] > 0.0,
        "Data series is constant or has nonfinite autocovariance"
    );
    Ok(acf)
}

fn fit_likelihood(squared: &[Real], alpha: Real, beta: Real, omega: Real) -> Option<(Real, Real)> {
    let mut variance = 0.0;
    let mut previous_square = 0.0;
    let mut cost = 0.0;
    for &square in squared {
        variance = omega + alpha * previous_square + beta * variance;
        if !variance.is_finite() || variance <= 0.0 {
            return None;
        }
        cost += (variance.ln() + square / variance) / (2.0 * squared.len() as Real);
        if !cost.is_finite() {
            return None;
        }
        previous_square = square;
    }
    let next_variance = omega + alpha * previous_square + beta * variance;
    (next_variance.is_finite() && next_variance > 0.0).then_some((cost, next_variance))
}

fn fit_encode(alpha: Real, beta: Real, omega: Real, scale: Real, lower: Real) -> Vec<Real> {
    let gamma = alpha + beta;
    let span = 1.0 - FIT_MARGIN - lower;
    let proportion = ((gamma - lower) / span).clamp(FIT_MARGIN, 1.0 - FIT_MARGIN);
    let alpha_fraction =
        if gamma == 0.0 { 0.5 } else { alpha / gamma }.clamp(FIT_MARGIN, 1.0 - FIT_MARGIN);
    vec![
        (omega / scale).ln(),
        (proportion / (1.0 - proportion)).ln(),
        (alpha_fraction / (1.0 - alpha_fraction)).ln(),
    ]
}

fn fit_decode(x: &[Real], scale: Real, lower: Real) -> (Real, Real, Real) {
    let omega = scale * x[0].exp();
    let gamma = lower + (1.0 - FIT_MARGIN - lower) / (1.0 + (-x[1]).exp());
    let alpha = gamma / (1.0 + (-x[2]).exp());
    (alpha, gamma - alpha, omega)
}

fn fit_from_start(
    squared: &[Real],
    mean: Real,
    lower: Real,
    start: (Real, Real, Real),
) -> QlResult<Garch11Fit> {
    let x0 = fit_encode(start.0, start.1, start.2, mean, lower);
    require!(
        x0.iter().all(|value| value.is_finite()),
        "nonfinite GARCH starting transform"
    );
    let cost = GarchFitCost {
        squared,
        scale: mean,
        lower,
    };
    let constraint = NoConstraint;
    let mut problem = Problem::new(&cost, &constraint, Array::from(x0));
    let end = EndCriteria::new(10_000, Some(500), 1.0e-8, 1.0e-8, Some(1.0e-8))?;
    let status = Simplex::new(0.1).minimize(&mut problem, &end)?;
    require!(
        status != EndCriteriaType::MaxIterations,
        "GARCH optimizer did not converge: {status}"
    );
    let (alpha, beta, omega) = fit_decode(problem.current_value(), mean, lower);
    let Some((cost, _)) = fit_likelihood(squared, alpha, beta, omega) else {
        crate::fail!("nonfinite GARCH fitted likelihood or forecast");
    };
    require!(
        omega > 0.0 && alpha >= 0.0 && beta >= 0.0 && alpha + beta < 1.0 - FIT_MARGIN,
        "GARCH fitted parameters violate the stationary constraint"
    );
    let long_run_variance = omega / (1.0 - alpha - beta);
    let model = Garch11::new(alpha, beta, long_run_variance)?;
    let mut variance = squared[0];
    for &square in squared {
        variance = model.omega + model.alpha * square + model.beta * variance;
        require!(
            variance.is_finite(),
            "nonfinite GARCH fitted variance forecast"
        );
    }
    Ok(Garch11Fit {
        alpha,
        beta,
        omega,
        log_likelihood: -cost,
        next_variance: variance,
    })
}

impl Garch11 {
    /// Fits stationary GARCH(1,1) parameters by Gaussian maximum likelihood.
    /// Two starting points are estimated from the autocovariance of squared returns.
    ///
    /// # Errors
    /// Rejects fewer than four or more than 100,000 observations, nonfinite or
    /// degenerate samples, nonfinite intermediate values, and non-convergence.
    pub fn fit(returns: &[Real]) -> QlResult<Garch11Fit> {
        let (squared, mean) = fit_sample(returns)?;
        let acf = fit_acf(&squared, mean)?;
        let fourth_moment = acf[0] + mean * mean;
        require!(
            fourth_moment.is_finite() && fourth_moment > 0.0,
            "nonfinite GARCH fourth moment"
        );
        let a = mean * mean / fourth_moment;
        let b = acf[1] / fourth_moment;
        require!(a.is_finite() && b.is_finite(), "nonfinite GARCH ACF ratios");
        let lower = if a <= 1.0 / 3.0 - FIT_MARGIN {
            ((1.0 - 3.0 * a) / (3.0 - 3.0 * a)).sqrt() + FIT_MARGIN
        } else {
            FIT_MARGIN
        };
        require!(
            lower.is_finite() && lower < 1.0 - FIT_MARGIN,
            "GARCH ACF cannot produce a stationary start"
        );
        let moment_gamma = lower + (1.0 - FIT_MARGIN - lower) * 0.5;
        let mut gamma_sum = 0.0;
        let mut gamma_count = 0;
        for lag in 2..acf.len() {
            if acf[lag] > 0.0 && acf[lag - 1] > acf[lag] {
                gamma_sum += acf[lag] / acf[lag - 1];
                gamma_count += 1;
            }
        }
        let acf_gamma = if gamma_count > 0 {
            (gamma_sum / gamma_count as Real).clamp(lower, 1.0 - 2.0 * FIT_MARGIN)
        } else {
            lower + FIT_MARGIN
        };
        let start = |gamma: Real| {
            let beta = (gamma * (1.0 - a) - b).clamp(0.0, gamma);
            (gamma - beta, beta, mean * (1.0 - gamma))
        };
        let first = fit_from_start(&squared, mean, lower, start(moment_gamma));
        let second = fit_from_start(&squared, mean, lower, start(acf_gamma));
        match (first, second) {
            (Ok(a), Ok(b)) => Ok(if a.log_likelihood >= b.log_likelihood {
                a
            } else {
                b
            }),
            (Ok(fit), Err(_)) | (Err(_), Ok(fit)) => Ok(fit),
            (Err(first), Err(second)) => crate::fail!(
                "GARCH fitting did not converge from either ACF start: {first}; {second}"
            ),
        }
    }

    /// Fits from caller-provided `alpha`, `beta`, and variance intercept `omega`.
    ///
    /// # Errors
    /// Rejects samples outside four to 100,000 returns, invalid starts,
    /// nonfinite intermediate values, and optimization that does not converge.
    pub fn fit_with_start(
        returns: &[Real],
        alpha: Real,
        beta: Real,
        omega: Real,
    ) -> QlResult<Garch11Fit> {
        let (squared, mean) = fit_sample(returns)?;
        require!(
            alpha.is_finite() && alpha >= 0.0,
            "GARCH starting alpha must be finite and nonnegative"
        );
        require!(
            beta.is_finite() && beta >= 0.0,
            "GARCH starting beta must be finite and nonnegative"
        );
        require!(
            omega.is_finite() && omega > 0.0,
            "GARCH starting omega must be finite and positive"
        );
        require!(
            (alpha + beta).total_cmp(&(1.0 - FIT_MARGIN)).is_lt(),
            "GARCH starting alpha plus beta must be below one"
        );
        fit_from_start(&squared, mean, 0.0, (alpha, beta, omega))
    }

    /// Builds a stationary model with `omega = (1 - alpha - beta) * long_run_variance`.
    ///
    /// # Errors
    /// Returns an error for nonfinite or negative parameters, or `alpha + beta >= 1`.
    pub fn new(alpha: Real, beta: Real, long_run_variance: Real) -> QlResult<Self> {
        require!(
            alpha.is_finite() && alpha >= 0.0,
            "GARCH alpha must be finite and nonnegative"
        );
        require!(
            beta.is_finite() && beta >= 0.0,
            "GARCH beta must be finite and nonnegative"
        );
        require!(
            long_run_variance.is_finite() && long_run_variance >= 0.0,
            "GARCH long-run variance must be finite and nonnegative"
        );
        require!(
            (alpha + beta).total_cmp(&1.0).is_lt(),
            "GARCH alpha plus beta must be below one"
        );
        let omega = (1.0 - alpha - beta) * long_run_variance;
        require!(omega.is_finite(), "nonfinite GARCH intercept");
        Ok(Self { alpha, beta, omega })
    }

    /// Forecasts the next conditional variance from the latest return and variance.
    ///
    /// # Errors
    /// Returns an error for invalid inputs or a nonfinite update.
    pub fn forecast(&self, return_value: Real, current_variance: Real) -> QlResult<Real> {
        require!(return_value.is_finite(), "nonfinite GARCH return");
        require!(
            current_variance.is_finite() && current_variance >= 0.0,
            "GARCH current variance must be finite and nonnegative"
        );
        let squared_return = return_value * return_value;
        require!(squared_return.is_finite(), "GARCH return square overflow");
        let next_variance = self.omega + self.alpha * squared_return + self.beta * current_variance;
        require!(next_variance.is_finite(), "GARCH variance update overflow");
        Ok(next_variance)
    }

    /// Filters returns using the first squared return as the initial variance.
    /// `conditional_volatility[0]` is a zero placeholder; the first valid value
    /// uses return zero and appears at index one. The final return contributes to
    /// `next_variance`, which is in variance units rather than volatility units.
    ///
    /// # Errors
    /// Returns an error for empty or nonfinite returns, squared-return overflow,
    /// or a nonfinite variance update.
    pub fn filter(&self, returns: &[Real]) -> QlResult<Garch11Filter> {
        require!(!returns.is_empty(), "GARCH returns must not be empty");
        for (index, value) in returns.iter().enumerate() {
            require!(value.is_finite(), "nonfinite GARCH return at index {index}");
            require!(
                (value * value).is_finite(),
                "GARCH return square overflow at index {index}"
            );
        }
        let mut conditional_volatility = ChartSeries::zeroed(returns.len(), 1)?;
        let mut variance = returns[0] * returns[0];
        for index in 1..returns.len() {
            variance = self.forecast(returns[index - 1], variance)?;
            conditional_volatility.values[index] = variance.sqrt();
        }
        let next_variance = self.forecast(returns[returns.len() - 1], variance)?;
        Ok(Garch11Filter {
            conditional_volatility,
            next_variance,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Garch11 {
        Garch11::new(0.2, 0.3, 0.4).unwrap()
    }

    #[test]
    fn quantlib_fixed_parameter_fixture_and_forecast() {
        let result = model().filter(&[0.1, 0.1, 0.1]).unwrap();
        assert_eq!(result.conditional_volatility.first_valid, 1);
        assert_eq!(result.conditional_volatility.values[0], 0.0);
        assert_eq!(result.conditional_volatility.get(0), None);
        assert!((result.conditional_volatility.values[1] - 0.452_769_256_906_870_87).abs() < 1e-14);
        assert!((result.conditional_volatility.values[2] - 0.513_322_510_708_423_9).abs() < 1e-14);
        assert!((result.next_variance - 0.281_05).abs() < 1e-14);
        assert_eq!(result.next_variance, model().forecast(0.1, 0.2635).unwrap());
    }

    #[test]
    fn one_return_produces_only_a_forecast() {
        let result = model().filter(&[0.1]).unwrap();
        assert_eq!(result.conditional_volatility.values, [0.0]);
        assert_eq!(result.conditional_volatility.first_valid, 1);
        assert!((result.next_variance - 0.205).abs() < 1e-14);
    }

    #[test]
    fn quantlib_longer_fixture() {
        let result = model().filter(&[0.1; 10]).unwrap();
        assert!((result.conditional_volatility.values[9] - 0.537_183_344_352_745_2).abs() < 1e-14);
        assert!((result.next_variance - 0.288_569_783_635).abs() < 1e-14);
    }

    #[test]
    fn differing_returns_use_the_preceding_observation() {
        let result = model().filter(&[0.1, 0.2, 0.3]).unwrap();
        assert!((result.conditional_volatility.values[1] - 0.205_f64.sqrt()).abs() < 1e-14);
        assert!((result.conditional_volatility.values[2] - 0.2695_f64.sqrt()).abs() < 1e-14);
        assert!((result.next_variance - 0.29885).abs() < 1e-14);
    }

    #[test]
    fn rejects_invalid_parameters() {
        for (alpha, beta, long_run_variance) in [
            (-0.1, 0.2, 0.4),
            (f64::NAN, 0.2, 0.4),
            (0.2, f64::INFINITY, 0.4),
            (0.2, -0.1, 0.4),
            (0.2, 0.3, -0.4),
            (0.2, 0.3, f64::INFINITY),
            (0.2, 0.8, 0.4),
            (1.0, 0.0, 0.4),
        ] {
            assert!(Garch11::new(alpha, beta, long_run_variance).is_err());
        }
        assert!(Garch11::new(0.0, 0.0, 0.0).is_ok());
    }

    #[test]
    fn rejects_invalid_returns_and_variances() {
        let model = model();
        assert!(model.filter(&[]).is_err());
        for returns in [
            vec![f64::NAN],
            vec![0.1, f64::INFINITY],
            vec![f64::MAX],
            vec![0.1, f64::MAX],
        ] {
            assert!(model.filter(&returns).is_err());
        }
        for variance in [f64::NAN, f64::INFINITY, -0.1] {
            assert!(model.forecast(0.1, variance).is_err());
        }
        assert!(model.forecast(f64::NAN, 0.1).is_err());
        assert!(model.forecast(f64::MAX, 0.1).is_err());
    }
}
