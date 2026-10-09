//! Rolling Williams percent R with explicit alignment and arithmetic validation.

use std::collections::VecDeque;

use super::{ChartSeries, check_hlc, check_period};
use crate::errors::{QlError, QlResult};
use crate::require;
use crate::types::Real;

/// Williams percent R on an inclusive trailing window of `period` bars.
///
/// Values are `-100 * ((highest_high - close) / (highest_high - lowest_low))`
/// in `[-100, 0]`. A flat window returns -50, matching KD's neutral RSV 50.
/// First-valid is `min(period - 1, len)`; earlier slots are zero placeholders.
/// Partial windows are still checked for overflowing differences during warmup.
/// Negative prices are valid. Complexity is linear in the number of bars.
///
/// # Errors
///
/// Rejects a zero period, mismatched lengths, nonfinite or unordered HLC,
/// allocation failure, and nonfinite rolling differences or output values.
pub fn williams_r(
    high: &[Real],
    low: &[Real],
    close: &[Real],
    period: usize,
) -> QlResult<ChartSeries> {
    check_period(period)?;
    check_hlc(high, low, close)?;
    let mut result = ChartSeries::zeroed(close.len(), period - 1)?;
    let mut maxima = VecDeque::<usize>::new();
    let mut minima = VecDeque::<usize>::new();
    for queue in [&mut maxima, &mut minima] {
        queue
            .try_reserve_exact(period.min(close.len()))
            .map_err(|_| QlError::new("Williams R window allocation failed", file!(), line!()))?;
    }
    for index in 0..close.len() {
        let start = index.saturating_sub(period - 1);
        while maxima.front().is_some_and(|previous| *previous < start) {
            maxima.pop_front();
        }
        while minima.front().is_some_and(|previous| *previous < start) {
            minima.pop_front();
        }
        while maxima
            .back()
            .is_some_and(|previous| high[*previous] <= high[index])
        {
            maxima.pop_back();
        }
        while minima
            .back()
            .is_some_and(|previous| low[*previous] >= low[index])
        {
            minima.pop_back();
        }
        maxima.push_back(index);
        minima.push_back(index);
        let highest = high[maxima[0]];
        let lowest = low[minima[0]];
        let span = highest - lowest;
        let shortfall = highest - close[index];
        require!(
            span.is_finite() && shortfall.is_finite(),
            "nonfinite Williams R difference at index {index}"
        );
        if index >= result.first_valid {
            let value = if span == 0.0 {
                -50.0
            } else {
                -100.0 * (shortfall / span)
            };
            require!(
                value.is_finite() && (-100.0..=0.0).contains(&value),
                "invalid Williams R output at index {index}"
            );
            result.values[index] = value;
        }
    }
    Ok(result)
}

/// Williams percent R with the conventional 14-bar period.
///
/// # Errors
///
/// Returns the same validation, allocation, and arithmetic errors as [`williams_r`].
pub fn williams_r_default(high: &[Real], low: &[Real], close: &[Real]) -> QlResult<ChartSeries> {
    williams_r(high, low, close, 14)
}

#[cfg(test)]
#[path = "williams_tests.rs"]
mod tests;
