//! Averages and volume bars.

use super::{ChartSeries, check_close, check_hlc, check_period};
use crate::errors::{QlError, QlResult};
use crate::require;
use crate::types::Real;

/// Volume values and direction relative to each bar's open.
#[derive(Clone, Debug, PartialEq)]
pub struct VolumeBars {
    pub volume: ChartSeries,
    pub direction: Vec<i8>,
}

/// Simple moving average, first valid at `period - 1`.
pub fn sma(close: &[Real], period: usize) -> QlResult<ChartSeries> {
    check_period(period)?;
    check_close(close)?;
    let mut result = ChartSeries::zeroed(close.len(), period - 1)?;
    if close.len() < period {
        return Ok(result);
    }
    let denominator = period as Real;
    let mut mean = 0.0;
    for (index, value) in close.iter().enumerate() {
        if index >= period {
            mean -= close[index - period] / denominator;
        }
        mean += value / denominator;
        if index + 1 >= period {
            require!(mean.is_finite(), "nonfinite SMA at index {index}");
            result.values[index] = mean;
        }
    }
    Ok(result)
}

/// Exponential moving average seeded by the first `period`-bar SMA.
pub fn ema(close: &[Real], period: usize) -> QlResult<ChartSeries> {
    check_period(period)?;
    check_close(close)?;
    let mut result = ChartSeries::zeroed(close.len(), period - 1)?;
    if close.len() < period {
        return Ok(result);
    }
    let seed = sma(&close[..period], period)?.values[period - 1];
    result.values[period - 1] = seed;
    let alpha = 2.0 / (period as Real + 1.0);
    let mut previous = seed;
    for (index, value) in close.iter().enumerate().skip(period) {
        previous = (1.0 - alpha) * previous + alpha * value;
        require!(previous.is_finite(), "nonfinite EMA at index {index}");
        result.values[index] = previous;
    }
    Ok(result)
}

/// Copies nonnegative volume and classifies each bar as down, flat, or up.
pub fn volume_bars(
    open: &[Real],
    high: &[Real],
    low: &[Real],
    close: &[Real],
    volume: &[Real],
) -> QlResult<VolumeBars> {
    let len = close.len();
    require!(
        open.len() == len && high.len() == len && low.len() == len && volume.len() == len,
        "chart OHLCV lengths differ"
    );
    check_hlc(high, low, close)?;
    let mut values = ChartSeries::zeroed(len, 0)?;
    let mut direction = Vec::new();
    direction
        .try_reserve_exact(len)
        .map_err(|_| QlError::new("chart output allocation failed", file!(), line!()))?;
    for index in 0..len {
        require!(
            open[index].is_finite() && volume[index].is_finite(),
            "nonfinite OHLCV at index {index}"
        );
        require!(
            low[index] <= open[index] && open[index] <= high[index],
            "invalid OHLC at index {index}"
        );
        require!(
            volume[index].partial_cmp(&0.0) != Some(std::cmp::Ordering::Less),
            "negative volume at index {index}"
        );
        values.values[index] = volume[index];
        direction.push(if close[index] > open[index] {
            1
        } else if close[index] < open[index] {
            -1
        } else {
            0
        });
    }
    Ok(VolumeBars {
        volume: values,
        direction,
    })
}

#[cfg(test)]
mod tests {
    use super::{ema, sma, volume_bars};

    #[test]
    fn averages_seed_and_align() {
        let values = [1.0, 2.0, 3.0, 4.0];
        let simple = sma(&values, 3).unwrap();
        let exponential = ema(&values, 3).unwrap();
        assert_eq!(simple.first_valid, 2);
        assert_eq!(simple.values, [0.0, 0.0, 2.0, 3.0]);
        assert_eq!(exponential.values, [0.0, 0.0, 2.0, 3.0]);
        assert_eq!(simple.get(1), None);
        assert_eq!(simple.get(2), Some(2.0));
    }

    #[test]
    fn short_and_invalid_inputs() {
        assert_eq!(sma(&[], 2).unwrap().first_valid, 0);
        assert_eq!(ema(&[1.0], 2).unwrap().first_valid, 1);
        assert!(sma(&[1.0], 0).is_err());
        assert!(ema(&[f64::NAN], 2).is_err());
    }

    #[test]
    fn finite_extremes_do_not_overflow_intermediate_results() {
        assert_eq!(ema(&[-1e308, 1e308], 1).unwrap().values, [-1e308, 1e308]);
        assert_eq!(sma(&[1e308, 1e308], 2).unwrap().values[1], 1e308);
        assert!(
            sma(&[1e308, 0.0, 1e308], 2)
                .unwrap()
                .values
                .iter()
                .all(|value| value.is_finite())
        );
    }

    #[test]
    fn volume_preserves_input_and_direction() {
        let bars = volume_bars(
            &[1.0, 2.0, -2.0],
            &[3.0, 3.0, 0.0],
            &[0.0, 0.0, -3.0],
            &[2.0, 1.0, -2.0],
            &[10.0, 11.0, 0.0],
        )
        .unwrap();
        assert_eq!(bars.volume.values, [10.0, 11.0, 0.0]);
        assert_eq!(bars.direction, [1, -1, 0]);
        assert!(volume_bars(&[0.0], &[1.0], &[-1.0], &[0.0], &[-1.0]).is_err());
    }
}
