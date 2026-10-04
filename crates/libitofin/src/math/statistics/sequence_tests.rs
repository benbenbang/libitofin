use super::*;

fn close(actual: &[Real], expected: &[Real], tolerance: Real) {
    assert_eq!(actual.len(), expected.len());
    for (&actual, &expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() <= tolerance * expected.abs().max(1.0),
            "actual {actual}, expected {expected}"
        );
    }
}

pub(super) fn weighted() -> SequenceStatistics {
    let mut statistics = SequenceStatistics::new(2).unwrap();
    for (row, weight) in [
        ([1.0, 2.0], 1.0),
        ([3.0, 6.0], 2.0),
        ([5.0, 10.0], 1.0),
        ([-7.0, 20.0], 0.0),
    ] {
        statistics.add_weighted(&row, weight).unwrap();
    }
    statistics
}

#[test]
fn weighted_hand_calculation_uses_count_correction() {
    let statistics = weighted();
    assert_eq!(statistics.samples(), 4);
    assert_eq!(statistics.weight_sum(), 4.0);
    close(&statistics.mean().unwrap(), &[3.0, 6.0], 1e-15);
    close(
        &statistics.variance().unwrap(),
        &[8.0 / 3.0, 32.0 / 3.0],
        1e-15,
    );
    close(
        &statistics.covariance().unwrap(),
        &[8.0 / 3.0, 16.0 / 3.0, 16.0 / 3.0, 32.0 / 3.0],
        1e-15,
    );
    close(&statistics.correlation().unwrap(), &[1.0; 4], 1e-15);
    close(
        &statistics.standard_deviation().unwrap(),
        &[(8.0_f64 / 3.0).sqrt(), (32.0_f64 / 3.0).sqrt()],
        1e-15,
    );
    close(
        &statistics.error_estimate().unwrap(),
        &[(2.0_f64 / 3.0).sqrt(), (8.0_f64 / 3.0).sqrt()],
        1e-15,
    );
    assert_eq!(statistics.min().unwrap(), [-7.0, 2.0]);
    assert_eq!(statistics.max().unwrap(), [5.0, 20.0]);
}

#[test]
fn batch_rows_and_matrix_outputs_are_row_major() {
    let values = [1.0, 10.0, 5.0, 30.0, 7.0, -10.0];
    close(
        &evaluate_sequence_batch(&values, 3, 2, None, SequenceStatistic::Mean).unwrap(),
        &[13.0 / 3.0, 10.0],
        1e-15,
    );
    close(
        &evaluate_sequence_batch(&values, 3, 2, None, SequenceStatistic::Covariance).unwrap(),
        &[28.0 / 3.0, -20.0, -20.0, 400.0],
        1e-15,
    );
    assert_eq!(
        evaluate_sequence_batch(&values, 3, 2, None, SequenceStatistic::Minimum).unwrap(),
        [1.0, -10.0]
    );
    assert_eq!(
        evaluate_sequence_batch(&values, 3, 2, None, SequenceStatistic::Maximum).unwrap(),
        [7.0, 30.0]
    );
}

#[test]
fn all_batch_measures_match_accumulator() {
    let samples = [1.0, 2.0, 3.0, 6.0, 5.0, 10.0, -7.0, 20.0];
    let weights = [1.0, 2.0, 1.0, 0.0];
    let statistics = weighted();
    for (measure, expected) in [
        (SequenceStatistic::Mean, statistics.mean()),
        (SequenceStatistic::Variance, statistics.variance()),
        (
            SequenceStatistic::StandardDeviation,
            statistics.standard_deviation(),
        ),
        (
            SequenceStatistic::ErrorEstimate,
            statistics.error_estimate(),
        ),
        (SequenceStatistic::Minimum, statistics.min()),
        (SequenceStatistic::Maximum, statistics.max()),
        (SequenceStatistic::Covariance, statistics.covariance()),
        (SequenceStatistic::Correlation, statistics.correlation()),
    ] {
        assert_eq!(
            evaluate_sequence_batch(&samples, 4, 2, Some(&weights), measure).unwrap(),
            expected.unwrap()
        );
    }
}

#[test]
fn covariance_is_exactly_symmetric_and_matches_variance_diagonal() {
    let mut statistics = SequenceStatistics::new(3).unwrap();
    for row in [[1.2, -3.0, 8.0], [7.1, 8.0, -1.0], [2.0, 4.0, 3.1]] {
        statistics.add(&row).unwrap();
    }
    let covariance = statistics.covariance().unwrap();
    let variance = statistics.variance().unwrap();
    for i in 0..3 {
        assert_eq!(covariance[i * 3 + i], variance[i]);
        for j in 0..3 {
            assert_eq!(covariance[i * 3 + j], covariance[j * 3 + i]);
        }
    }
    let correlation = statistics.correlation().unwrap();
    for i in 0..3 {
        assert_eq!(correlation[i * 3 + i], 1.0);
        for j in 0..3 {
            assert_eq!(correlation[i * 3 + j], correlation[j * 3 + i]);
        }
    }
}

#[test]
fn constant_components_use_native_correlation_convention() {
    let mut statistics = SequenceStatistics::new(3).unwrap();
    for row in [[7.0, -2.0, 1.0], [7.0, -2.0, 2.0], [7.0, -2.0, 3.0]] {
        statistics.add(&row).unwrap();
    }
    close(
        &statistics.covariance().unwrap(),
        &[0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0],
        1e-15,
    );
    assert_eq!(
        statistics.correlation().unwrap(),
        [1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    );
}

#[test]
fn large_offset_and_near_constant_variance_remain_centered() {
    let base = 1e16;
    let mut statistics = SequenceStatistics::new(2).unwrap();
    for offset in [0.0, 2.0, 4.0] {
        statistics
            .add(&[base + offset, -base - 2.0 * offset])
            .unwrap();
    }
    assert_eq!(statistics.mean().unwrap(), [base + 2.0, -base - 4.0]);
    close(&statistics.variance().unwrap(), &[4.0, 16.0], 1e-15);
    close(
        &statistics.covariance().unwrap(),
        &[4.0, -8.0, -8.0, 16.0],
        1e-15,
    );
    close(
        &statistics.correlation().unwrap(),
        &[1.0, -1.0, -1.0, 1.0],
        1e-15,
    );
    let mut tiny = SequenceStatistics::new(1).unwrap();
    for value in [1.0, 1.0 + 1e-12, 1.0 - 1e-12] {
        tiny.add(&[value]).unwrap();
    }
    assert!(tiny.variance().unwrap()[0] > 1e-25);
}

#[test]
fn large_weights_are_normalized_before_sample_products() {
    let mut statistics = SequenceStatistics::new(1).unwrap();
    statistics.add_weighted(&[1e100], 1e300).unwrap();
    statistics.add_weighted(&[1e100 + 1e90], 1e300).unwrap();
    assert!(statistics.mean().unwrap()[0].is_finite());
    assert!(statistics.variance().unwrap()[0].is_finite());
    let difference = (1e100 + 1e90) - 1e100;
    close(
        &statistics.variance().unwrap(),
        &[difference * difference / 2.0],
        1e-15,
    );
}

#[test]
fn tiny_positive_weight_can_contribute_finite_extreme_moments() {
    let mut statistics = SequenceStatistics::new(1).unwrap();
    statistics.add_weighted(&[0.0], 1e300).unwrap();
    statistics.add_weighted(&[1e300], 1e-300).unwrap();
    close(&statistics.mean().unwrap(), &[1e-300], 1e-15);
    close(&statistics.variance().unwrap(), &[2.0], 1e-15);
    let mut tiny = SequenceStatistics::new(1).unwrap();
    tiny.add_weighted(&[1.0], Real::from_bits(1)).unwrap();
    tiny.add_weighted(&[3.0], Real::from_bits(1)).unwrap();
    close(&tiny.mean().unwrap(), &[2.0], 1e-15);
    close(&tiny.variance().unwrap(), &[2.0], 1e-15);
}

#[test]
fn opposite_large_values_mean_is_finite_but_unrepresentable_variance_errors() {
    let mut statistics = SequenceStatistics::new(1).unwrap();
    statistics.add_weighted(&[Real::MAX], 1e-300).unwrap();
    statistics.add_weighted(&[-Real::MAX], 1e-300).unwrap();
    assert_eq!(statistics.mean().unwrap(), [0.0]);
    assert!(statistics.variance().is_err());
    assert!(statistics.covariance().is_err());
    assert!(statistics.correlation().is_err());
}

#[test]
fn huge_constants_and_zero_weight_extremes_do_not_overflow() {
    let mut statistics = SequenceStatistics::new(2).unwrap();
    statistics
        .add_weighted(&[Real::MAX, Real::MAX], 1e300)
        .unwrap();
    statistics
        .add_weighted(&[Real::MAX, Real::MAX], 1e300)
        .unwrap();
    statistics
        .add_weighted(&[-Real::MAX, -Real::MAX], 0.0)
        .unwrap();
    assert_eq!(statistics.mean().unwrap(), [Real::MAX; 2]);
    assert_eq!(statistics.variance().unwrap(), [0.0; 2]);
    assert_eq!(statistics.covariance().unwrap(), [0.0; 4]);
    assert_eq!(statistics.correlation().unwrap(), [1.0; 4]);
    assert_eq!(statistics.min().unwrap(), [-Real::MAX; 2]);
}

#[test]
fn correlation_avoids_variance_product_overflow() {
    let mut statistics = SequenceStatistics::new(2).unwrap();
    statistics.add(&[1e100, -1e100]).unwrap();
    statistics.add(&[2e100, -2e100]).unwrap();
    close(
        &statistics.correlation().unwrap(),
        &[1.0, -1.0, -1.0, 1.0],
        1e-15,
    );
}

#[test]
fn dominant_weight_center_prevents_mean_cancellation_in_both_row_orders() {
    for (values, weights, expected) in [
        ([1e16, 1.0], [1e-16, 1.0], 2.0),
        ([1e300, 0.0], [1e-300, 1e300], 1e-300),
    ] {
        for reverse in [false, true] {
            let mut statistics = SequenceStatistics::new(1).unwrap();
            let indices = if reverse { [1, 0] } else { [0, 1] };
            for index in indices {
                statistics
                    .add_weighted(&[values[index]], weights[index])
                    .unwrap();
            }
            assert!((statistics.mean().unwrap()[0] / expected - 1.0).abs() < 1e-15);
            assert!(statistics.variance().unwrap()[0].is_finite());
        }
    }
}

#[test]
fn cancellation_of_opposing_samples_retains_small_mean() {
    for values in [
        [1e16, 1.0, -1e16],
        [-1e16, 1.0, 1e16],
        [1.0, 1e16, -1e16],
        [1e16, -1e16, 1.0],
        [-1e16, 1e16, 1.0],
        [1.0, -1e16, 1e16],
    ] {
        close(
            &evaluate_sequence_batch(&values, 3, 1, None, SequenceStatistic::Mean).unwrap(),
            &[1.0 / 3.0],
            1e-15,
        );
    }
}
