//! Benchmark beta over aligned caller-supplied return observations.

use super::{SequenceStatistic, SequenceStatistics, validate_sequence_shape};
use crate::{errors::QlResult, require, types::Real};

/// Weighted asset/benchmark covariance divided by benchmark sample variance.
///
/// Observations must already align by date and return frequency. No data feed,
/// annualization or risk-free adjustment is applied. Omitted weights are one;
/// supplied weights must be finite, nonnegative and have a finite positive sum.
/// Both moments use the observation-count `N/(N-1)` correction, including
/// zero-weight rows, so that common correction cancels in the ratio.
/// Inputs are borrowed and unchanged; at most 100,000 paired rows are supported.
///
/// # Errors
/// Rejects unequal lengths, fewer than two rows, invalid samples/weights,
/// zero benchmark variance (including underflow), and nonfinite moments/beta.
pub fn benchmark_beta(
    asset_returns: &[Real],
    benchmark_returns: &[Real],
    weights: Option<&[Real]>,
) -> QlResult<Real> {
    let rows = asset_returns.len();
    require!(
        rows >= 2,
        "benchmark beta requires at least two observations"
    );
    require!(
        rows == benchmark_returns.len(),
        "asset and benchmark lengths differ"
    );
    validate_sequence_shape(rows, 2, SequenceStatistic::Covariance)?;
    if let Some(weights) = weights {
        require!(
            weights.len() == rows,
            "benchmark beta weight length mismatch"
        );
    }
    let mut statistics = SequenceStatistics::new(2)?;
    for (index, (&asset, &benchmark)) in asset_returns.iter().zip(benchmark_returns).enumerate() {
        statistics.add_weighted(&[asset, benchmark], weights.map_or(1.0, |w| w[index]))?;
    }
    let covariance = statistics.covariance()?;
    require!(
        covariance[3].is_sign_positive() && covariance[3] != 0.0,
        "benchmark variance must be positive"
    );
    let beta = covariance[1] / covariance[3];
    require!(beta.is_finite(), "benchmark beta result is nonfinite");
    Ok(beta)
}

#[cfg(test)]
#[path = "benchmark_beta_tests.rs"]
mod tests;
