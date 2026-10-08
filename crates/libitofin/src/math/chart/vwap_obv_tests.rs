use super::{obv, vwap};

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2e-14 * expected.abs().max(1.0),
        "{actual} != {expected}"
    );
}

#[test]
fn independently_calculated_cumulative_fixture() {
    let price = [10.0, 12.0, 11.0, 11.0, 9.0];
    let volume = [0.0, 2.0, 1.0, 0.0, 3.0];
    let result = vwap(&price, &volume).unwrap();
    assert_eq!(result.first_valid, 1);
    for (&actual, expected) in
        result
            .values
            .iter()
            .zip([0.0, 12.0, 35.0 / 3.0, 35.0 / 3.0, 31.0 / 3.0])
    {
        close(actual, expected);
    }
    assert_eq!(result.get(0), None);
    assert_eq!(result.get(1), Some(12.0));
    assert_eq!(result.get(price.len()), None);
    assert_eq!(
        obv(&price, &volume).unwrap().values,
        [0.0, 2.0, 1.0, 1.0, -2.0]
    );
}

#[test]
fn vwap_is_cumulative_not_rolling_and_session_reset_is_explicit() {
    let result = vwap(&[2.0, 4.0, 8.0], &[1.0, 1.0, 2.0]).unwrap();
    assert_eq!(result.values, [2.0, 3.0, 5.5]);
    assert_eq!(result.first_valid, 0);
    let reset = vwap(&[8.0], &[2.0]).unwrap();
    assert_eq!(reset.values, [8.0]);
    assert_eq!(reset.first_valid, 0);
}

#[test]
fn vwap_preserves_negative_and_zero_caller_prices() {
    let result = vwap(&[-3.0, -1.0, 1.0], &[1.0, 1.0, 1.0]).unwrap();
    for (&actual, expected) in result.values.iter().zip([-3.0, -2.0, -1.0]) {
        close(actual, expected);
    }
    assert_eq!(vwap(&[0.0, 0.0], &[1.0, 2.0]).unwrap().values, [0.0, 0.0]);
}

#[test]
fn missing_vwap_prefix_and_zero_volume_carry_align() {
    let result = vwap(&[100.0, 200.0, 3.0, 999.0], &[0.0, -0.0, 2.0, 0.0]).unwrap();
    assert_eq!(result.first_valid, 2);
    assert_eq!(result.values, [0.0, 0.0, 3.0, 3.0]);
    assert_eq!(result.get(1), None);
    assert_eq!(result.get(2), Some(3.0));
    let missing = vwap(&[1.0, -1.0], &[0.0, 0.0]).unwrap();
    assert_eq!(missing.first_valid, 2);
    assert_eq!(missing.values, [0.0, 0.0]);
    assert_eq!(missing.get(0), None);
    assert_eq!(missing.get(1), None);
}

#[test]
fn empty_and_single_bar_contracts() {
    for result in [vwap(&[], &[]).unwrap(), obv(&[], &[]).unwrap()] {
        assert!(result.values.is_empty());
        assert_eq!(result.first_valid, 0);
        assert_eq!(result.get(0), None);
    }
    let single = vwap(&[7.0], &[3.0]).unwrap();
    assert_eq!(single.values, [7.0]);
    assert_eq!(single.first_valid, 0);
    assert_eq!(obv(&[7.0], &[3.0]).unwrap().values, [0.0]);
    let missing = vwap(&[7.0], &[0.0]).unwrap();
    assert_eq!(missing.first_valid, 1);
    assert_eq!(missing.get(0), None);
}

#[test]
fn obv_zero_seed_equal_closes_and_zero_volumes() {
    let result = obv(&[10.0, 12.0, 12.0, 8.0, 9.0], &[999.0, 2.0, 50.0, 3.0, 0.0]).unwrap();
    assert_eq!(result.values, [0.0, 2.0, 2.0, -1.0, -1.0]);
    assert_eq!(result.first_valid, 0);
    assert_eq!(result.get(0), Some(0.0));
    assert_eq!(
        obv(&[-3.0, -1.0, 1.0], &[1.0, 1.0, 1.0]).unwrap().values,
        [0.0, 1.0, 2.0]
    );
    assert_eq!(
        obv(&[1.0, 2.0, 3.0], &[0.0, 0.0, -0.0]).unwrap().values,
        [0.0, 0.0, 0.0]
    );
}

#[test]
fn invalid_lengths_and_every_nonfinite_position_are_rejected() {
    for function in [vwap, obv] {
        assert!(function(&[], &[0.0]).is_err());
        assert!(function(&[1.0], &[]).is_err());
        assert!(function(&[1.0, 2.0], &[1.0]).is_err());
        for index in 0..3 {
            for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
                let mut price = [1.0, 2.0, 3.0];
                price[index] = invalid;
                assert!(function(&price, &[0.0, 0.0, 0.0]).is_err());
                let mut volume = [0.0, 0.0, 0.0];
                volume[index] = invalid;
                assert!(function(&[1.0, 2.0, 3.0], &volume).is_err());
            }
            let mut volume = [0.0, 0.0, 0.0];
            volume[index] = -1.0;
            assert!(function(&[1.0, 2.0, 3.0], &volume).is_err());
        }
    }
}

#[test]
fn vwap_avoids_price_volume_product_overflow() {
    let result = vwap(&[f64::MAX, f64::MAX], &[2.0, 2.0]).unwrap();
    assert_eq!(result.values, [f64::MAX, f64::MAX]);
    let result = vwap(&[-f64::MAX, -f64::MAX], &[2.0, 2.0]).unwrap();
    assert_eq!(result.values, [-f64::MAX, -f64::MAX]);
    assert_eq!(vwap(&[f64::MAX], &[f64::MAX]).unwrap().values, [f64::MAX]);
}

#[test]
fn opposite_signed_extreme_prices_have_finite_convex_means() {
    let result = vwap(&[-f64::MAX, f64::MAX, -f64::MAX], &[1.0, 1.0, 2.0]).unwrap();
    assert_eq!(result.values, [-f64::MAX, 0.0, -f64::MAX * 0.5]);
    let result = vwap(&[f64::MAX, -f64::MAX], &[1.0, 3.0]).unwrap();
    close(result.values[1] / f64::MAX, -0.5);
    assert_eq!(
        obv(&[-f64::MAX, f64::MAX, -f64::MAX], &[1.0, 2.0, 3.0])
            .unwrap()
            .values,
        [0.0, 2.0, -1.0]
    );
}

#[test]
fn subnormal_volumes_have_exact_initial_prices_and_nonzero_weight() {
    let tiny = f64::from_bits(1);
    let result = vwap(&[3.0, 5.0], &[tiny, tiny]).unwrap();
    assert_eq!(result.values, [3.0, 4.0]);
    assert_eq!(vwap(&[tiny], &[tiny]).unwrap().values, [tiny]);
    let result = vwap(&[0.0, -f64::MAX], &[0.0, tiny]).unwrap();
    assert_eq!(result.first_valid, 1);
    assert_eq!(result.values, [0.0, -f64::MAX]);
}

#[test]
fn volume_and_obv_overflow_are_errors_with_bar_index() {
    let error = vwap(&[0.0, 0.0], &[f64::MAX, f64::MAX]).unwrap_err();
    assert!(error.message().contains("cumulative volume at index 1"));
    let error = obv(&[1.0, 2.0, 3.0], &[0.0, f64::MAX, f64::MAX]).unwrap_err();
    assert!(error.message().contains("OBV at index 2"));
    let error = obv(&[3.0, 2.0, 1.0], &[0.0, f64::MAX, f64::MAX]).unwrap_err();
    assert!(error.message().contains("OBV at index 2"));
}

#[test]
fn obv_does_not_sum_unsigned_volumes_or_initial_seed() {
    let result = obv(&[1.0, 1.0, 1.0], &[f64::MAX; 3]).unwrap();
    assert_eq!(result.values, [0.0, 0.0, 0.0]);
    let result = obv(&[1.0, 2.0, 1.0, 2.0], &[f64::MAX; 4]).unwrap();
    assert_eq!(result.values, [0.0, f64::MAX, 0.0, f64::MAX]);
}

#[test]
fn independent_weight_ratios_preserve_dominant_new_volume_tail() {
    let result = vwap(&[f64::MAX, 0.0], &[1.0, f64::MAX]).unwrap();
    close(result.values[1], 1.0);
    let result = vwap(&[-f64::MAX, 0.0], &[1.0, f64::MAX]).unwrap();
    close(result.values[1], -1.0);
    let result = vwap(&[f64::MAX, -1.0], &[1.0, f64::MAX]).unwrap();
    close(result.values[1], 0.0);
    let result = vwap(&[-f64::MAX, 1.0], &[1.0, f64::MAX]).unwrap();
    close(result.values[1], 0.0);
}

#[test]
fn same_sign_convex_mean_anchors_on_dominant_weight() {
    let low_weight = vwap(&[4.0, 8.0], &[3.0, 1.0]).unwrap();
    let high_weight = vwap(&[4.0, 8.0], &[1.0, 3.0]).unwrap();
    assert_eq!(low_weight.values, [4.0, 5.0]);
    assert_eq!(high_weight.values, [4.0, 7.0]);
    assert_eq!(
        vwap(&[-4.0, -8.0], &[1.0, 3.0]).unwrap().values,
        [-4.0, -7.0]
    );
    let extremes = vwap(&[f64::MAX, 0.0], &[1.0, 3.0]).unwrap();
    assert_eq!(extremes.values[1], f64::MAX * 0.25);
}
