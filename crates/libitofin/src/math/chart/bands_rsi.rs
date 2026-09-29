//! Bollinger Bands and Wilder RSI.

use super::{ChartSeries, check_close, check_period};
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// Population-standard-deviation bands aligned with the input closes.
#[derive(Clone, Debug, PartialEq)]
pub struct BollingerBands {
    pub middle: ChartSeries,
    pub upper: ChartSeries,
    pub lower: ChartSeries,
}

/// Rolling Bollinger bands with a nonnegative standard-deviation multiplier.
/// All three series first become valid at `period - 1`.
/// Each window is recalculated in `O(period)` time.
pub fn bollinger_bands(
    close: &[Real],
    period: usize,
    multiplier: Real,
) -> QlResult<BollingerBands> {
    check_period(period)?;
    check_close(close)?;
    require!(
        multiplier.is_finite() && multiplier >= 0.0,
        "Bollinger multiplier must be finite and nonnegative"
    );
    let first_valid = period - 1;
    let mut middle = ChartSeries::zeroed(close.len(), first_valid)?;
    let mut upper = ChartSeries::zeroed(close.len(), first_valid)?;
    let mut lower = ChartSeries::zeroed(close.len(), first_valid)?;

    if close.len() >= period {
        let denominator = period as Real;
        for index in first_valid..close.len() {
            let window = &close[index + 1 - period..=index];
            let scale = window
                .iter()
                .fold(0.0_f64, |max, value| max.max(value.abs()));
            if scale == 0.0 {
                continue;
            }
            let anchor = window[0] / scale;
            let mean_delta = window
                .iter()
                .map(|value| (value / scale - anchor) / denominator)
                .sum::<Real>();
            let mean_scaled = anchor + mean_delta;
            let variance_scaled = window
                .iter()
                .map(|value| {
                    let deviation = (value / scale - anchor) - mean_delta;
                    (deviation * deviation) / denominator
                })
                .sum::<Real>();
            let center = scale * mean_scaled;
            let deviation = scale * variance_scaled.sqrt();
            let offset = multiplier * deviation;
            let high = center + offset;
            let low = center - offset;
            require!(
                center.is_finite() && high.is_finite() && low.is_finite(),
                "nonfinite Bollinger band at index {index}"
            );
            middle.values[index] = center;
            upper.values[index] = high;
            lower.values[index] = low;
        }
    }

    Ok(BollingerBands {
        middle,
        upper,
        lower,
    })
}

/// Wilder's relative strength index, seeded by `period` price changes.
/// The first valid output is at index `period`; a flat window has RSI 50.
pub fn rsi(close: &[Real], period: usize) -> QlResult<ChartSeries> {
    check_period(period)?;
    check_close(close)?;
    let mut result = ChartSeries::zeroed(close.len(), period)?;
    if close.len() <= period {
        return Ok(result);
    }

    let change = |index: usize| PriceChange::between(close[index], close[index - 1]);
    let mut change_scale = PriceChange::zero();
    for index in 1..=period {
        let magnitude = change(index).absolute();
        if magnitude.is_larger_than(change_scale) {
            change_scale = magnitude;
        }
    }
    let denominator = period as Real;
    let mut average_gain = 0.0;
    let mut average_loss = 0.0;
    for index in 1..=period {
        let delta = change(index).divided_by(change_scale);
        average_gain += delta.max(0.0) / denominator;
        average_loss += (-delta).max(0.0) / denominator;
    }
    result.values[period] = rsi_value(average_gain, average_loss);

    let retained = 1.0 - 1.0 / denominator;
    let added = 1.0 / denominator;
    for (index, value) in result.values.iter_mut().enumerate().skip(period + 1) {
        let raw_change = change(index);
        let magnitude = raw_change.absolute();
        if magnitude.is_larger_than(change_scale) {
            let ratio = change_scale.divided_by(magnitude);
            average_gain *= ratio;
            average_loss *= ratio;
            change_scale = magnitude;
        }
        let delta = raw_change.divided_by(change_scale);
        average_gain = retained * average_gain + added * delta.max(0.0);
        average_loss = retained * average_loss + added * (-delta).max(0.0);
        *value = rsi_value(average_gain, average_loss);
    }
    Ok(result)
}

#[derive(Clone, Copy)]
struct PriceChange {
    value: Real,
    halved: bool,
}

impl PriceChange {
    fn zero() -> Self {
        Self {
            value: 0.0,
            halved: false,
        }
    }

    fn between(current: Real, previous: Real) -> Self {
        let direct = current - previous;
        if direct.is_finite() {
            Self {
                value: direct,
                halved: false,
            }
        } else {
            Self {
                value: current * 0.5 - previous * 0.5,
                halved: true,
            }
        }
    }

    fn absolute(self) -> Self {
        Self {
            value: self.value.abs(),
            halved: self.halved,
        }
    }

    fn is_larger_than(self, other: Self) -> bool {
        if self.halved != other.halved {
            self.halved
        } else {
            self.value.abs() > other.value.abs()
        }
    }

    fn divided_by(self, scale: Self) -> Real {
        if scale.value == 0.0 {
            0.0
        } else if scale.halved && !self.halved {
            (self.value * 0.5) / scale.value
        } else {
            self.value / scale.value
        }
    }
}

fn rsi_value(gain: Real, loss: Real) -> Real {
    if gain == 0.0 && loss == 0.0 {
        50.0
    } else if loss == 0.0 {
        100.0
    } else if gain == 0.0 {
        0.0
    } else if gain >= loss {
        100.0 / (1.0 + loss / gain)
    } else {
        let ratio = gain / loss;
        100.0 * ratio / (1.0 + ratio)
    }
}

#[cfg(test)]
mod tests {
    use super::{bollinger_bands, rsi};

    #[test]
    fn bollinger_population_window_and_warmup() {
        let bands = bollinger_bands(&[1.0, 2.0, 3.0, 4.0], 3, 2.0).unwrap();
        assert_eq!(bands.middle.values, [0.0, 0.0, 2.0, 3.0]);
        assert_eq!(bands.middle.first_valid, 2);
        for index in 2..4 {
            let expected_offset = 2.0 * (2.0_f64 / 3.0).sqrt();
            assert!(
                (bands.upper.values[index] - (bands.middle.values[index] + expected_offset)).abs()
                    < 1e-12
            );
            assert!(
                (bands.lower.values[index] - (bands.middle.values[index] - expected_offset)).abs()
                    < 1e-12
            );
        }
        assert_eq!(bands.upper.get(1), None);
    }

    #[test]
    fn bollinger_extremes_and_bad_inputs() {
        let extremes = bollinger_bands(&[-f64::MAX, f64::MAX], 2, 1.0).unwrap();
        assert_eq!(extremes.middle.values[1], 0.0);
        assert_eq!(extremes.upper.values[1], f64::MAX);
        assert_eq!(extremes.lower.values[1], -f64::MAX);
        let constant = bollinger_bands(&vec![f64::MAX; 49], 49, 2.0).unwrap();
        assert_eq!(constant.middle.values[48], f64::MAX);
        assert_eq!(constant.upper.values[48], f64::MAX);
        assert_eq!(constant.lower.values[48], f64::MAX);
        assert!(bollinger_bands(&[1.0], 0, 2.0).is_err());
        assert!(bollinger_bands(&[1.0], 1, -1.0).is_err());
        assert!(bollinger_bands(&[f64::NAN], 2, 2.0).is_err());
        assert!(bollinger_bands(&[f64::MAX, -f64::MAX], 2, 2.0).is_err());
        assert_eq!(bollinger_bands(&[], 3, 2.0).unwrap().middle.first_valid, 0);
    }

    #[test]
    fn rsi_wilder_seed_smoothing_and_flat_price() {
        let result = rsi(&[1.0, 2.0, 3.0, 2.0, 2.0], 2).unwrap();
        assert_eq!(result.first_valid, 2);
        assert_eq!(result.values, [0.0, 0.0, 100.0, 50.0, 50.0]);
        assert_eq!(rsi(&[4.0, 4.0, 4.0], 2).unwrap().values[2], 50.0);
        assert_eq!(rsi(&[3.0, 2.0, 1.0], 2).unwrap().values[2], 0.0);
        assert_eq!(rsi(&[1.0, 2.0], 1).unwrap().values[1], 100.0);
    }

    #[test]
    fn rsi_extremes_short_and_bad_inputs() {
        assert_eq!(
            rsi(&[-f64::MAX, f64::MAX, -f64::MAX], 2).unwrap().values[2],
            50.0
        );
        let mixed = rsi(&[0.0, f64::MAX, -f64::MAX], 2).unwrap();
        assert!((mixed.values[2] - 100.0 / 3.0).abs() < 1e-12);
        assert_eq!(rsi(&[1.0], 2).unwrap().first_valid, 1);
        assert_eq!(rsi(&[], 2).unwrap().first_valid, 0);
        let tiny = f64::from_bits(1);
        assert_eq!(rsi(&[0.0, tiny, tiny * 2.0], 2).unwrap().values[2], 100.0);
        assert_eq!(
            rsi(&[0.0, tiny, tiny * 2.0, f64::MAX], 2).unwrap().values[2],
            100.0
        );
        assert!(rsi(&[], 0).is_err());
        assert!(rsi(&[f64::INFINITY], 2).is_err());
    }
}
