use libitofin::math::array::Array;
use libitofin::math::matrixutilities::{
    HouseholderReflection, HouseholderTransformation, householder_transformation,
};

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual:.17e} != {expected:.17e}"
    );
}

#[test]
fn matches_compiled_original_quantlib_branches() {
    for &(e, a, v, y) in ORACLES {
        let reflection = HouseholderReflection::new(e.into()).unwrap();
        let actual_v = reflection.reflection_vector(&a.into()).unwrap();
        let actual_y = reflection.apply(&a.into()).unwrap();
        for i in 0..3 {
            close(actual_v[i], v[i], 2e-15);
            close(actual_y[i], y[i], 4e-15);
        }
    }
}

#[test]
fn transformation_preserves_native_nonunit_application_vs_matrix_difference() {
    let transformation = HouseholderTransformation::new([2.0, 0.0].into()).unwrap();
    let x = Array::from([3.0, 4.0]);
    assert_eq!(transformation.apply(&x).unwrap(), Array::from([-21.0, 4.0]));
    assert_eq!(
        &transformation.matrix().unwrap() * &x,
        Array::from([-3.0, 4.0])
    );
    for scale in [1e-300, 1e300] {
        let transformation = HouseholderTransformation::new([scale, scale].into()).unwrap();
        let matrix = transformation.matrix().unwrap();
        close(matrix[(0, 0)], 0.0, 3e-16);
        close(matrix[(0, 1)], -1.0, 3e-16);
    }
}

#[test]
fn matrix_is_symmetric_orthogonal_and_reflection_is_an_involution() {
    for &(e, a, _, _) in ORACLES {
        let matrix = householder_transformation(e.into(), &a.into()).unwrap();
        let square = &matrix * &matrix;
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(matrix[(i, j)], matrix[(j, i)]);
                close(square[(i, j)], if i == j { 1.0 } else { 0.0 }, 1e-15);
            }
        }
        let twice = &matrix * &(&matrix * &Array::from(a));
        for i in 0..3 {
            close(twice[i], a[i], 4e-15);
        }
    }
}

#[test]
fn large_small_and_mixed_scales_preserve_direction_and_native_sign() {
    let reflection = HouseholderReflection::new([1.0, 0.0, 0.0].into()).unwrap();
    for scale in [1e-300, 1.0, 1e300] {
        let a = Array::from([3.0 * scale, 4.0 * scale, 0.0]);
        let y = reflection.apply(&a).unwrap();
        close(y[0] / scale, 5.0, 2e-15);
        close(y[1] / scale, 0.0, 2e-15);
        assert_eq!(y[2], 0.0);
        let negative = Array::from([-scale, 1e-3 * scale, -2e-3 * scale]);
        let y = reflection.apply(&negative).unwrap();
        close(y[0] / scale, -1.000002499996875, 4e-16);
        close(y[1] / scale, 0.0, 4e-18);
        close(y[2] / scale, 0.0, 8e-18);
    }
    for a in [[1e300, 1e-300, 0.0], [-1e300, 1e-300, 0.0]] {
        assert_eq!(
            reflection.reflection_vector(&a.into()).unwrap(),
            Array::with_size(3)
        );
        assert_eq!(reflection.apply(&a.into()).unwrap(), Array::from(a));
    }
    let minimum = f64::from_bits(1);
    assert_eq!(
        reflection.apply(&[minimum, 0.0, 0.0].into()).unwrap(),
        Array::from([minimum, 0.0, 0.0])
    );
}

#[test]
fn zero_transformation_and_parallel_helper_are_checked_identity_extensions() {
    let transformation = HouseholderTransformation::new([0.0, -0.0].into()).unwrap();
    let x = Array::from([1.0, -2.0]);
    assert_eq!(transformation.apply(&x).unwrap(), x);
    assert_eq!(&transformation.matrix().unwrap() * &x, x);
    for target in [[2.0, 0.0], [-2.0, 0.0]] {
        let matrix = householder_transformation([1.0, 0.0].into(), &target.into()).unwrap();
        assert_eq!(&matrix * &x, x);
    }
}

#[test]
fn rejects_invalid_direction_dimension_nonfinite_and_unrepresentable_outputs() {
    for e in [
        vec![],
        vec![0.0],
        vec![2.0],
        vec![f64::NAN],
        vec![f64::INFINITY],
    ] {
        assert!(HouseholderReflection::new(e.into()).is_err());
    }
    for v in [vec![], vec![f64::NAN], vec![f64::NEG_INFINITY]] {
        assert!(HouseholderTransformation::new(v.into()).is_err());
    }
    let reflection = HouseholderReflection::new([1.0, 0.0].into()).unwrap();
    let transformation = HouseholderTransformation::new([1.0, 0.0].into()).unwrap();
    for a in [
        vec![],
        vec![1.0],
        vec![f64::NAN, 0.0],
        vec![0.0, f64::INFINITY],
    ] {
        assert!(reflection.apply(&a.clone().into()).is_err());
        assert!(transformation.apply(&a.into()).is_err());
    }
    assert!(reflection.reflection_vector(&[0.0, 0.0].into()).is_err());
    assert!(reflection.apply(&[f64::MAX, f64::MAX].into()).is_err());
    let huge = HouseholderTransformation::new([f64::MAX, 0.0].into()).unwrap();
    assert!(huge.apply(&[1.0, 0.0].into()).is_err());
    assert_eq!(
        huge.apply(&[0.0, 1.0].into()).unwrap(),
        Array::from([0.0, 1.0])
    );
    let nonunit = HouseholderTransformation::new([2.0, 0.0].into()).unwrap();
    assert!(nonunit.apply(&[f64::MAX, 0.0].into()).is_err());
}
type Oracle = ([f64; 3], [f64; 3], [f64; 3], [f64; 3]);

const ORACLES: &[Oracle] = &[
    (
        [1.0, 0.0, 0.0],
        [3.0, 4.0, 0.0],
        [-0.4472135954999579, 0.8944271909999159, 0.0],
        [5.0, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [-3.0, 4.0, 0.0],
        [-0.8944271909999159, 0.4472135954999579, 0.0],
        [5.0, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [0.0, 4.0, 3.0],
        [-0.7071067811865475, 0.565685424949238, 0.4242640687119285],
        [5.0, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [3.0, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [-3.0, 0.0, 0.0],
        [0.0, 0.0, 0.0],
        [-3.0, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [1.0, 0.001, -0.002],
        [
            -0.0011180318924429355,
            0.4472133159924216,
            -0.8944266319848432,
        ],
        [1.000002499996875, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [-1.0, 0.001, -0.002],
        [
            -0.0011180318924429355,
            -0.4472133159924216,
            0.8944266319848432,
        ],
        [-1.000002499996875, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [1.0, 1e-12, 0.0],
        [-5e-13, 1.0, 0.0],
        [1.0, 0.0, 0.0],
    ),
    (
        [1.0, 0.0, 0.0],
        [-1.0, 1e-12, 0.0],
        [-5e-13, -1.0, -0.0],
        [-1.0, 0.0, 0.0],
    ),
    (
        [0.6, 0.8, 0.0],
        [1.0, 2.0, 3.0],
        [
            -0.36654429830551444,
            -0.2924494592742942,
            0.8832432230987628,
        ],
        [2.244994432064365, 2.993325909419153, 8.881784197001252e-16],
    ),
    (
        [0.0, 0.0, -1.0],
        [0.1, 0.2, 0.3],
        [0.14078930153343572, 0.28157860306687144, 0.9491532346616307],
        [0.0, 0.0, -0.37416573867739417],
    ),
];
