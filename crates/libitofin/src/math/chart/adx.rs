//! Transition-seeded Wilder directional movement and trend strength.

use super::{ChartSeries, check_period, true_range};
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// Four aligned directional indicators with separate warmup metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct Adx {
    /// Positive directional indicator, valid from index `period`.
    pub plus_di: ChartSeries,
    /// Negative directional indicator, valid from index `period`.
    pub minus_di: ChartSeries,
    /// Directional index, valid from index `period`.
    pub dx: ChartSeries,
    /// Wilder-smoothed directional index, valid from index `2*period-1`.
    pub adx: ChartSeries,
}

fn wilder(values: &[Real], period: usize, start: usize) -> QlResult<ChartSeries> {
    let first = start.saturating_add(period - 1);
    let mut result = ChartSeries::zeroed(values.len(), first)?;
    if first >= values.len() {
        return Ok(result);
    }
    let mut mean = 0.0;
    for (index, value) in values[start..=first].iter().enumerate() {
        mean += (*value - mean) / (index + 1) as Real;
        require!(mean.is_finite(), "nonfinite Wilder seed");
    }
    result.values[first] = mean;
    for (index, value) in values.iter().enumerate().skip(first + 1) {
        mean = if period == 1 {
            *value
        } else {
            mean + (*value - mean) / period as Real
        };
        require!(mean.is_finite(), "nonfinite Wilder value at index {index}");
        result.values[index] = mean;
    }
    Ok(result)
}

/// Compute Wilder +DI, -DI, DX and ADX from ordered finite HLC bars.
///
/// For each transition, `up=high[i]-high[i-1]` and
/// `down=low[i-1]-low[i]`. Only the strictly larger positive movement is
/// selected; equal movements select neither. Bar zero has no movement.
/// TR and both movements seed their means from bars `1..=period`, excluding
/// bar zero unlike [`super::atr`]. Subsequent means follow
/// `previous+(current-previous)/period`. DI is `100*mean_DM/mean_TR`.
/// Zero mean TR sets both DI to zero; zero DI sum sets DX to zero, otherwise
/// DX is `100*abs(plus_DI-minus_DI)/(plus_DI+minus_DI)`.
///
/// DI and DX start at `period`; ADX seeds the first `period` valid DX values
/// and starts at `2*period-1`. Validity indices are capped at input length.
/// Earlier entries are zero placeholders. Period one is supported and all
/// indicators start at index one. Defaults are provided by [`adx_default`].
///
/// # Errors
///
/// Rejects zero periods, invalid HLC, allocation failure and nonfinite raw
/// differences or derived arithmetic, including differences during warmup.
pub fn adx(high: &[Real], low: &[Real], close: &[Real], period: usize) -> QlResult<Adx> {
    check_period(period)?;
    let ranges = true_range(high, low, close)?;
    let mut plus = ChartSeries::zeroed(close.len(), 0)?;
    let mut minus = ChartSeries::zeroed(close.len(), 0)?;
    for index in 1..close.len() {
        let up = high[index] - high[index - 1];
        let down = low[index - 1] - low[index];
        require!(
            up.is_finite() && down.is_finite(),
            "nonfinite directional movement at index {index}"
        );
        if up > 0.0 && up > down {
            plus.values[index] = up;
        } else if down > 0.0 && down > up {
            minus.values[index] = down;
        }
    }
    let ranges = wilder(&ranges.values, period, 1)?;
    let mut plus_di = wilder(&plus.values, period, 1)?;
    let mut minus_di = wilder(&minus.values, period, 1)?;
    let mut dx = ChartSeries::zeroed(close.len(), period)?;
    for index in dx.first_valid..close.len() {
        let range = ranges.values[index];
        plus_di.values[index] = if range == 0.0 {
            0.0
        } else {
            (plus_di.values[index] / range) * 100.0
        };
        minus_di.values[index] = if range == 0.0 {
            0.0
        } else {
            (minus_di.values[index] / range) * 100.0
        };
        let sum = plus_di.values[index] + minus_di.values[index];
        let value = if sum == 0.0 {
            0.0
        } else {
            ((plus_di.values[index] - minus_di.values[index]).abs() / sum) * 100.0
        };
        require!(
            plus_di.values[index].is_finite()
                && minus_di.values[index].is_finite()
                && sum.is_finite()
                && value.is_finite(),
            "nonfinite directional index at index {index}"
        );
        dx.values[index] = value;
    }
    let smoothed = wilder(&dx.values, period, period)?;
    Ok(Adx {
        plus_di,
        minus_di,
        dx,
        adx: smoothed,
    })
}

/// Compute Wilder ADX/DMI using the conventional 14-transition period.
///
/// # Errors
/// Returns the same validation, allocation and arithmetic errors as [`adx`].
pub fn adx_default(high: &[Real], low: &[Real], close: &[Real]) -> QlResult<Adx> {
    adx(high, low, close, 14)
}

#[cfg(test)]
#[path = "adx_tests.rs"]
mod tests;
