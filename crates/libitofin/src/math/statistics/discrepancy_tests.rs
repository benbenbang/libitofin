use super::*;

fn close(actual: Real, expected: Real) {
    assert!(
        (actual - expected).abs() <= 2.0e-14 * expected.abs().max(1.0),
        "actual={actual:.17e}, expected={expected:.17e}"
    );
}

fn assert_unchanged(actual: &DiscrepancyStatistics, expected: &DiscrepancyStatistics) {
    assert_eq!(actual.dimension, expected.dimension);
    assert_eq!(actual.values, expected.values);
    assert_eq!(
        actual.pair_sum.sum.to_bits(),
        expected.pair_sum.sum.to_bits()
    );
    assert_eq!(
        actual.pair_sum.correction.to_bits(),
        expected.pair_sum.correction.to_bits()
    );
    assert_eq!(
        actual.coordinate_sum.sum.to_bits(),
        expected.coordinate_sum.sum.to_bits()
    );
    assert_eq!(
        actual.coordinate_sum.correction.to_bits(),
        expected.coordinate_sum.correction.to_bits()
    );
}

#[test]
fn discrepancy_matches_single_point_hand_formulas() {
    for (point, squared) in [
        ([0.0, 0.0], 11.0_f64 / 18.0),
        ([1.0, 1.0], 1.0 / 9.0),
        ([0.5, 0.5], 23.0 / 288.0),
        ([0.0, 1.0], 1.0 / 9.0),
    ] {
        close(
            evaluate_discrepancy_batch(&point, 1, 2, None).unwrap(),
            squared.sqrt(),
        );
    }
    close(
        evaluate_discrepancy_batch(&[0.5, 0.5, 0.5], 1, 3, None).unwrap(),
        (1.0_f64 / 8.0 - 27.0 / 256.0 + 1.0 / 27.0).sqrt(),
    );
}

#[test]
fn discrepancy_accepts_closed_cube_boundaries_and_signed_zero() {
    close(
        evaluate_discrepancy_batch(&[0.0, 0.0, 1.0, 1.0], 2, 2, None).unwrap(),
        1.0 / 3.0,
    );
    let signed = evaluate_discrepancy_batch(&[-0.0, 0.0], 1, 2, None).unwrap();
    assert_eq!(
        signed.to_bits(),
        evaluate_discrepancy_batch(&[0.0, 0.0], 1, 2, None)
            .unwrap()
            .to_bits()
    );
}

#[test]
fn discrepancy_duplicate_replication_preserves_the_empirical_measure() {
    let points = [0.125, 0.875, 0.25, 0.5, 0.75, 0.125];
    let expected = evaluate_discrepancy_batch(&points, 3, 2, None).unwrap();
    let replicated = points.repeat(4);
    close(
        evaluate_discrepancy_batch(&replicated, 12, 2, None).unwrap(),
        expected,
    );
}

#[test]
fn discrepancy_matches_independent_exact_dyadic_formula() {
    let points = [0.125, 0.875, 0.25, 0.5, 0.75, 0.125];
    let count = 3.0;
    let mut pairs = 0_u64;
    let mut coordinates = 0_u64;
    let integer_points = [[1_u64, 7], [2, 4], [6, 1]];
    for left in integer_points {
        coordinates += (64 - left[0] * left[0]) * (64 - left[1] * left[1]);
        for right in integer_points {
            pairs += (8 - left[0].max(right[0])) * (8 - left[1].max(right[1]));
        }
    }
    let squared = pairs as Real / 64.0 / count / count - coordinates as Real / 4096.0 / 2.0 / count
        + 1.0 / 9.0;
    close(
        evaluate_discrepancy_batch(&points, 3, 2, None).unwrap(),
        squared.sqrt(),
    );
}

#[test]
fn discrepancy_is_invariant_under_row_and_coordinate_permutations() {
    let rows = [[0.125, 0.875], [0.25, 0.5], [0.75, 0.125]];
    let expected = evaluate_discrepancy_batch(&rows.concat(), 3, 2, None).unwrap();
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        for swap in [false, true] {
            let mut values = Vec::new();
            for index in order {
                let mut point = rows[index];
                if swap {
                    point.swap(0, 1);
                }
                values.extend_from_slice(&point);
            }
            close(
                evaluate_discrepancy_batch(&values, 3, 2, None).unwrap(),
                expected,
            );
        }
    }
}

#[test]
fn discrepancy_incremental_prefixes_match_batch_and_clone_is_independent() {
    let rows = [[0.125, 0.875], [0.25, 0.5], [0.75, 0.125]];
    let mut statistics = DiscrepancyStatistics::new(2).unwrap();
    assert_eq!(statistics.dimension(), 2);
    assert_eq!(statistics.samples(), 0);
    assert!(statistics.discrepancy().is_err());
    for (index, point) in rows.iter().enumerate() {
        statistics.add(point).unwrap();
        assert_eq!(statistics.samples(), index + 1);
        let flat = rows[..=index].concat();
        assert_eq!(
            statistics.discrepancy().unwrap().to_bits(),
            evaluate_discrepancy_batch(&flat, index + 1, 2, None)
                .unwrap()
                .to_bits()
        );
    }
    let original = statistics.clone();
    statistics.add(&[0.0, 1.0]).unwrap();
    assert_eq!(original.samples(), 3);
    assert_eq!(statistics.samples(), 4);
}

#[test]
fn discrepancy_accepts_only_exact_unit_weights() {
    let points = [0.125, 0.875, 0.25, 0.5];
    let unit = evaluate_discrepancy_batch(&points, 2, 2, Some(&[1.0, 1.0])).unwrap();
    assert_eq!(
        unit.to_bits(),
        evaluate_discrepancy_batch(&points, 2, 2, None)
            .unwrap()
            .to_bits()
    );
    for weight in [
        0.0,
        -0.0,
        -1.0,
        2.0,
        Real::NAN,
        Real::INFINITY,
        Real::NEG_INFINITY,
        Real::from_bits(1.0_f64.to_bits() + 1),
    ] {
        assert!(evaluate_discrepancy_batch(&points, 2, 2, Some(&[1.0, weight])).is_err());
        let mut statistics = DiscrepancyStatistics::new(2).unwrap();
        statistics.add(&[0.5, 0.5]).unwrap();
        let before = statistics.clone();
        assert!(statistics.add_weighted(&[0.125, 0.875], weight).is_err());
        assert_unchanged(&statistics, &before);
    }
    assert!(evaluate_discrepancy_batch(&points, 2, 2, Some(&[])).is_err());
    assert!(evaluate_discrepancy_batch(&points, 2, 2, Some(&[1.0])).is_err());
    assert!(evaluate_discrepancy_batch(&points, 2, 2, Some(&[1.0, 1.0, 1.0])).is_err());
}

#[test]
fn discrepancy_rejects_invalid_shapes_before_reading_values() {
    for (rows, dimension) in [
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 257),
        (4097, 2),
        (4096, 6),
        (626, 256),
        (usize::MAX, 2),
        (2, usize::MAX),
        (usize::MAX, usize::MAX),
    ] {
        assert!(validate_discrepancy_shape(rows, dimension).is_err());
        assert!(evaluate_discrepancy_batch(&[], rows, dimension, None).is_err());
    }
    assert_eq!(validate_discrepancy_shape(4096, 5).unwrap(), 20480);
    assert_eq!(validate_discrepancy_shape(625, 256).unwrap(), 160000);
    for values in [&[][..], &[0.0][..], &[0.0, 0.0, 0.0][..]] {
        assert!(evaluate_discrepancy_batch(values, 1, 2, None).is_err());
    }
}

#[test]
fn discrepancy_invalid_coordinates_and_dimensions_do_not_change_state() {
    let mut statistics = DiscrepancyStatistics::new(2).unwrap();
    statistics.add(&[0.25, 0.75]).unwrap();
    let before = statistics.clone();
    for value in [
        -Real::EPSILON,
        1.0 + Real::EPSILON,
        Real::NAN,
        Real::INFINITY,
        Real::NEG_INFINITY,
    ] {
        assert!(statistics.add(&[0.5, value]).is_err());
        assert_unchanged(&statistics, &before);
        assert!(evaluate_discrepancy_batch(&[0.5, value], 1, 2, None).is_err());
    }
    for sample in [&[][..], &[0.5][..], &[0.5, 0.5, 0.5][..]] {
        assert!(statistics.add(sample).is_err());
        assert_unchanged(&statistics, &before);
    }
    for dimension in [1, 257, usize::MAX] {
        assert!(statistics.reset(dimension).is_err());
        assert_unchanged(&statistics, &before);
        assert!(DiscrepancyStatistics::new(dimension).is_err());
    }
    assert!(DiscrepancyStatistics::new(0).is_err());
}

#[test]
fn discrepancy_reset_zero_keeps_dimension_and_other_valid_sizes_replace_it() {
    let mut statistics = DiscrepancyStatistics::new(2).unwrap();
    statistics.add(&[0.5, 0.5]).unwrap();
    statistics.reset(0).unwrap();
    assert_eq!(statistics.dimension(), 2);
    assert_eq!(statistics.samples(), 0);
    assert!(statistics.discrepancy().is_err());
    statistics.reset(3).unwrap();
    statistics.add(&[1.0, 1.0, 1.0]).unwrap();
    assert_eq!(statistics.dimension(), 3);
    close(statistics.discrepancy().unwrap(), (1.0_f64 / 27.0).sqrt());
}

#[test]
fn discrepancy_candidate_work_and_row_limits_are_atomic() {
    for (rows, dimension) in [(4096, 2), (625, 256)] {
        let mut statistics = DiscrepancyStatistics::new(dimension).unwrap();
        statistics.values = vec![1.0; rows * dimension];
        let before = statistics.clone();
        assert!(statistics.add(&vec![1.0; dimension]).is_err());
        assert_unchanged(&statistics, &before);
    }
    let mut statistics = DiscrepancyStatistics::new(256).unwrap();
    statistics.values = vec![1.0; 624 * 256];
    statistics.add(&vec![1.0; 256]).unwrap();
    assert_eq!(statistics.samples(), 625);
    close(statistics.discrepancy().unwrap(), 3.0_f64.powi(-128));
}

#[test]
fn discrepancy_clamps_only_bounded_negative_roundoff() {
    assert_eq!(
        finish_discrepancy(1.0, 1.0 + 4.0 * Real::EPSILON, 0.0, 2).unwrap(),
        0.0
    );
    assert!(finish_discrepancy(1.0, 1.001, 0.0, 2).is_err());
    assert!(finish_discrepancy(Real::NAN, 0.0, 0.0, 2).is_err());
    assert!(finish_discrepancy(Real::INFINITY, 0.0, 0.0, 2).is_err());
}

#[test]
fn discrepancy_max_dimension_remains_finite_with_product_underflow() {
    let points = vec![1.0 - 1.0e-12; 256];
    let result = evaluate_discrepancy_batch(&points, 1, 256, None).unwrap();
    assert!(result.is_finite() && result > 0.0);
    close(result, 3.0_f64.powi(-128));
}
