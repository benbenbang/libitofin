//! Cumulative volume-weighted prices and zero-seeded on-balance volume.

use super::{ChartSeries, check_close};
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

fn check_volume(prices: &[Real], volume: &[Real]) -> QlResult<()> {
    require!(
        prices.len() == volume.len(),
        "chart price/volume lengths differ"
    );
    check_close(prices)?;
    for (index, value) in volume.iter().enumerate() {
        require!(value.is_finite(), "nonfinite volume at index {index}");
        require!(
            value.partial_cmp(&0.0) != Some(std::cmp::Ordering::Less),
            "negative volume at index {index}"
        );
    }
    Ok(())
}

/// Cumulative VWAP of caller-supplied prices, starting a new session per call.
///
/// Supply trade prices, closes, or precomputed typical prices explicitly.
/// This is not a rolling indicator and does not infer sessions from timestamps.
/// Outputs are aligned with the input and become valid at the first positive
/// volume. An all-zero-volume input has `first_valid == price.len()`; later
/// zero-volume bars preserve the previous VWAP.
///
/// An online weighted mean avoids forming overflowing price-volume products.
/// The first positive-volume bar returns its price exactly, including when its
/// volume is subnormal. Subsequent updates use ordinary floating-point rounding;
/// a sufficiently small relative volume can round its contribution to zero.
///
/// # Errors
///
/// Returns an error for mismatched lengths, nonfinite prices or volumes,
/// negative volumes, cumulative-volume overflow, or nonfinite arithmetic.
///
/// Formula reference: [StockCharts ChartSchool](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-overlays/volume-weighted-average-price-vwap).
/// Session boundaries and missing-value conventions are caller-controlled.
pub fn vwap(price: &[Real], volume: &[Real]) -> QlResult<ChartSeries> {
    check_volume(price, volume)?;
    let mut result = ChartSeries::zeroed(price.len(), price.len())?;
    let mut total = 0.0;
    let mut mean: Real = 0.0;
    for (index, (&value, &weight)) in price.iter().zip(volume).enumerate() {
        if weight > 0.0 {
            let next_total = total + weight;
            require!(
                next_total.is_finite(),
                "nonfinite cumulative volume at index {index}"
            );
            if total == 0.0 {
                mean = value;
                result.first_valid = index;
            } else {
                let fraction = weight / next_total;
                let previous_fraction = total / next_total;
                let (retained, incoming) = if mean.is_sign_positive() == value.is_sign_positive() {
                    if fraction <= 0.5 {
                        (mean, (value - mean) * fraction)
                    } else {
                        (value, (mean - value) * previous_fraction)
                    }
                } else {
                    (mean * previous_fraction, value * fraction)
                };
                require!(
                    retained.is_finite() && incoming.is_finite(),
                    "nonfinite VWAP arithmetic at index {index}"
                );
                mean = retained + incoming;
                require!(mean.is_finite(), "nonfinite VWAP at index {index}");
            }
            total = next_total;
        }
        if index >= result.first_valid {
            result.values[index] = mean;
        }
    }
    Ok(result)
}

/// On-balance volume with a zero seed at the first bar.
///
/// Every bar is valid, including the initial zero. The first volume is validated
/// but does not contribute to the seed. Each later bar adds its volume when its
/// close rises, subtracts it when its close falls, and leaves OBV unchanged for
/// equal closes. A separate call starts a new zero-seeded series.
///
/// # Errors
///
/// Returns an error for mismatched lengths, nonfinite closes or volumes,
/// negative volumes, or cumulative signed-volume overflow.
///
/// Recurrence reference: [StockCharts ChartSchool](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-indicators/on-balance-volume-obv).
/// The zero seed is this API's convention, not StockCharts' arbitrary origin.
pub fn obv(close: &[Real], volume: &[Real]) -> QlResult<ChartSeries> {
    check_volume(close, volume)?;
    let mut result = ChartSeries::zeroed(close.len(), 0)?;
    let mut value = 0.0;
    for index in 1..close.len() {
        if close[index] > close[index - 1] {
            value += volume[index];
        } else if close[index] < close[index - 1] {
            value -= volume[index];
        }
        require!(value.is_finite(), "nonfinite OBV at index {index}");
        result.values[index] = value;
    }
    Ok(result)
}

#[cfg(test)]
#[path = "vwap_obv_tests.rs"]
mod tests;
