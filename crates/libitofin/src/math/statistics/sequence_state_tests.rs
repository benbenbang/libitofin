use super::tests::weighted;
use super::*;

#[test]
fn rejected_samples_preserve_all_observable_state() {
    let mut statistics = weighted();
    let old_mean = statistics.mean().unwrap();
    let old_covariance = statistics.covariance().unwrap();
    for (sample, weight) in [
        (&[1.0][..], 1.0),
        (&[1.0, Real::NAN][..], 0.0),
        (&[Real::INFINITY, 1.0][..], 0.0),
        (&[1.0, 2.0][..], -1.0),
        (&[1.0, 2.0][..], Real::NAN),
        (&[1.0, 2.0][..], Real::INFINITY),
    ] {
        assert!(statistics.add_weighted(sample, weight).is_err());
        assert_eq!(statistics.samples(), 4);
        assert_eq!(statistics.weight_sum(), 4.0);
        assert_eq!(statistics.mean().unwrap(), old_mean);
        assert_eq!(statistics.covariance().unwrap(), old_covariance);
    }
}

#[test]
fn rejected_weight_sum_overflow_is_atomic() {
    let mut statistics = SequenceStatistics::new(1).unwrap();
    statistics.add_weighted(&[2.0], Real::MAX).unwrap();
    assert!(statistics.add_weighted(&[3.0], Real::MAX).is_err());
    assert_eq!(statistics.samples(), 1);
    assert_eq!(statistics.weight_sum(), Real::MAX);
    assert_eq!(statistics.mean().unwrap(), [2.0]);
}

#[test]
fn reset_rejects_bad_dimensions_atomically_and_clears_valid_dimension() {
    let mut statistics = weighted();
    for dimension in [0, MAX_SEQUENCE_DIMENSION + 1, usize::MAX] {
        assert!(statistics.reset(dimension).is_err());
        assert_eq!(statistics.dimension(), 2);
        assert_eq!(statistics.samples(), 4);
        assert_eq!(statistics.weight_sum(), 4.0);
    }
    statistics.reset(3).unwrap();
    assert_eq!(statistics.dimension(), 3);
    assert_eq!(statistics.samples(), 0);
    assert_eq!(statistics.weight_sum(), 0.0);
    assert!(statistics.mean().is_err());
    statistics.add(&[1.0, 2.0, 3.0]).unwrap();
    assert_eq!(statistics.mean().unwrap(), [1.0, 2.0, 3.0]);
}

#[test]
fn zero_weight_queries_are_rejected_including_extrema() {
    let mut statistics = SequenceStatistics::new(1).unwrap();
    statistics.add_weighted(&[1.0], 0.0).unwrap();
    statistics.add_weighted(&[2.0], 0.0).unwrap();
    assert_eq!(statistics.samples(), 2);
    for result in [
        statistics.mean(),
        statistics.variance(),
        statistics.standard_deviation(),
        statistics.error_estimate(),
        statistics.min(),
        statistics.max(),
        statistics.covariance(),
        statistics.correlation(),
    ] {
        assert!(result.is_err());
    }
}

#[test]
fn one_row_allows_mean_extrema_but_not_moments() {
    let mut statistics = SequenceStatistics::new(2).unwrap();
    statistics.add(&[1.0, 2.0]).unwrap();
    assert_eq!(statistics.mean().unwrap(), [1.0, 2.0]);
    assert_eq!(statistics.min().unwrap(), [1.0, 2.0]);
    assert_eq!(statistics.max().unwrap(), [1.0, 2.0]);
    for result in [
        statistics.variance(),
        statistics.standard_deviation(),
        statistics.error_estimate(),
        statistics.covariance(),
        statistics.correlation(),
    ] {
        assert!(result.is_err());
    }
}

#[test]
fn shape_validation_checks_every_cap_before_allocation() {
    let mean = SequenceStatistic::Mean;
    let covariance = SequenceStatistic::Covariance;
    for (rows, dimension) in [
        (0, 1),
        (1, 0),
        (1, 257),
        (100_001, 1),
        (100_000, 11),
        (usize::MAX, usize::MAX),
    ] {
        assert!(validate_sequence_shape(rows, dimension, mean).is_err());
    }
    assert_eq!(validate_sequence_shape(100_000, 10, mean).unwrap(), 10);
    assert_eq!(validate_sequence_shape(3_906, 256, mean).unwrap(), 256);
    assert!(validate_sequence_shape(1_526, 256, covariance).is_err());
    assert_eq!(
        validate_sequence_shape(1_525, 256, covariance).unwrap(),
        65_536
    );
    assert!(SequenceStatistics::new(0).is_err());
    assert!(SequenceStatistics::new(257).is_err());
    assert!(evaluate_sequence_batch(&[], 1, 1, None, mean).is_err());
    assert!(evaluate_sequence_batch(&[1.0], 1, 1, Some(&[]), mean).is_err());
    assert!(evaluate_sequence_batch(&[Real::NAN], 1, 1, Some(&[0.0]), mean).is_err());
}

#[test]
fn row_and_component_caps_reject_adds_without_mutation() {
    let mut rows = SequenceStatistics::new(1).unwrap();
    for _ in 0..MAX_SEQUENCE_ROWS {
        rows.add(&[2.0]).unwrap();
    }
    assert!(rows.add(&[3.0]).is_err());
    assert_eq!(rows.samples(), MAX_SEQUENCE_ROWS);
    assert_eq!(rows.mean().unwrap(), [2.0]);
    let mut components = SequenceStatistics::new(256).unwrap();
    for _ in 0..3_906 {
        components.add(&[2.0; 256]).unwrap();
    }
    assert!(components.add(&[3.0; 256]).is_err());
    assert_eq!(components.samples(), 3_906);
    assert_eq!(components.mean().unwrap(), vec![2.0; 256]);
    assert!(components.covariance().is_err());
    assert!(components.correlation().is_err());
    assert_eq!(components.variance().unwrap(), vec![0.0; 256]);
}
