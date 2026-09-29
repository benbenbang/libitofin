//! Close-price and OHLC volatility estimators aligned with chart input bars.

use crate::errors::QlResult;
use crate::math::chart::ChartSeries;
use crate::prices::IntervalPrice;
use crate::require;
use crate::types::Real;

/// Annualized point estimates aligned with every input OHLC bar.
#[derive(Clone, Debug, PartialEq)]
pub struct OhlcPointEstimates {
    pub simple_sigma: ChartSeries,
    pub parkinson_sigma: ChartSeries,
    pub garman_klass_sigma4: ChartSeries,
    pub garman_klass_sigma5: ChartSeries,
}

/// Annualized overnight-aware estimates aligned with every input OHLC bar.
#[derive(Clone, Debug, PartialEq)]
pub struct OhlcOvernightEstimates {
    pub garman_klass_sigma1: ChartSeries,
    pub garman_klass_sigma3: ChartSeries,
    pub garman_klass_sigma6: ChartSeries,
}

/// Estimates three QuantLib overnight volatilities from consecutive OHLC bars.
/// `year_fractions[i]` belongs to the interval ending at `i`; index zero is unused.
/// `overnight_fraction` is the fraction of each interval while the market is closed.
///
/// # Errors
/// Returns an error for mismatched lengths, invalid OHLC prices or used fractions,
/// or a negative or nonfinite annualized variance.
pub fn ohlc_overnight_volatility(
    prices: &[IntervalPrice],
    year_fractions: &[Real],
    overnight_fraction: Real,
) -> QlResult<OhlcOvernightEstimates> {
    require!(
        prices.len() == year_fractions.len(),
        "OHLC and year-fraction lengths differ"
    );
    for (index, fraction) in year_fractions.iter().enumerate().skip(1) {
        require!(
            fraction.is_finite() && *fraction > 0.0,
            "invalid year fraction at index {index}"
        );
    }
    ohlc_overnight(prices, overnight_fraction, |index| year_fractions[index])
}

/// Estimates three QuantLib overnight volatilities with a common year fraction.
/// The first bar is a zero placeholder because it has no preceding close.
///
/// # Errors
/// Returns an error for invalid OHLC prices or fractions, or a negative or
/// nonfinite annualized variance.
pub fn ohlc_overnight_volatility_constant_fraction(
    prices: &[IntervalPrice],
    year_fraction: Real,
    overnight_fraction: Real,
) -> QlResult<OhlcOvernightEstimates> {
    require!(
        year_fraction.is_finite() && year_fraction > 0.0,
        "year fraction must be positive and finite"
    );
    ohlc_overnight(prices, overnight_fraction, |_| year_fraction)
}

fn ohlc_overnight(
    prices: &[IntervalPrice],
    overnight_fraction: Real,
    year_fraction: impl Fn(usize) -> Real,
) -> QlResult<OhlcOvernightEstimates> {
    require!(
        overnight_fraction.is_finite() && overnight_fraction > 0.0 && overnight_fraction < 1.0,
        "overnight fraction must be finite and between zero and one"
    );
    let len = prices.len();
    let mut result = OhlcOvernightEstimates {
        garman_klass_sigma1: ChartSeries::zeroed(len, 1)?,
        garman_klass_sigma3: ChartSeries::zeroed(len, 1)?,
        garman_klass_sigma6: ChartSeries::zeroed(len, 1)?,
    };
    for (index, price) in prices.iter().copied().enumerate() {
        let [simple, parkinson, sigma4, _] = ohlc_point_terms(price, index)?;
        if index == 0 {
            continue;
        }
        let gap = log_ratio(price.open(), prices[index - 1].close());
        for (weight, point, series) in [
            (0.5, simple, &mut result.garman_klass_sigma1),
            (0.17, parkinson, &mut result.garman_klass_sigma3),
            (0.012, sigma4, &mut result.garman_klass_sigma6),
        ] {
            series.values[index] = overnight_volatility(
                gap,
                point,
                weight,
                overnight_fraction,
                year_fraction(index),
                index,
            )?;
        }
    }
    Ok(result)
}

fn overnight_volatility(
    gap: Real,
    point: Real,
    weight: Real,
    overnight_fraction: Real,
    year_fraction: Real,
    index: usize,
) -> QlResult<Real> {
    let variance = weight * gap * gap / overnight_fraction
        + (1.0 - weight) * point / (1.0 - overnight_fraction);
    let root = if variance.is_finite() {
        require!(
            variance.is_sign_positive() || variance == 0.0,
            "negative overnight variance at index {index}"
        );
        variance.sqrt()
    } else {
        let overnight = gap.abs() * weight.sqrt() / overnight_fraction.sqrt();
        let intraday =
            point.abs().sqrt() * (1.0 - weight).sqrt() / (1.0 - overnight_fraction).sqrt();
        if point < 0.0 {
            require!(
                overnight.total_cmp(&intraday).is_ge(),
                "negative overnight variance at index {index}"
            );
            overnight * (1.0 - (intraday / overnight).powi(2)).sqrt()
        } else {
            overnight.hypot(intraday)
        }
    };
    let volatility = root / year_fraction.sqrt();
    require!(
        volatility.is_finite(),
        "nonfinite overnight volatility at index {index}"
    );
    Ok(volatility)
}

/// Estimates four QuantLib point volatilities using one fraction per OHLC bar.
/// All bars, including the first, have a valid estimate.
///
/// # Errors
/// Returns an error for mismatched lengths, nonpositive or nonfinite OHLC
/// prices or year fractions, or a nonfinite annualized estimate.
pub fn ohlc_point_volatility(
    prices: &[IntervalPrice],
    year_fractions: &[Real],
) -> QlResult<OhlcPointEstimates> {
    require!(
        prices.len() == year_fractions.len(),
        "OHLC and year-fraction lengths differ"
    );
    for (index, fraction) in year_fractions.iter().enumerate() {
        require!(
            fraction.is_finite() && *fraction > 0.0,
            "invalid year fraction at index {index}"
        );
    }
    ohlc_points(prices, |index| year_fractions[index])
}

/// Estimates four QuantLib point volatilities with a common year fraction.
/// All bars, including the first, have a valid estimate.
///
/// # Errors
/// Returns an error for a nonpositive or nonfinite fraction, nonpositive or
/// nonfinite OHLC prices, or a nonfinite annualized estimate.
pub fn ohlc_point_volatility_constant_fraction(
    prices: &[IntervalPrice],
    year_fraction: Real,
) -> QlResult<OhlcPointEstimates> {
    require!(
        year_fraction.is_finite() && year_fraction > 0.0,
        "year fraction must be positive and finite"
    );
    ohlc_points(prices, |_| year_fraction)
}

fn ohlc_points(
    prices: &[IntervalPrice],
    year_fraction: impl Fn(usize) -> Real,
) -> QlResult<OhlcPointEstimates> {
    let len = prices.len();
    let mut result = OhlcPointEstimates {
        simple_sigma: ChartSeries::zeroed(len, 0)?,
        parkinson_sigma: ChartSeries::zeroed(len, 0)?,
        garman_klass_sigma4: ChartSeries::zeroed(len, 0)?,
        garman_klass_sigma5: ChartSeries::zeroed(len, 0)?,
    };
    for (index, price) in prices.iter().copied().enumerate() {
        let [simple, parkinson, sigma4, sigma5] = ohlc_point_terms(price, index)?;
        let divisor = year_fraction(index).sqrt();
        for (point, series) in [
            (simple, &mut result.simple_sigma),
            (parkinson, &mut result.parkinson_sigma),
            (sigma4, &mut result.garman_klass_sigma4),
            (sigma5, &mut result.garman_klass_sigma5),
        ] {
            let volatility = point.abs().sqrt() / divisor;
            require!(
                volatility.is_finite(),
                "nonfinite OHLC point volatility at index {index}"
            );
            series.values[index] = volatility;
        }
    }
    Ok(result)
}

fn ohlc_point_terms(price: IntervalPrice, index: usize) -> QlResult<[Real; 4]> {
    let (open, high, low, close) = (price.open(), price.high(), price.low(), price.close());
    require!(
        [open, high, low, close]
            .iter()
            .all(|value| value.is_finite() && *value > 0.0),
        "invalid OHLC price at index {index}"
    );
    let u = log_ratio(high, open);
    let d = log_ratio(low, open);
    let c = log_ratio(close, open);
    let range = u - d;
    Ok([
        c * c,
        range * range / (4.0 * std::f64::consts::LN_2),
        0.511 * range * range - 0.019 * (c * (u + d) - 2.0 * u * d) - 0.383 * c * c,
        0.5 * range * range - (2.0 * std::f64::consts::LN_2 - 1.0) * c * c,
    ])
}

fn log_ratio(numerator: Real, denominator: Real) -> Real {
    let ratio = numerator / denominator;
    if ratio.is_finite() && ratio > 0.0 {
        ratio.ln()
    } else {
        numerator.ln() - denominator.ln()
    }
}

/// Estimates annualized local volatility from consecutive positive closes.
/// `year_fractions[i]` belongs to the interval ending at `i`; index zero is unused.
///
/// # Errors
/// Returns an error for mismatched lengths, invalid closes or used year fractions.
pub fn simple_local_volatility(close: &[Real], year_fractions: &[Real]) -> QlResult<ChartSeries> {
    require!(
        close.len() == year_fractions.len(),
        "close and year-fraction lengths differ"
    );
    for (index, year_fraction) in year_fractions.iter().enumerate().skip(1) {
        require!(
            year_fraction.is_finite() && *year_fraction > 0.0,
            "invalid year fraction at index {index}"
        );
    }
    local_volatility(close, |index| year_fractions[index])
}

/// Estimates annualized local volatility with one fraction for every interval.
///
/// # Errors
/// Returns an error for invalid closes or a nonpositive or nonfinite fraction.
pub fn simple_local_volatility_constant_fraction(
    close: &[Real],
    year_fraction: Real,
) -> QlResult<ChartSeries> {
    require!(
        year_fraction.is_finite() && year_fraction > 0.0,
        "year fraction must be positive and finite"
    );
    local_volatility(close, |_| year_fraction)
}

fn local_volatility(
    close: &[Real],
    year_fraction: impl Fn(usize) -> Real,
) -> QlResult<ChartSeries> {
    for (index, value) in close.iter().enumerate() {
        require!(
            value.is_finite() && *value > 0.0,
            "invalid close at index {index}"
        );
    }
    let mut result = ChartSeries::zeroed(close.len(), 1)?;
    for index in 1..close.len() {
        let ratio = close[index] / close[index - 1];
        let log_return = if ratio.is_finite() && ratio > 0.0 {
            ratio.ln()
        } else {
            close[index].ln() - close[index - 1].ln()
        };
        let volatility = log_return.abs() / year_fraction(index).sqrt();
        require!(
            volatility.is_finite(),
            "nonfinite local volatility at index {index}"
        );
        result.values[index] = volatility;
    }
    Ok(result)
}

/// Applies QuantLib's constant estimator to each preceding window of valid volatility.
/// The current input value is excluded from its estimate.
///
/// # Errors
/// Returns an error for a zero window, invalid valid-range metadata, or a
/// nonfinite value in the valid input suffix.
pub fn constant_volatility(input: &ChartSeries, window: usize) -> QlResult<ChartSeries> {
    require!(window > 0, "volatility window must be positive");
    let len = input.values.len();
    require!(input.first_valid <= len, "invalid first-valid index");
    for (index, value) in input.values.iter().enumerate().skip(input.first_valid) {
        require!(value.is_finite(), "invalid volatility at index {index}");
    }
    let first_valid = input.first_valid.saturating_add(window).min(len);
    let mut result = ChartSeries::zeroed(len, first_valid)?;
    let count = window as Real;
    for index in first_valid..len {
        let preceding = &input.values[index - window..index];
        let scale = preceding
            .iter()
            .fold(0.0_f64, |max, value| max.max(value.abs()));
        if scale == 0.0 {
            continue;
        }
        let mut mean = 0.0;
        let mut squared_deviations = 0.0;
        for (offset, value) in preceding.iter().enumerate() {
            let normalized = value / scale;
            let delta = normalized - mean;
            mean += delta / (offset + 1) as Real;
            squared_deviations += delta * (normalized - mean);
        }
        let variance = (squared_deviations / count).max(0.0);
        let normalized = (variance + mean * mean / (count + 1.0)).sqrt();
        let volatility = scale * normalized;
        require!(
            volatility.is_finite(),
            "nonfinite constant volatility at index {index}"
        );
        result.values[index] = volatility;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(open: Real, high: Real, low: Real, close: Real) -> IntervalPrice {
        IntervalPrice::new(open, high, low, close).unwrap()
    }

    #[test]
    fn ohlc_quantlib_fixture() {
        let result =
            ohlc_point_volatility_constant_fraction(&[bar(100.0, 110.0, 90.0, 105.0)], 1.0 / 252.0)
                .unwrap();
        for (series, expected) in [
            (&result.simple_sigma, 0.7745198449099887),
            (&result.parkinson_sigma, 1.9131168640323526),
            (&result.garman_klass_sigma4, 2.204975405342331),
            (&result.garman_klass_sigma5, 2.2004838300550182),
        ] {
            assert_eq!(series.first_valid, 0);
            assert_eq!(series.values.len(), 1);
            assert!((series.values[0] - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn ohlc_scalar_indexed_parity_and_input_immutability() {
        let prices = [
            bar(100.0, 110.0, 90.0, 105.0),
            bar(105.0, 106.0, 95.0, 96.0),
        ];
        let original = prices;
        let fractions = [1.0 / 252.0; 2];
        let indexed = ohlc_point_volatility(&prices, &fractions).unwrap();
        let scalar = ohlc_point_volatility_constant_fraction(&prices, 1.0 / 252.0).unwrap();
        assert_eq!(indexed, scalar);
        assert_eq!(prices, original);
        assert_eq!(fractions, [1.0 / 252.0; 2]);
        assert!(indexed.garman_klass_sigma4.values[1] > 0.0);
    }

    #[test]
    fn ohlc_validates_all_bars_and_fractions() {
        let valid = bar(1.0, 2.0, 0.5, 1.5);
        assert!(ohlc_point_volatility(&[valid], &[]).is_err());
        assert!(ohlc_point_volatility(&[], &[1.0]).is_err());
        for invalid in [0.0, -1.0, Real::NAN, Real::INFINITY] {
            assert!(ohlc_point_volatility(&[valid], &[invalid]).is_err());
            assert!(ohlc_point_volatility_constant_fraction(&[], invalid).is_err());
        }
        for invalid in [
            bar(0.0, 1.0, 0.0, 0.5),
            bar(-1.0, -0.5, -2.0, -1.5),
            bar(1.0, 1.0, -1.0, 0.5),
        ] {
            assert!(ohlc_point_volatility(&[valid, invalid], &[1.0, 1.0]).is_err());
            assert!(ohlc_point_volatility_constant_fraction(&[invalid], 1.0).is_err());
        }
    }

    #[test]
    fn ohlc_empty_and_short_series_are_valid_from_zero() {
        let empty = ohlc_point_volatility(&[], &[]).unwrap();
        for series in [
            &empty.simple_sigma,
            &empty.parkinson_sigma,
            &empty.garman_klass_sigma4,
            &empty.garman_klass_sigma5,
        ] {
            assert_eq!(series.first_valid, 0);
            assert!(series.values.is_empty());
        }
        assert_eq!(
            empty,
            ohlc_point_volatility_constant_fraction(&[], 1.0).unwrap()
        );
        let flat =
            ohlc_point_volatility_constant_fraction(&[bar(1.0, 1.0, 1.0, 1.0)], 1.0).unwrap();
        for series in [
            flat.simple_sigma,
            flat.parkinson_sigma,
            flat.garman_klass_sigma4,
            flat.garman_klass_sigma5,
        ] {
            assert_eq!(series.first_valid, 0);
            assert_eq!(series.values, [0.0]);
        }
    }

    #[test]
    fn ohlc_extreme_finite_ratios_and_fractions() {
        let extreme = bar(Real::MIN_POSITIVE, Real::MAX, Real::MIN_POSITIVE, Real::MAX);
        let result = ohlc_point_volatility_constant_fraction(&[extreme], 1.0).unwrap();
        for series in [
            result.simple_sigma,
            result.parkinson_sigma,
            result.garman_klass_sigma4,
            result.garman_klass_sigma5,
        ] {
            assert!(series.values[0].is_finite());
            assert!(series.values[0] > 0.0);
        }
        let tiny_fraction =
            ohlc_point_volatility_constant_fraction(&[extreme], Real::MIN_POSITIVE).unwrap();
        assert!(tiny_fraction.simple_sigma.values[0].is_finite());
    }

    #[test]
    fn overnight_quantlib_fixture() {
        let prices = [
            bar(100.0, 100.0, 100.0, 100.0),
            bar(110.0, 120.0, 105.0, 115.0),
        ];
        let result = ohlc_overnight_volatility(&prices, &[Real::NAN, 1.0 / 252.0], 0.25).unwrap();
        for (series, expected) in [
            (&result.garman_klass_sigma1, 2.2159224836472786),
            (&result.garman_klass_sigma3, 1.8303355611439194),
            (&result.garman_klass_sigma6, 1.6795672145926133),
        ] {
            assert_eq!(series.first_valid, 1);
            assert_eq!(series.values.len(), 2);
            assert_eq!(series.values[0], 0.0);
            assert!((series.values[1] - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn overnight_scalar_indexed_parity_and_immutability() {
        let prices = [
            bar(100.0, 100.0, 100.0, 100.0),
            bar(110.0, 120.0, 105.0, 115.0),
            bar(112.0, 118.0, 108.0, 110.0),
        ];
        let original = prices;
        let fractions = [0.0, 1.0 / 252.0, 1.0 / 252.0];
        let indexed = ohlc_overnight_volatility(&prices, &fractions, 0.25).unwrap();
        let scalar =
            ohlc_overnight_volatility_constant_fraction(&prices, 1.0 / 252.0, 0.25).unwrap();
        assert_eq!(indexed, scalar);
        assert_eq!(prices, original);
        assert_eq!(fractions, [0.0, 1.0 / 252.0, 1.0 / 252.0]);
        assert!(indexed.garman_klass_sigma1.values[2] > 0.0);
        let differing =
            ohlc_overnight_volatility(&prices, &[0.0, 1.0 / 252.0, 1.0 / 365.0], 0.25).unwrap();
        assert!(differing.garman_klass_sigma3.values[2] > indexed.garman_klass_sigma3.values[2]);
    }

    #[test]
    fn overnight_validates_prices_and_fractions() {
        let valid = bar(1.0, 2.0, 0.5, 1.5);
        assert!(ohlc_overnight_volatility(&[valid], &[], 0.25).is_err());
        for invalid in [0.0, 1.0, -0.5, Real::NAN, Real::INFINITY] {
            assert!(ohlc_overnight_volatility(&[], &[], invalid).is_err());
            assert!(ohlc_overnight_volatility_constant_fraction(&[], 1.0, invalid).is_err());
        }
        for invalid in [0.0, -1.0, Real::NAN, Real::INFINITY] {
            assert!(ohlc_overnight_volatility(&[valid, valid], &[0.0, invalid], 0.25).is_err());
            assert!(ohlc_overnight_volatility_constant_fraction(&[], invalid, 0.25).is_err());
        }
        for invalid in [bar(0.0, 1.0, 0.0, 0.5), bar(-1.0, -0.5, -2.0, -1.5)] {
            assert!(ohlc_overnight_volatility(&[invalid], &[Real::NAN], 0.25).is_err());
            assert!(ohlc_overnight_volatility(&[valid, invalid], &[0.0, 1.0], 0.25).is_err());
        }
        assert!(overnight_volatility(0.0, -1.0, 0.5, 0.25, 1.0, 1).is_err());
    }

    #[test]
    fn overnight_short_series_have_zero_warmup() {
        let empty = ohlc_overnight_volatility(&[], &[], 0.25).unwrap();
        let single =
            ohlc_overnight_volatility(&[bar(1.0, 1.0, 1.0, 1.0)], &[Real::NAN], 0.25).unwrap();
        for series in [
            &empty.garman_klass_sigma1,
            &empty.garman_klass_sigma3,
            &empty.garman_klass_sigma6,
        ] {
            assert_eq!(series.first_valid, 0);
            assert!(series.values.is_empty());
        }
        for series in [
            &single.garman_klass_sigma1,
            &single.garman_klass_sigma3,
            &single.garman_klass_sigma6,
        ] {
            assert_eq!(series.first_valid, 1);
            assert_eq!(series.values, [0.0]);
        }
    }

    #[test]
    fn overnight_extreme_finite_inputs_avoid_intermediate_overflow() {
        let prices = [
            bar(
                Real::MIN_POSITIVE,
                Real::MIN_POSITIVE,
                Real::MIN_POSITIVE,
                Real::MIN_POSITIVE,
            ),
            bar(Real::MAX, Real::MAX, Real::MIN_POSITIVE, Real::MAX),
        ];
        let result =
            ohlc_overnight_volatility_constant_fraction(&prices, 1.0, Real::MIN_POSITIVE).unwrap();
        for series in [
            result.garman_klass_sigma1,
            result.garman_klass_sigma3,
            result.garman_klass_sigma6,
        ] {
            assert!(series.values[1].is_finite());
            assert!(series.values[1] > 1e150);
        }
        assert!(
            ohlc_overnight_volatility_constant_fraction(
                &prices,
                Real::MIN_POSITIVE,
                Real::MIN_POSITIVE
            )
            .is_err()
        );
    }

    fn close_fixture() -> [Real; 3] {
        [100.0, 110.0, 99.0]
    }

    #[test]
    fn quantlib_fixture_and_composition() {
        let close = close_fixture();
        let indexed = simple_local_volatility(&close, &[0.0, 1.0 / 252.0, 1.0 / 252.0]).unwrap();
        let scalar = simple_local_volatility_constant_fraction(&close, 1.0 / 252.0).unwrap();
        assert_eq!(indexed, scalar);
        assert_eq!(indexed.first_valid, 1);
        assert_eq!(indexed.values[0], 0.0);
        assert!((indexed.values[1] - 1.5130021990505673).abs() < 1e-12);
        assert!((indexed.values[2] - 1.6725463346168112).abs() < 1e-12);
        let constant = constant_volatility(&indexed, 1).unwrap();
        assert_eq!(constant.first_valid, 2);
        assert_eq!(constant.values[0..2], [0.0, 0.0]);
        assert!((constant.values[2] - 1.0698541148988145).abs() < 1e-12);
    }

    #[test]
    fn local_validates_inputs_even_without_an_interval() {
        assert!(simple_local_volatility(&[100.0], &[]).is_err());
        for close in [0.0, -1.0, Real::NAN, Real::INFINITY] {
            assert!(simple_local_volatility_constant_fraction(&[close], 1.0).is_err());
        }
        for fraction in [0.0, -1.0, Real::NAN, Real::INFINITY] {
            assert!(simple_local_volatility_constant_fraction(&[], fraction).is_err());
            assert!(simple_local_volatility(&[100.0, 101.0], &[0.0, fraction]).is_err());
        }
        assert_eq!(simple_local_volatility(&[], &[]).unwrap().first_valid, 0);
        assert_eq!(
            simple_local_volatility(&[100.0], &[Real::NAN])
                .unwrap()
                .values,
            [0.0]
        );
    }

    #[test]
    fn short_series_are_aligned_and_zeroed() {
        let local = simple_local_volatility_constant_fraction(&[100.0], 1.0).unwrap();
        assert_eq!(local.first_valid, 1);
        assert_eq!(local.values, [0.0]);
        let constant = constant_volatility(&local, 2).unwrap();
        assert_eq!(constant.first_valid, 1);
        assert_eq!(constant.values, [0.0]);
        let empty = constant_volatility(
            &ChartSeries {
                values: vec![],
                first_valid: 0,
            },
            1,
        )
        .unwrap();
        assert_eq!(empty.first_valid, 0);
        assert!(empty.values.is_empty());
    }

    #[test]
    fn constant_validates_only_the_valid_suffix() {
        let input = ChartSeries {
            values: vec![Real::NAN, -1.0, 2.0, 4.0, 9.0],
            first_valid: 2,
        };
        let result = constant_volatility(&input, 2).unwrap();
        assert_eq!(result.first_valid, 4);
        assert!(result.values[..4].iter().all(|value| *value == 0.0));
        assert!((result.values[4] - 2.0).abs() < 1e-12);
        assert!(constant_volatility(&input, 0).is_err());
        assert!(
            constant_volatility(
                &ChartSeries {
                    values: vec![1.0],
                    first_valid: 2
                },
                1
            )
            .is_err()
        );
        for invalid in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            assert!(
                constant_volatility(
                    &ChartSeries {
                        values: vec![invalid],
                        first_valid: 0
                    },
                    2
                )
                .is_err()
            );
        }
    }

    #[test]
    fn constant_accepts_signed_values_like_quantlib() {
        let input = ChartSeries {
            values: vec![-1.0, 2.0, 9.0],
            first_valid: 0,
        };
        let result = constant_volatility(&input, 2).unwrap();
        assert_eq!(result.first_valid, 2);
        assert!((result.values[2] - (7.0_f64 / 3.0).sqrt()).abs() < 1e-12);
        let extreme = ChartSeries {
            values: vec![-Real::MAX, Real::MAX, 0.0],
            first_valid: 0,
        };
        assert_eq!(
            constant_volatility(&extreme, 2).unwrap().values[2],
            Real::MAX
        );
    }

    #[test]
    fn finite_extremes_avoid_intermediate_overflow() {
        let local =
            simple_local_volatility_constant_fraction(&[Real::MIN_POSITIVE, Real::MAX], 1.0)
                .unwrap();
        assert!(local.values[1].is_finite());
        assert!(local.values[1] > 1400.0);
        let reverse =
            simple_local_volatility_constant_fraction(&[Real::MAX, Real::MIN_POSITIVE], 1.0)
                .unwrap();
        assert!((local.values[1] - reverse.values[1]).abs() < 1e-12);
        let input = ChartSeries {
            values: vec![Real::MAX, Real::MAX, 0.0],
            first_valid: 0,
        };
        let result = constant_volatility(&input, 2).unwrap();
        assert!(result.values[2].is_finite());
        assert!((result.values[2] / Real::MAX - 1.0 / 3.0_f64.sqrt()).abs() < 1e-12);
    }
}
