use super::{atr, atr_default, true_range};

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 4.0 * f64::EPSILON * expected.abs().max(1.0),
        "actual {actual}, expected {expected}"
    );
}

#[test]
fn hand_calculated_gaps_and_wilder_recurrence() {
    let high = [12.0, 16.0, 11.0, 15.0, 14.0, 14.0];
    let low = [10.0, 14.0, 9.0, 13.0, 12.0, 14.0];
    let close = [11.0, 15.0, 10.0, 14.0, 13.0, 14.0];
    let ranges = true_range(&high, &low, &close).unwrap();
    assert_eq!(ranges.first_valid, 0);
    assert_eq!(ranges.values, [2.0, 5.0, 6.0, 5.0, 2.0, 1.0]);
    let result = atr(&high, &low, &close, 3).unwrap();
    assert_eq!(result.first_valid, 2);
    assert_eq!(result.values[..2], [0.0, 0.0]);
    assert_eq!(result.get(1), None);
    let expected = [13.0 / 3.0, 41.0 / 9.0, 100.0 / 27.0, 227.0 / 81.0];
    for (actual, expected) in result.values[2..].iter().zip(expected) {
        assert_close(*actual, expected);
    }
}

#[test]
fn default_uses_fourteen_bar_arithmetic_seed() {
    let mut high = [12.0; 15];
    let mut low = [10.0; 15];
    let mut close = [11.0; 15];
    high[14] = 15.0;
    low[14] = 13.0;
    close[14] = 14.0;
    let result = atr_default(&high, &low, &close).unwrap();
    assert_eq!(result, atr(&high, &low, &close, 14).unwrap());
    assert_eq!(result.first_valid, 13);
    assert_eq!(result.values[..13], [0.0; 13]);
    assert_eq!(result.values[13], 2.0);
    assert_close(result.values[14], 15.0 / 7.0);
}

#[test]
fn nonconstant_default_seed_and_next_bar_use_wilder_weights() {
    let high: Vec<f64> = (1..=15).map(f64::from).collect();
    let result = atr_default(&high, &[0.0; 15], &[0.0; 15]).unwrap();
    assert_eq!(result.first_valid, 13);
    assert_eq!(result.values[13], 7.5);
    assert_close(result.values[14], 225.0 / 28.0);
}

#[test]
fn zero_ranges_and_single_bar_follow_first_bar_policy() {
    let result = true_range(&[-2.0; 4], &[-2.0; 4], &[-2.0; 4]).unwrap();
    assert_eq!(result.values, [0.0; 4]);
    assert_eq!(
        atr(&[-2.0; 4], &[-2.0; 4], &[-2.0; 4], 2).unwrap().values,
        [0.0; 4]
    );
    assert_eq!(
        true_range(&[5.0], &[1.0], &[3.0]).unwrap().get(0),
        Some(4.0)
    );
    assert_eq!(atr(&[5.0], &[1.0], &[3.0], 1).unwrap().get(0), Some(4.0));
}

#[test]
fn period_one_preserves_tiny_range_after_a_huge_range() {
    let high = [f64::MAX, f64::from_bits(1)];
    let low = [0.0, 0.0];
    let close = [0.0, 0.0];
    let result = atr(&high, &low, &close, 1).unwrap();
    assert_eq!(result, true_range(&high, &low, &close).unwrap());
    assert_eq!(result.values[1].to_bits(), 1);
}

#[test]
fn empty_and_short_series_are_aligned_missing_values() {
    assert!(true_range(&[], &[], &[]).unwrap().values.is_empty());
    assert_eq!(atr(&[], &[], &[], 14).unwrap().first_valid, 0);
    let result = atr(&[2.0, 3.0], &[0.0, 1.0], &[1.0, 2.0], 3).unwrap();
    assert_eq!(result.first_valid, 2);
    assert_eq!(result.values, [0.0, 0.0]);
    assert_eq!(result.get(0), None);
    assert_eq!(result.get(1), None);
    let huge_period = atr(&[2.0], &[0.0], &[1.0], usize::MAX).unwrap();
    assert_eq!(huge_period.first_valid, 1);
    assert_eq!(huge_period.get(0), None);
}

#[test]
fn seed_and_smoothing_do_not_overflow_a_representable_mean() {
    let result = atr(&[f64::MAX; 5], &[0.0; 5], &[0.0; 5], 3).unwrap();
    assert_eq!(result.values, [0.0, 0.0, f64::MAX, f64::MAX, f64::MAX]);
    let mixed = atr(&[f64::MAX, 0.0, 0.0], &[0.0; 3], &[0.0; 3], 3).unwrap();
    assert_close(mixed.values[2] / f64::MAX, 1.0 / 3.0);
}

#[test]
fn rejects_zero_period_even_with_empty_data() {
    assert!(atr(&[], &[], &[], 0).is_err());
    assert!(atr(&[2.0], &[0.0], &[1.0], 0).is_err());
}

#[test]
fn rejects_every_length_mismatch() {
    for (high, low, close) in [
        (vec![], vec![0.0], vec![1.0]),
        (vec![2.0], vec![], vec![1.0]),
        (vec![2.0], vec![0.0], vec![]),
    ] {
        assert!(true_range(&high, &low, &close).is_err());
        assert!(atr(&high, &low, &close, 14).is_err());
    }
}

#[test]
fn rejects_nonfinite_values_in_every_input_and_warmup() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for column in 0..3 {
            let mut high = [2.0, 2.0];
            let mut low = [0.0, 0.0];
            let mut close = [1.0, 1.0];
            match column {
                0 => high[1] = value,
                1 => low[1] = value,
                _ => close[1] = value,
            }
            assert!(true_range(&high, &low, &close).is_err());
            assert!(atr(&high, &low, &close, 14).is_err());
        }
    }
}

#[test]
fn rejects_unordered_hlc_and_all_overflow_candidates() {
    for (high, low, close) in [
        (vec![0.0], vec![2.0], vec![1.0]),
        (vec![2.0], vec![0.0], vec![3.0]),
        (vec![2.0], vec![0.0], vec![-1.0]),
        (vec![f64::MAX], vec![-f64::MAX], vec![0.0]),
        (
            vec![-f64::MAX, f64::MAX],
            vec![-f64::MAX, 0.0],
            vec![-f64::MAX, 0.0],
        ),
        (
            vec![f64::MAX, 0.0],
            vec![f64::MAX, -f64::MAX],
            vec![f64::MAX, 0.0],
        ),
    ] {
        assert!(true_range(&high, &low, &close).is_err());
        assert!(atr(&high, &low, &close, 14).is_err());
    }
}
