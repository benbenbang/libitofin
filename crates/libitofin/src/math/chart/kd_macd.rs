//! Taiwan KD and SMA-seeded MACD chart indicators.

use super::{ChartSeries, check_close, check_hlc, check_period, ema};
use crate::errors::QlResult;
use crate::require;
use crate::types::Real;

/// Taiwan stochastic oscillator values, aligned with the input bars.
#[derive(Clone, Debug, PartialEq)]
pub struct Kd {
    pub rsv: ChartSeries,
    pub k: ChartSeries,
    pub d: ChartSeries,
}

/// MACD line, signal, and histogram, aligned with the input closes.
#[derive(Clone, Debug, PartialEq)]
pub struct Macd {
    pub line: ChartSeries,
    pub signal: ChartSeries,
    pub histogram: ChartSeries,
}

/// Taiwan KD with rolling RSV and K/D recursive smoothing seeded at 50.
///
/// A flat high-low window has RSV 50. All three series first become valid at
/// `period - 1`; entries before then are zero placeholders.
pub fn kd(
    high: &[Real],
    low: &[Real],
    close: &[Real],
    period: usize,
    k_smooth: usize,
    d_smooth: usize,
) -> QlResult<Kd> {
    check_period(period)?;
    check_period(k_smooth)?;
    check_period(d_smooth)?;
    check_hlc(high, low, close)?;
    let len = close.len();
    let mut rsv = ChartSeries::zeroed(len, period - 1)?;
    let mut k = ChartSeries::zeroed(len, period - 1)?;
    let mut d = ChartSeries::zeroed(len, period - 1)?;
    if len < period {
        return Ok(Kd { rsv, k, d });
    }

    let mut previous_k = 50.0;
    let mut previous_d = 50.0;
    let k_weight = 1.0 / k_smooth as Real;
    let d_weight = 1.0 / d_smooth as Real;
    for index in period - 1..len {
        let start = index + 1 - period;
        let highest = high[start..=index]
            .iter()
            .copied()
            .fold(Real::NEG_INFINITY, Real::max);
        let lowest = low[start..=index]
            .iter()
            .copied()
            .fold(Real::INFINITY, Real::min);
        let range = highest - lowest;
        let current_rsv = if range == 0.0 {
            50.0
        } else if close[index] == lowest {
            0.0
        } else if close[index] == highest {
            100.0
        } else if range.is_finite() {
            100.0 * ((close[index] - lowest) / range)
        } else {
            100.0 * ((close[index] / 2.0 - lowest / 2.0) / (highest / 2.0 - lowest / 2.0))
        };
        require!(current_rsv.is_finite(), "nonfinite KD RSV at index {index}");
        previous_k = (1.0 - k_weight) * previous_k + k_weight * current_rsv;
        previous_d = (1.0 - d_weight) * previous_d + d_weight * previous_k;
        require!(
            previous_k.is_finite() && previous_d.is_finite(),
            "nonfinite KD at index {index}"
        );
        rsv.values[index] = current_rsv;
        k.values[index] = previous_k;
        d.values[index] = previous_d;
    }
    Ok(Kd { rsv, k, d })
}

/// Taiwan KD with the standard `(9, 3, 3)` periods.
pub fn kd_default(high: &[Real], low: &[Real], close: &[Real]) -> QlResult<Kd> {
    kd(high, low, close, 9, 3, 3)
}

/// MACD using SMA-seeded EMAs for both price averages and signal.
///
/// The line first becomes valid at `slow_period - 1`. The signal and histogram
/// first become valid after `signal_period` line values are available.
pub fn macd(
    close: &[Real],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> QlResult<Macd> {
    check_period(fast_period)?;
    check_period(slow_period)?;
    check_period(signal_period)?;
    require!(
        fast_period < slow_period,
        "MACD fast period must be less than slow period"
    );
    check_close(close)?;
    let len = close.len();
    let line_start = slow_period - 1;
    let signal_start = line_start.saturating_add(signal_period - 1);
    let mut line = ChartSeries::zeroed(len, line_start)?;
    let mut signal = ChartSeries::zeroed(len, signal_start)?;
    let mut histogram = ChartSeries::zeroed(len, signal_start)?;
    if len < slow_period {
        return Ok(Macd {
            line,
            signal,
            histogram,
        });
    }

    let fast = ema(close, fast_period)?;
    let slow = ema(close, slow_period)?;
    for index in line_start..len {
        let value = fast.values[index] - slow.values[index];
        require!(value.is_finite(), "nonfinite MACD line at index {index}");
        line.values[index] = value;
    }
    let signal_values = ema(&line.values[line_start..], signal_period)?;
    for (offset, value) in signal_values
        .values
        .into_iter()
        .enumerate()
        .skip(signal_period - 1)
    {
        let index = line_start + offset;
        let difference = line.values[index] - value;
        require!(
            value.is_finite() && difference.is_finite(),
            "nonfinite MACD signal or histogram at index {index}"
        );
        signal.values[index] = value;
        histogram.values[index] = difference;
    }
    Ok(Macd {
        line,
        signal,
        histogram,
    })
}

/// MACD with the standard `(12, 26, 9)` periods.
pub fn macd_default(close: &[Real]) -> QlResult<Macd> {
    macd(close, 12, 26, 9)
}

#[cfg(test)]
mod tests {
    use super::{kd, kd_default, macd, macd_default};

    #[test]
    fn taiwan_kd_seeds_from_fifty() {
        let values = kd(
            &[12.0, 14.0, 16.0, 18.0],
            &[8.0, 10.0, 12.0, 14.0],
            &[10.0, 12.0, 14.0, 16.0],
            3,
            3,
            3,
        )
        .unwrap();
        assert_eq!(values.rsv.first_valid, 2);
        assert_eq!(values.k.first_valid, 2);
        assert_eq!(values.d.first_valid, 2);
        assert_eq!(values.rsv.values, [0.0, 0.0, 75.0, 75.0]);
        assert!((values.k.values[2] - 58.333333333333336).abs() < 1e-12);
        assert!((values.d.values[2] - 52.77777777777778).abs() < 1e-12);
        assert!((values.k.values[3] - 63.88888888888889).abs() < 1e-12);
        assert!((values.d.values[3] - 56.48148148148148).abs() < 1e-12);
        assert_eq!(values.k.get(1), None);
    }

    #[test]
    fn flat_and_extreme_kd_windows_stay_finite() {
        let flat = kd(&[2.0; 3], &[2.0; 3], &[2.0; 3], 2, 3, 3).unwrap();
        assert_eq!(flat.rsv.values, [0.0, 50.0, 50.0]);
        assert_eq!(flat.k.values[1], 50.0);
        assert_eq!(flat.d.values[2], 50.0);
        let extreme = kd(&[1e308], &[-1e308], &[0.0], 1, 3, 3).unwrap();
        assert_eq!(extreme.rsv.values, [50.0]);
        assert!(extreme.k.values[0].is_finite());
    }

    #[test]
    fn macd_uses_valid_line_values_to_seed_signal() {
        let values = macd(&[1.0, 2.0, 3.0, 4.0], 2, 3, 2).unwrap();
        assert_eq!(values.line.first_valid, 2);
        assert_eq!(values.signal.first_valid, 3);
        assert_eq!(values.histogram.first_valid, 3);
        assert_eq!(values.line.values, [0.0, 0.0, 0.5, 0.5]);
        assert_eq!(values.signal.values, [0.0, 0.0, 0.0, 0.5]);
        assert_eq!(values.histogram.values, [0.0; 4]);
        assert_eq!(values.signal.get(2), None);
    }

    #[test]
    fn invalid_and_short_inputs_are_explicit() {
        assert!(kd(&[1.0], &[0.0], &[0.5], 0, 3, 3).is_err());
        assert!(kd(&[1.0], &[0.0], &[2.0], 1, 3, 3).is_err());
        assert!(kd(&[1.0], &[0.0], &[f64::NAN], 1, 3, 3).is_err());
        assert_eq!(kd_default(&[], &[], &[]).unwrap().rsv.first_valid, 0);
        assert!(macd(&[1.0], 2, 2, 1).is_err());
        assert!(macd(&[f64::INFINITY], 1, 2, 1).is_err());
        assert_eq!(macd_default(&[]).unwrap().line.first_valid, 0);
        let closes: Vec<f64> = (1..=34).map(f64::from).collect();
        let default = macd_default(&closes).unwrap();
        assert_eq!(default.line.first_valid, 25);
        assert_eq!(default.signal.first_valid, 33);
        assert_eq!(default.histogram.first_valid, 33);
        let short = macd(&[1.0, 2.0], 2, 3, 2).unwrap();
        assert_eq!(short.line.first_valid, 2);
        assert_eq!(short.signal.first_valid, 2);
        assert_eq!(short.line.values, [0.0; 2]);
    }
}
