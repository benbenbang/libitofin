use super::{keltner_channels, keltner_channels_default};
use crate::math::chart::{atr, ema};

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 8.0 * f64::EPSILON * expected.abs().max(1.0),
        "actual {actual}, expected {expected}"
    );
}

#[test]
fn independent_fraction_fixture_preserves_close_ema_and_wilder_atr() {
    let high = [12.0, 16.0, 11.0, 15.0, 14.0, 14.0];
    let low = [10.0, 14.0, 9.0, 13.0, 12.0, 14.0];
    let close = [11.0, 15.0, 10.0, 14.0, 13.0, 14.0];
    let result = keltner_channels(&high, &low, &close, 3, 2, 1.5).unwrap();
    let centers = [12.0, 13.0, 13.0, 13.5];
    let uppers = [153.0 / 8.0, 325.0 / 16.0, 581.0 / 32.0, 1077.0 / 64.0];
    let lowers = [39.0 / 8.0, 91.0 / 16.0, 251.0 / 32.0, 651.0 / 64.0];
    for series in [&result.center, &result.upper, &result.lower] {
        assert_eq!(series.first_valid, 2);
        assert_eq!(series.values[..2], [0.0, 0.0]);
        assert_eq!(series.get(1), None);
    }
    for index in 0..4 {
        assert_close(result.center.values[index + 2], centers[index]);
        assert_close(result.upper.values[index + 2], uppers[index]);
        assert_close(result.lower.values[index + 2], lowers[index]);
    }
}

#[test]
fn either_period_can_control_combined_warmup() {
    let close = [1.0, 2.0, 3.0, 4.0, 5.0];
    for (center_period, atr_period) in [(2, 4), (4, 2)] {
        let result =
            keltner_channels(&close, &close, &close, center_period, atr_period, 0.0).unwrap();
        assert_eq!(result.center.first_valid, 3);
        assert_eq!(result.upper.first_valid, 3);
        assert_eq!(result.lower.first_valid, 3);
        assert_eq!(result.center.values[..3], [0.0; 3]);
        assert_eq!(result.center, result.upper);
        assert_eq!(result.center, result.lower);
        let existing = ema(&close, center_period).unwrap();
        assert_eq!(result.center.values[3..], existing.values[3..]);
    }
}

#[test]
fn flat_negative_prices_have_collapsed_channels() {
    let result = keltner_channels(&[-3.0; 5], &[-3.0; 5], &[-3.0; 5], 3, 2, 7.0).unwrap();
    assert_eq!(result.center.values, [0.0, 0.0, -3.0, -3.0, -3.0]);
    assert_eq!(result.center, result.upper);
    assert_eq!(result.center, result.lower);
}

#[test]
fn period_one_uses_current_close_and_gap_aware_range() {
    let result = keltner_channels(&[2.0, 6.0], &[0.0, 4.0], &[1.0, 5.0], 1, 1, 2.0).unwrap();
    assert_eq!(result.center.first_valid, 0);
    assert_eq!(result.center.values, [1.0, 5.0]);
    assert_eq!(result.upper.values, [5.0, 15.0]);
    assert_eq!(result.lower.values, [-3.0, -5.0]);
}

#[test]
fn default_parameters_are_twenty_ten_and_two() {
    let close: Vec<f64> = (1..=24).map(f64::from).collect();
    let result = keltner_channels_default(&close, &close, &close).unwrap();
    assert_eq!(
        result,
        keltner_channels(&close, &close, &close, 20, 10, 2.0).unwrap()
    );
    assert_eq!(result.center.first_valid, 19);
    let center = ema(&close, 20).unwrap();
    let range = atr(&close, &close, &close, 10).unwrap();
    for index in 19..close.len() {
        assert_eq!(result.center.values[index], center.values[index]);
        assert_close(
            result.upper.values[index],
            center.values[index] + 2.0 * range.values[index],
        );
        assert_close(
            result.lower.values[index],
            center.values[index] - 2.0 * range.values[index],
        );
    }
    assert_ne!(
        result,
        keltner_channels(&close, &close, &close, 20, 14, 2.0).unwrap()
    );
}

#[test]
fn empty_and_short_channels_are_aligned_missing_series() {
    let empty = keltner_channels_default(&[], &[], &[]).unwrap();
    for series in [empty.center, empty.upper, empty.lower] {
        assert!(series.values.is_empty());
        assert_eq!(series.first_valid, 0);
    }
    let result = keltner_channels(&[2.0], &[0.0], &[1.0], 2, 3, 2.0).unwrap();
    for series in [result.center, result.upper, result.lower] {
        assert_eq!(series.values, [0.0]);
        assert_eq!(series.first_valid, 1);
        assert_eq!(series.get(0), None);
    }
    assert_eq!(
        keltner_channels(&[2.0], &[0.0], &[1.0], usize::MAX, 1, 2.0)
            .unwrap()
            .center
            .first_valid,
        1
    );
}

#[test]
fn zero_multiplier_preserves_finite_extreme_center() {
    let result = keltner_channels(&[f64::MAX], &[0.0], &[f64::MAX], 1, 1, 0.0).unwrap();
    assert_eq!(result.center.values, [f64::MAX]);
    assert_eq!(result.center, result.upper);
    assert_eq!(result.center, result.lower);
}

#[test]
fn rejects_invalid_parameters_even_for_empty_input() {
    for (center_period, atr_period) in [(0, 1), (1, 0), (0, 0)] {
        assert!(keltner_channels(&[], &[], &[], center_period, atr_period, 2.0).is_err());
    }
    for multiplier in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(keltner_channels(&[], &[], &[], 1, 1, multiplier).is_err());
    }
}

#[test]
fn rejects_invalid_bars_and_raw_range_overflow_during_warmup() {
    for (high, low, close) in [
        (vec![], vec![0.0], vec![1.0]),
        (vec![2.0], vec![3.0], vec![1.0]),
        (vec![f64::NAN], vec![0.0], vec![1.0]),
        (vec![2.0], vec![f64::NEG_INFINITY], vec![1.0]),
        (vec![2.0], vec![0.0], vec![f64::INFINITY]),
        (vec![f64::MAX], vec![-f64::MAX], vec![0.0]),
    ] {
        assert!(keltner_channels(&high, &low, &close, 20, 10, 0.0).is_err());
    }
}

#[test]
fn rejects_offset_and_either_band_overflow_separately() {
    let offset = keltner_channels(&[f64::MAX], &[0.0], &[0.0], 1, 1, 2.0).unwrap_err();
    assert!(offset.to_string().contains("Keltner offset"));
    let upper = keltner_channels(&[f64::MAX], &[0.0], &[f64::MAX], 1, 1, 1.0).unwrap_err();
    assert!(upper.to_string().contains("Keltner band"));
    let lower = keltner_channels(&[0.0], &[-f64::MAX], &[-f64::MAX], 1, 1, 1.0).unwrap_err();
    assert!(lower.to_string().contains("Keltner band"));
}

#[test]
fn center_uses_close_not_typical_price() {
    let result = keltner_channels(
        &[9.0, 12.0, 15.0],
        &[0.0, 3.0, 6.0],
        &[0.0, 3.0, 6.0],
        3,
        2,
        0.0,
    )
    .unwrap();
    assert_eq!(result.center.values, [0.0, 0.0, 3.0]);
    assert_eq!(result.center, result.upper);
    assert_eq!(result.center, result.lower);
}
