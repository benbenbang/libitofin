//! Modern EMA-close and Wilder-ATR Keltner channels.
//!
//! Formula and defaults: [StockCharts Keltner channels](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-overlays/keltner-channels).
//! Close-price convention: [TradingView Keltner channels](https://www.tradingview.com/support/solutions/43000502266-keltner-channels-kc/).

use super::{ChartSeries, atr, check_period, ema};
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// EMA-close center and ATR envelopes, aligned after both indicator warmups.
#[derive(Clone, Debug, PartialEq)]
pub struct KeltnerChannels {
    pub center: ChartSeries,
    pub upper: ChartSeries,
    pub lower: ChartSeries,
}

/// Modern Keltner channels with independently configurable EMA and ATR periods.
///
/// The center preserves [`ema`]'s arithmetic close-price seed. Envelopes are
/// `center +/- multiplier * atr`, using Wilder [`atr`]. All three series share
/// the later first-valid index, with zero placeholders before it. This is not
/// the original typical-price SMA and high-low-range version of Keltner bands.
///
/// # Errors
///
/// Rejects zero periods, a negative or nonfinite multiplier, invalid HLC inputs,
/// allocation failure, and nonfinite arithmetic, including an overflowing band
/// offset even if a subsequent operation could cancel it. HLC and true ranges
/// are validated even when the input is too short to produce a valid channel.
pub fn keltner_channels(
    high: &[Real],
    low: &[Real],
    close: &[Real],
    center_period: usize,
    atr_period: usize,
    multiplier: Real,
) -> QlResult<KeltnerChannels> {
    check_period(center_period)?;
    check_period(atr_period)?;
    require!(
        multiplier.is_finite() && multiplier >= 0.0,
        "Keltner multiplier must be finite and nonnegative"
    );
    let mut upper = atr(high, low, close, atr_period)?;
    let mut center = ema(close, center_period)?;
    let first_valid = center.first_valid.max(upper.first_valid);
    let mut lower = ChartSeries::zeroed(close.len(), first_valid)?;
    center.first_valid = first_valid;
    upper.first_valid = first_valid;
    center.values[..first_valid].fill(0.0);
    upper.values[..first_valid].fill(0.0);
    for index in first_valid..close.len() {
        let offset = multiplier * upper.values[index];
        require!(
            offset.is_finite(),
            "nonfinite Keltner offset at index {index}"
        );
        let high_band = center.values[index] + offset;
        let low_band = center.values[index] - offset;
        require!(
            high_band.is_finite() && low_band.is_finite(),
            "nonfinite Keltner band at index {index}"
        );
        upper.values[index] = high_band;
        lower.values[index] = low_band;
    }
    Ok(KeltnerChannels {
        center,
        upper,
        lower,
    })
}

/// Modern Keltner channels using EMA-close 20, Wilder ATR 10, and multiplier 2.
///
/// The ATR period here is 10, distinct from [`super::atr_default`]'s period 14.
///
/// # Errors
///
/// Returns the same errors as [`keltner_channels`].
pub fn keltner_channels_default(
    high: &[Real],
    low: &[Real],
    close: &[Real],
) -> QlResult<KeltnerChannels> {
    keltner_channels(high, low, close, 20, 10, 2.0)
}

#[cfg(test)]
#[path = "keltner_tests.rs"]
mod tests;
