//! Gap-aware true range and Wilder average true range.
//!
//! Formula reference: [Fidelity's ATR guide](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/atr).
//! First-bar and seed convention: [AAII's ATR article](https://www.aaii.com/journal/article/average-true-range-atr).

use super::{ChartSeries, check_hlc, check_period};
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// Gap-aware true range, valid from the first bar.
///
/// The first value is `high - low`. Later values are the maximum of
/// `high - low`, `abs(high - previous_close)`, and `abs(low - previous_close)`.
///
/// # Errors
///
/// Rejects mismatched lengths, nonfinite or unordered HLC inputs, allocation
/// failure, and nonfinite differences, even if another candidate is finite.
pub fn true_range(high: &[Real], low: &[Real], close: &[Real]) -> QlResult<ChartSeries> {
    check_hlc(high, low, close)?;
    let mut result = ChartSeries::zeroed(close.len(), 0)?;
    for index in 0..close.len() {
        let span = high[index] - low[index];
        require!(span.is_finite(), "nonfinite true range at index {index}");
        let value = if index == 0 {
            span
        } else {
            let high_gap = (high[index] - close[index - 1]).abs();
            let low_gap = (low[index] - close[index - 1]).abs();
            require!(
                high_gap.is_finite() && low_gap.is_finite(),
                "nonfinite true range at index {index}"
            );
            span.max(high_gap).max(low_gap)
        };
        result.values[index] = value;
    }
    Ok(result)
}

/// Wilder ATR seeded by the arithmetic mean of the first `period` true ranges.
///
/// The first bar contributes its high-low range. The first valid index is
/// `period - 1`, capped at the input length. Earlier entries are zero
/// placeholders. Later values follow `previous + (true_range - previous)/period`.
/// Period one returns exactly the true-range series.
///
/// # Errors
///
/// Rejects a zero period and every error reported by [`true_range`], including
/// invalid bars or overflowing differences during an incomplete warmup.
pub fn atr(high: &[Real], low: &[Real], close: &[Real], period: usize) -> QlResult<ChartSeries> {
    check_period(period)?;
    let mut result = true_range(high, low, close)?;
    if period == 1 {
        return Ok(result);
    }
    result.first_valid = (period - 1).min(close.len());
    if close.len() < period {
        result.values.fill(0.0);
        return Ok(result);
    }
    let mut mean = 0.0;
    for (index, value) in result.values[..period].iter().enumerate() {
        mean += (value - mean) / (index + 1) as Real;
    }
    require!(mean.is_finite(), "nonfinite ATR seed");
    result.values[..period - 1].fill(0.0);
    result.values[period - 1] = mean;
    for (index, value) in result.values.iter_mut().enumerate().skip(period) {
        mean += (*value - mean) / period as Real;
        require!(mean.is_finite(), "nonfinite ATR at index {index}");
        *value = mean;
    }
    Ok(result)
}

/// Wilder ATR with the conventional period of 14 bars.
///
/// # Errors
///
/// Returns the same input, allocation, and arithmetic errors as [`atr`].
pub fn atr_default(high: &[Real], low: &[Real], close: &[Real]) -> QlResult<ChartSeries> {
    atr(high, low, close, 14)
}

#[cfg(test)]
#[path = "true_range_atr_tests.rs"]
mod tests;
