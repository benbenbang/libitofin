use libitofin::math::matrix::{Matrix, det3, inverse_3x3};

type Matrix3 = [[f64; 3]; 3];

fn assert_identity(matrix: &Matrix, tolerance: f64) {
    for i in 0..3 {
        for j in 0..3 {
            let expected = f64::from(i == j);
            assert!((matrix[(i, j)] - expected).abs() <= tolerance);
        }
    }
}

#[test]
fn checked_matrix_helpers_match_independent_pivot_oracle() {
    let matrix = Matrix::from([[0.0, 2.0, 1.0], [1.0, 0.0, 3.0], [4.0, 1.0, 8.0]]);
    let inverse = inverse_3x3(&matrix).unwrap();
    let expected = Matrix::from([
        [-1.0 / 3.0, -5.0 / 3.0, 2.0 / 3.0],
        [4.0 / 9.0, -4.0 / 9.0, 1.0 / 9.0],
        [1.0 / 9.0, 8.0 / 9.0, -2.0 / 9.0],
    ]);
    assert!((det3(&matrix).unwrap() - 9.0).abs() < 1e-13);
    for i in 0..3 {
        for j in 0..3 {
            assert!((inverse[(i, j)] - expected[(i, j)]).abs() < 1e-14);
        }
    }
    assert_identity(&(&matrix * &inverse), 2e-14);
    assert_identity(&(&inverse * &matrix), 2e-14);
}

#[test]
fn inverse_does_not_require_representable_determinant() {
    for scale in [1e-200, 1e200] {
        let matrix = Matrix::from([[scale, 0.0, 0.0], [0.0, scale, 0.0], [0.0, 0.0, scale]]);
        let inverse = inverse_3x3(&matrix).unwrap();
        assert_identity(&(&matrix * &inverse), 1e-15);
        assert_identity(&(&inverse * &matrix), 1e-15);
        assert!(det3(&matrix).is_err());
    }
}

#[test]
fn determinant_preserves_representable_mixed_scale_products() {
    for diagonal in [
        [1e-200, 1e200, 1e200],
        [1e200, 1e-200, 1.0],
        [1e-200, 1e-200, 1e200],
    ] {
        let matrix = Matrix::from([
            [diagonal[0], 0.0, 0.0],
            [0.0, diagonal[1], 0.0],
            [0.0, 0.0, diagonal[2]],
        ]);
        let expected = if diagonal[2] == 1.0 { 1.0 } else { diagonal[0] };
        let expected = if diagonal[0] == 1e-200 && diagonal[1] == 1e200 {
            1e200
        } else {
            expected
        };
        assert!((det3(&matrix).unwrap() / expected - 1.0).abs() < 1e-14);
        assert_identity(&(&matrix * &inverse_3x3(&matrix).unwrap()), 1e-15);
    }
}

#[test]
fn wrong_shapes_and_nonfinite_inputs_return_errors() {
    for shape in [(0, 0), (2, 3), (3, 2), (4, 4)] {
        let matrix = Matrix::with_size(shape.0, shape.1);
        assert!(det3(&matrix).is_err());
        assert!(inverse_3x3(&matrix).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let matrix = Matrix::from([[1.0, bad, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        assert!(det3(&matrix).is_err());
        assert!(inverse_3x3(&matrix).is_err());
    }
}

#[test]
fn singular_unresolved_and_unrepresentable_inverse_are_rejected() {
    let singular = Matrix::from([[1.0, 2.0, 3.0], [1.0, 2.0, 3.0], [0.0, 1.0, 0.0]]);
    assert_eq!(det3(&singular).unwrap(), 0.0);
    assert!(inverse_3x3(&singular).is_err());
    let tiny = Matrix::from([
        [f64::from_bits(1), 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
    ]);
    assert_eq!(det3(&tiny).unwrap(), f64::from_bits(1));
    assert!(inverse_3x3(&tiny).is_err());
    let unresolved = Matrix::from([[1e308, 1e-308, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    assert!(det3(&unresolved).is_err());
    assert!(inverse_3x3(&unresolved).is_err());
}

#[test]
fn inverse_matches_compiled_quantlib_fixture_family() {
    let cases: [(Matrix3, Matrix3); 8] = [
        (
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]],
        ),
        (
            [[0.0, 2.0, 1.0], [1.0, 0.0, 3.0], [4.0, 1.0, 8.0]],
            [
                [-0.3333333333333333, -1.6666666666666665, 0.6666666666666666],
                [0.4444444444444444, -0.4444444444444444, 0.1111111111111111],
                [0.1111111111111111, 0.8888888888888888, -0.2222222222222222],
            ],
        ),
        (
            [[4.0, 1.0, 2.0], [0.0, 3.0, -1.0], [2.0, 0.0, 5.0]],
            [
                [
                    0.32608695652173914,
                    -0.10869565217391304,
                    -0.15217391304347827,
                ],
                [
                    -0.043478260869565216,
                    0.34782608695652173,
                    0.08695652173913043,
                ],
                [
                    -0.13043478260869565,
                    0.043478260869565216,
                    0.2608695652173913,
                ],
            ],
        ),
        (
            [[1e+200, 0.0, 0.0], [0.0, 1e+200, 0.0], [0.0, 0.0, 1e+200]],
            [[1e-200, 0.0, 0.0], [0.0, 1e-200, 0.0], [0.0, 0.0, 1e-200]],
        ),
        (
            [[1e-200, 0.0, 0.0], [0.0, 1e-200, 0.0], [0.0, 0.0, 1e-200]],
            [[1e+200, 0.0, 0.0], [0.0, 1e+200, 0.0], [0.0, 0.0, 1e+200]],
        ),
        (
            [[1e-200, 0.0, 0.0], [0.0, 1e+200, 0.0], [0.0, 0.0, 1.0]],
            [[1e+200, 0.0, 0.0], [0.0, 1e-200, 0.0], [0.0, 0.0, 1.0]],
        ),
        (
            [[1e-300, 0.0, 0.0], [0.0, 1e+300, 0.0], [0.0, 0.0, 1.0]],
            [
                [9.999999999999999e+299, 0.0, 0.0],
                [0.0, 1e-300, 0.0],
                [0.0, 0.0, 1.0],
            ],
        ),
        (
            [[0.0, 1e+200, 0.0], [1e-200, 0.0, 0.0], [0.0, 0.0, 1.0]],
            [[0.0, 1e+200, 0.0], [1e-200, 0.0, 0.0], [0.0, 0.0, 1.0]],
        ),
    ];
    for (input, expected) in cases {
        let matrix = Matrix::from(input);
        let actual = inverse_3x3(&matrix).unwrap();
        assert_identity(&(&matrix * &actual), 2e-14);
        assert_identity(&(&actual * &matrix), 2e-14);
        for i in 0..3 {
            for j in 0..3 {
                if expected[i][j] == 0.0 {
                    assert!(actual[(i, j)].abs() < 1e-14);
                } else {
                    assert!((actual[(i, j)] / expected[i][j] - 1.0).abs() < 2e-14);
                }
            }
        }
    }
}

#[test]
fn nonduplicate_singular_rows_do_not_produce_a_false_inverse() {
    let matrix = Matrix::from([[1.0, 2.0, 3.0], [4.0, 5.0, 6.0], [5.0, 7.0, 9.0]]);
    assert!(inverse_3x3(&matrix).is_err());
}
