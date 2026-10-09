//! Explicitly annualized, unweighted performance ratios over signed returns.

use crate::errors::QlResult;
use crate::{ensure, require};

fn validate(returns: &[f64], target: f64, minimum: usize) -> QlResult<()> {
    require!(returns.len() >= minimum, "insufficient return observations");
    require!(target.is_finite(), "per-period target must be finite");
    for &value in returns {
        require!(value.is_finite(), "return observations must be finite");
        let excess = value - target;
        ensure!(excess.is_finite(), "nonfinite excess return");
    }
    Ok(())
}

fn downside_scale(returns: &[f64], target: f64) -> f64 {
    returns
        .iter()
        .map(|&value| (target - value).max(0.0))
        .fold(0.0, f64::max)
}

fn downside_scaled(returns: &[f64], target: f64, scale: f64) -> f64 {
    if scale == 0.0 {
        return 0.0;
    }
    let sum: f64 = returns
        .iter()
        .map(|&value| (((target - value).max(0.0)) / scale).powi(2))
        .sum();
    (sum / returns.len() as f64).sqrt()
}

fn moments(returns: &[f64]) -> (f64, f64, f64, f64) {
    let origin = returns[0];
    let anchored = returns.iter().all(|value| (value - origin).is_finite());
    let shift = if anchored { origin } else { 0.0 };
    let scale = returns
        .iter()
        .map(|value| (value - shift).abs())
        .fold(0.0, f64::max);
    if scale == 0.0 {
        return (origin, 0.0, 0.0, 0.0);
    }
    let mut sum = 0.0;
    let mut correction = 0.0;
    for &value in returns {
        let term = (value - shift) / scale - correction;
        let next = sum + term;
        correction = (next - sum) - term;
        sum = next;
    }
    let center = (sum / returns.len() as f64).clamp(-1.0, 1.0);
    let squares = returns
        .iter()
        .map(|value| ((value - shift) / scale - center).powi(2))
        .sum::<f64>();
    let deviation = if returns.len() > 1 {
        (squares / (returns.len() - 1) as f64).sqrt()
    } else {
        0.0
    };
    (shift, scale, center, deviation)
}

/// Root mean squared shortfall below a scalar per-period target.
///
/// Returns `sqrt(sum(min(return - target, 0)^2) / N)`, dividing by ALL
/// observations with no sample correction. Returns zero for an all-above-target
/// sample. This is not the conditional QuantLib `downside_deviation` measure.
/// Signed simple returns and target use the same units, normally decimals.
///
/// # Errors
/// Errors on empty/nonfinite inputs or nonfinite subtraction/output arithmetic.
pub fn target_downside_deviation(returns: &[f64], target: f64) -> QlResult<f64> {
    validate(returns, target, 1)?;
    let scale = downside_scale(returns, target);
    let result = scale * downside_scaled(returns, target, scale);
    ensure!(result.is_finite(), "nonfinite target downside deviation");
    Ok(result)
}

fn ratio_inputs(returns: &[f64], target: f64, periods_per_year: f64) -> QlResult<(f64, f64, f64)> {
    require!(
        periods_per_year.is_finite() && periods_per_year > 0.0,
        "periods per year must be finite and positive"
    );
    validate(returns, target, 2)?;
    let (shift, scale, center, deviation) = moments(returns);
    let excess_mean = (shift - target) + scale * center;
    ensure!(excess_mean.is_finite(), "nonfinite arithmetic excess mean");
    Ok((excess_mean, scale, deviation))
}

fn annualize(mean_scaled: f64, deviation_scaled: f64, periods_per_year: f64) -> QlResult<f64> {
    require!(
        deviation_scaled.is_finite() && deviation_scaled > 0.0,
        "performance ratio denominator is zero"
    );
    let result = (mean_scaled / deviation_scaled) * periods_per_year.sqrt();
    ensure!(result.is_finite(), "nonfinite performance ratio");
    Ok(result)
}

/// Arithmetic excess-return mean divided by sample standard deviation,
/// multiplied by `sqrt(periods_per_year)`.
///
/// `risk_free_return` is a finite scalar return for the SAME period as each
/// observation, not an annual rate. Dispersion divides centered squares by
/// `N-1`. At least two unweighted signed simple returns are required.
/// Frequency is mandatory and never inferred. Square-root annualization is a
/// statistical convention, not a compounded annual-return calculation; serial
/// dependence and changing distributions can invalidate that scaling.
///
/// # Errors
/// Errors on invalid/nonfinite inputs, nonpositive frequency, a zero sample
/// deviation, or nonfinite arithmetic. No artificial epsilon denominator is used.
pub fn sharpe_ratio(
    returns: &[f64],
    risk_free_return: f64,
    periods_per_year: f64,
) -> QlResult<f64> {
    let (excess_mean, scale, deviation) =
        ratio_inputs(returns, risk_free_return, periods_per_year)?;
    require!(scale != 0.0, "performance ratio denominator is zero");
    annualize(excess_mean / scale, deviation, periods_per_year)
}

/// Arithmetic return mean minus MAR, divided by all-observation downside
/// deviation, multiplied by `sqrt(periods_per_year)`.
///
/// `minimum_acceptable_return` (MAR) is a finite scalar for the SAME period as
/// each return. Downside squares divide by `N`, not the below-MAR count or
/// `N-1`. At least two unweighted signed simple returns are required. A sample
/// with no shortfalls has an undefined ratio and returns an error, not infinity.
/// Square-root scaling is explicit, not exact annual compounded downside risk.
///
/// # Errors
/// Errors on invalid/nonfinite inputs, nonpositive frequency, zero downside
/// deviation, or nonfinite arithmetic.
pub fn sortino_ratio(
    returns: &[f64],
    minimum_acceptable_return: f64,
    periods_per_year: f64,
) -> QlResult<f64> {
    let (excess_mean, _, _) = ratio_inputs(returns, minimum_acceptable_return, periods_per_year)?;
    let scale = downside_scale(returns, minimum_acceptable_return);
    require!(scale != 0.0, "performance ratio denominator is zero");
    annualize(
        excess_mean / scale,
        downside_scaled(returns, minimum_acceptable_return, scale),
        periods_per_year,
    )
}
