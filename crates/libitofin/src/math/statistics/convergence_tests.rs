use super::*;

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2e-15 * expected.abs().max(1.0),
        "actual {actual}, expected {expected}"
    );
}

fn unchanged(statistics: &ConvergenceStatistics, previous: &ConvergenceStatistics) {
    assert_eq!(statistics.values, previous.values);
    assert_eq!(statistics.weights, previous.weights);
    assert_eq!(statistics.weight_sum(), previous.weight_sum());
    assert_eq!(statistics.convergence_table(), previous.convergence_table());
}

#[test]
fn doubling_steps_are_checked() {
    assert_eq!(DoublingConvergenceSteps::initial_samples(), 1);
    let mut current = 1;
    for expected in [3, 7, 15, 31, 63] {
        current = DoublingConvergenceSteps::next_samples(current).unwrap();
        assert_eq!(current, expected);
    }
    assert_eq!(DoublingConvergenceSteps::next_samples(0).unwrap(), 1);
    assert_eq!(
        DoublingConvergenceSteps::next_samples(usize::MAX / 2).unwrap(),
        usize::MAX
    );
    assert!(DoublingConvergenceSteps::next_samples(usize::MAX / 2 + 1).is_err());
    assert!(DoublingConvergenceSteps::next_samples(usize::MAX).is_err());
}

#[test]
fn input_length_preflight_returns_completed_checkpoints() {
    for (length, count) in [(0, 0), (1, 1), (2, 1), (3, 2), (6, 2), (7, 3), (15, 4)] {
        assert_eq!(validate_convergence_length(length).unwrap(), count);
    }
    assert_eq!(
        validate_convergence_length(MAX_CONVERGENCE_SAMPLES).unwrap(),
        16
    );
    assert!(validate_convergence_length(MAX_CONVERGENCE_SAMPLES + 1).is_err());
    assert!(validate_convergence_length(usize::MAX).is_err());
}

#[test]
fn addition_count_checks_overflow_and_cap_before_allocation() {
    assert_eq!(validate_addition(2, 5).unwrap(), 3);
    assert!(validate_addition(usize::MAX, 1).is_err());
    assert!(validate_addition(usize::MAX / 2 + 1, usize::MAX / 2 + 1).is_err());
    assert!(validate_addition(MAX_CONVERGENCE_SAMPLES, 1).is_err());
    assert_eq!(validate_addition(MAX_CONVERGENCE_SAMPLES, 0).unwrap(), 16);
}

#[test]
fn cumulative_checkpoints_exclude_incomplete_tail() {
    let values: Vec<_> = (1..=16).map(f64::from).collect();
    let mut statistics = ConvergenceStatistics::new();
    statistics.add_batch(&values, None).unwrap();
    assert_eq!(statistics.samples(), 16);
    assert_eq!(statistics.weight_sum(), 16.0);
    close(statistics.mean().unwrap(), 8.5);
    assert_eq!(
        statistics
            .convergence_table()
            .iter()
            .map(|point| point.samples)
            .collect::<Vec<_>>(),
        [1, 3, 7, 15]
    );
    for (point, expected) in statistics
        .convergence_table()
        .iter()
        .zip([1.0, 2.0, 4.0, 8.0])
    {
        close(point.mean, expected);
    }
    assert_eq!(
        evaluate_convergence_batch(&values, None).unwrap(),
        statistics.table
    );
}

#[test]
fn weighted_prefix_means_count_zero_weight_samples() {
    let values = [2.0, 1000.0, 8.0, 10.0, -1000.0, 6.0, 4.0, 100.0];
    let weights = [2.0, 0.0, 1.0, 3.0, 0.0, 2.0, 2.0, 5.0];
    let mut statistics = ConvergenceStatistics::new();
    statistics.add_batch(&values, Some(&weights)).unwrap();
    assert_eq!(statistics.samples(), 8);
    assert_eq!(statistics.weight_sum(), 15.0);
    for (point, (count, mean)) in statistics.table.iter().zip([(1, 2.0), (3, 4.0), (7, 6.2)]) {
        assert_eq!(point.samples, count);
        close(point.mean, mean);
    }
    close(statistics.mean().unwrap(), 562.0 / 15.0);
}

#[test]
fn streaming_and_differently_partitioned_batches_are_identical() {
    let values: Vec<_> = (0..100).map(|value| f64::from(value) - 15.0).collect();
    let weights: Vec<_> = (0..100).map(|index| f64::from(index % 4)).collect();
    let mut weights = weights;
    weights[0] = 1.0;
    let expected = evaluate_convergence_batch(&values, Some(&weights)).unwrap();
    let mut streamed = ConvergenceStatistics::new();
    for (&value, &weight) in values.iter().zip(&weights) {
        streamed.add_weighted(value, weight).unwrap();
    }
    assert_eq!(streamed.convergence_table(), expected);
    let mut batched = ConvergenceStatistics::new();
    for (values, weights) in values.chunks(9).zip(weights.chunks(9)) {
        batched.add_batch(values, Some(weights)).unwrap();
    }
    assert_eq!(batched.convergence_table(), expected);
    assert_eq!(
        batched.mean().unwrap().to_bits(),
        streamed.mean().unwrap().to_bits()
    );
}

#[test]
fn reset_and_clone_preserve_independent_storage() {
    let mut statistics = ConvergenceStatistics::default();
    assert_eq!(statistics.samples(), 0);
    assert_eq!(statistics.weight_sum(), 0.0);
    assert!(statistics.mean().is_err());
    assert!(statistics.convergence_table().is_empty());
    statistics.add_batch(&[2.0, 4.0, 6.0], None).unwrap();
    let original = statistics.clone();
    statistics.reset();
    assert_eq!(statistics.samples(), 0);
    assert_eq!(statistics.weight_sum(), 0.0);
    assert!(statistics.mean().is_err());
    assert!(statistics.convergence_table().is_empty());
    statistics.add(9.0).unwrap();
    assert_eq!(
        statistics.table,
        [ConvergencePoint {
            samples: 1,
            mean: 9.0
        }]
    );
    assert_eq!(original.samples(), 3);
    close(original.mean().unwrap(), 4.0);
}

#[test]
fn invalid_sample_or_weight_additions_are_atomic() {
    let mut statistics = ConvergenceStatistics::new();
    statistics.add(2.0).unwrap();
    let previous = statistics.clone();
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(statistics.add(value).is_err());
        assert!(statistics.add_weighted(value, 0.0).is_err());
        unchanged(&statistics, &previous);
    }
    for weight in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(statistics.add_weighted(3.0, weight).is_err());
        unchanged(&statistics, &previous);
    }
}

#[test]
fn invalid_batch_rolls_back_earlier_samples_and_checkpoints() {
    let mut statistics = ConvergenceStatistics::new();
    statistics.add_batch(&[1.0, 2.0], None).unwrap();
    let previous = statistics.clone();
    assert!(statistics.add_batch(&[3.0, 4.0, f64::NAN], None).is_err());
    unchanged(&statistics, &previous);
    assert!(
        statistics
            .add_batch(&[3.0, 4.0, 5.0], Some(&[1.0, 1.0, -1.0]))
            .is_err()
    );
    unchanged(&statistics, &previous);
    assert!(statistics.add_batch(&[3.0], Some(&[])).is_err());
    unchanged(&statistics, &previous);
    assert!(statistics.add_batch(&[], Some(&[1.0])).is_err());
    unchanged(&statistics, &previous);
    statistics.add_batch(&[], Some(&[])).unwrap();
    unchanged(&statistics, &previous);
}

#[test]
fn zero_total_first_checkpoint_rejects_whole_batch() {
    let mut statistics = ConvergenceStatistics::new();
    let previous = statistics.clone();
    assert!(statistics.add_weighted(2.0, 0.0).is_err());
    unchanged(&statistics, &previous);
    assert!(
        statistics
            .add_batch(&[2.0, 4.0, 6.0], Some(&[0.0, 1.0, 1.0]))
            .is_err()
    );
    unchanged(&statistics, &previous);
    assert!(evaluate_convergence_batch(&[2.0, 4.0], Some(&[0.0, 1.0])).is_err());
    statistics.add_weighted(2.0, 1.0).unwrap();
    statistics.add_weighted(100.0, -0.0).unwrap();
    statistics.add_weighted(200.0, 0.0).unwrap();
    close(statistics.table[1].mean, 2.0);
}

#[test]
fn empty_batch_is_allowed_and_has_no_checkpoint() {
    assert!(evaluate_convergence_batch(&[], None).unwrap().is_empty());
    assert!(
        evaluate_convergence_batch(&[], Some(&[]))
            .unwrap()
            .is_empty()
    );
    assert!(evaluate_convergence_batch(&[], Some(&[1.0])).is_err());
}

#[test]
fn overflowing_weight_sum_is_atomic() {
    let mut statistics = ConvergenceStatistics::new();
    statistics.add_weighted(3.0, f64::MAX).unwrap();
    let previous = statistics.clone();
    assert!(statistics.add_weighted(4.0, f64::MAX).is_err());
    unchanged(&statistics, &previous);
    assert!(
        statistics
            .add_batch(&[4.0, 5.0], Some(&[0.0, f64::MAX]))
            .is_err()
    );
    unchanged(&statistics, &previous);
    close(statistics.mean().unwrap(), 3.0);
}

#[test]
fn sample_cap_allows_maximum_without_quadratic_streaming_work() {
    let mut statistics = ConvergenceStatistics::new();
    for _ in 0..MAX_CONVERGENCE_SAMPLES {
        statistics.add(2.0).unwrap();
    }
    assert_eq!(statistics.samples(), MAX_CONVERGENCE_SAMPLES);
    assert_eq!(statistics.table.len(), 16);
    assert_eq!(statistics.table.last().unwrap().samples, 65_535);
    close(statistics.mean().unwrap(), 2.0);
    let previous = statistics.clone();
    assert!(statistics.add(3.0).is_err());
    unchanged(&statistics, &previous);
    assert!(statistics.add_batch(&[3.0, 4.0], None).is_err());
    unchanged(&statistics, &previous);
    assert!(evaluate_convergence_batch(&vec![1.0; MAX_CONVERGENCE_SAMPLES + 1], None).is_err());
}

#[test]
fn compensated_cancellation_mean_is_stable_for_all_permutations() {
    for values in [
        [1e16, -1e16, 1.0],
        [1e16, 1.0, -1e16],
        [-1e16, 1e16, 1.0],
        [-1e16, 1.0, 1e16],
        [1.0, 1e16, -1e16],
        [1.0, -1e16, 1e16],
    ] {
        let table = evaluate_convergence_batch(&values, None).unwrap();
        close(table[1].mean, 1.0 / 3.0);
    }
}

#[test]
fn extreme_weight_ratios_and_large_offsets_reuse_stable_sequence_mean() {
    let table = evaluate_convergence_batch(&[1e16, 1.0, 1.0], Some(&[1e-16, 1.0, 1.0])).unwrap();
    close(table[1].mean, 1.5);
    let table =
        evaluate_convergence_batch(&[1e300, 0.0, 0.0], Some(&[1e-300, 1e300, 0.0])).unwrap();
    assert!((table[1].mean / 1e-300 - 1.0).abs() < 2e-15);
    let table = evaluate_convergence_batch(&[1e12, 1e12 + 1.0, 1e12 + 2.0], None).unwrap();
    assert_eq!(table[1].mean, 1e12 + 1.0);
    let table = evaluate_convergence_batch(&[f64::MAX, -f64::MAX, 0.0], None).unwrap();
    assert_eq!(table[1].mean, 0.0);
}
