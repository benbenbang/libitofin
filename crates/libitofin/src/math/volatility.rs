//! Close-price volatility estimators aligned with chart input bars.

use crate::errors::QlResult;
use crate::math::chart::ChartSeries;
use crate::require;
use crate::types::Real;

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
