use libitofin::math::abcdmathfunction::{AbcdMathFunction, validate};
use libitofin::termstructures::volatility::{AbcdFunction, AbcdSquared};

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(actual.is_finite());
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual={actual:.17e}, expected={expected:.17e}"
    );
}

#[test]
fn distinct_native_defaults_and_coefficient_inspectors() {
    let math = AbcdMathFunction::with_defaults().unwrap();
    let vol = AbcdFunction::with_defaults().unwrap();
    assert_eq!(math.coefficients(), &[0.002, 0.001, 0.16, 0.0005]);
    assert_eq!(vol.coefficients(), &[-0.06, 0.17, 0.54, 0.17]);
    assert_eq!(
        math.coefficients(),
        AbcdMathFunction::default().coefficients()
    );
    assert_eq!(vol.coefficients(), AbcdFunction::default().coefficients());
    assert_eq!(
        [math.a(), math.b(), math.c(), math.d()],
        *math.coefficients()
    );
    assert_eq!([vol.a(), vol.b(), vol.c(), vol.d()], *vol.coefficients());
}

#[test]
fn compiled_native_values_and_instantaneous_covariances() {
    for &[a, b, c, d, t, expected] in NATIVE_VALUES {
        close(
            AbcdMathFunction::new(a, b, c, d).unwrap().value(t).unwrap(),
            expected,
            1e-15,
        );
    }
    for &[a, b, c, d, u, t, s, expected] in NATIVE_INSTANTANEOUS {
        let f = AbcdFunction::new(a, b, c, d).unwrap();
        close(
            f.instantaneous_covariance(u, t, s).unwrap(),
            expected,
            1e-15,
        );
        close(
            AbcdSquared::new(a, b, c, d, t, s)
                .unwrap()
                .value(u)
                .unwrap(),
            expected,
            1e-15,
        );
    }
}

#[test]
fn compiled_native_integrated_covariance_and_variance() {
    for &[a, b, c, d, lo, hi, t, s, expected] in NATIVE_COVARIANCES {
        let f = AbcdFunction::new(a, b, c, d).unwrap();
        close(f.covariance(lo, hi, t, s).unwrap(), expected, 2e-14);
        assert_eq!(
            f.covariance(lo, hi, t, s).unwrap(),
            f.covariance(lo, hi, s, t).unwrap()
        );
        if t == s {
            assert_eq!(
                f.variance(lo, hi, t).unwrap(),
                f.covariance(lo, hi, t, s).unwrap()
            );
        }
    }
}

#[test]
fn cutoff_zero_intervals_additivity_and_signed_time_translation() {
    let f = AbcdFunction::default();
    assert_eq!(f.covariance(2.0, 5.0, 2.0, 3.0).unwrap(), 0.0);
    assert_eq!(f.covariance(0.5, 0.5, 2.0, 3.0).unwrap(), 0.0);
    assert!(f.instantaneous_covariance(2.0, 2.0, 3.0).unwrap() > 0.0);
    assert_eq!(f.instantaneous_covariance(2.01, 2.0, 3.0).unwrap(), 0.0);
    close(
        f.covariance(-1.0, 2.5, 2.0, 3.0).unwrap(),
        f.covariance(-1.0, 0.7, 2.0, 3.0).unwrap() + f.covariance(0.7, 2.5, 2.0, 3.0).unwrap(),
        2e-15,
    );
    close(
        f.covariance(-10.0, -9.0, -8.0, -7.0).unwrap(),
        f.covariance(0.0, 1.0, 2.0, 3.0).unwrap(),
        1e-15,
    );
}

#[test]
fn small_decay_and_long_maturity_remain_finite() {
    for &[c, expected] in HIGH_PRECISION_SMALL_DECAY {
        let f = AbcdFunction::new(0.2, 0.1, c, 0.3).unwrap();
        close(f.covariance(0.0, 1.0, 2.0, 3.0).unwrap(), expected, 2e-15);
    }
    let f = AbcdFunction::default();
    close(
        f.covariance(0.0, 1.0, 2000.0, 2001.0).unwrap(),
        0.17 * 0.17,
        1e-15,
    );
    let constant = AbcdFunction::new(0.0, 0.0, 1e-50, 0.2).unwrap();
    close(constant.variance(1.0, 3.0, 10.0).unwrap(), 0.08, 1e-15);
}

#[test]
fn near_zero_shapes_use_stable_shifted_values_and_integrals() {
    let tiny = AbcdFunction::new(-1.0, 0.0, 1e-12, 1.0).unwrap();
    close(tiny.value(1.0).unwrap(), 9.999999999995e-13, 3e-28);
    close(
        tiny.variance(0.0, 1.0, 1.0).unwrap(),
        3.3333333333308333e-25,
        2e-40,
    );
    close(
        tiny.variance(0.0, 0.1, 0.1).unwrap(),
        3.3333333333330835e-28,
        2e-43,
    );
    let short = AbcdFunction::new(-1.0, 0.0, 0.5, 1.0).unwrap();
    close(
        short.variance(0.0, 1e-8, 1e-8).unwrap(),
        8.333333302083334e-26,
        5e-41,
    );
    assert!(validate(1e300, -1e-20, 1e-320, 1.0).is_err());
}

#[test]
fn finite_coefficient_and_stationary_minimum_validation() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for i in 0..4 {
            let mut coeff = [0.2, 0.1, 0.5, 0.2];
            coeff[i] = bad;
            assert!(AbcdMathFunction::new(coeff[0], coeff[1], coeff[2], coeff[3]).is_err());
        }
    }
    for coeff in [
        [0.2, 0.1, 0.0, 0.2],
        [0.2, 0.1, -0.5, 0.2],
        [-0.3, 0.1, 0.5, 0.2],
        [0.2, -1.0, 0.5, 0.01],
        [0.2, -0.01, 0.5, 0.0],
    ] {
        assert!(validate(coeff[0], coeff[1], coeff[2], coeff[3]).is_err());
    }
    assert!(validate(0.2, -0.1, 0.5, 0.2).is_ok());
    assert!(validate(-0.1, -0.01, 0.5, 0.2).is_ok());
    assert!(validate(f64::MAX, 0.0, 1.0, f64::MAX).is_err());
}

#[test]
fn non_finite_and_reversed_queries_are_checked_even_after_cutoff() {
    let f = AbcdFunction::default();
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(f.value(bad).is_err());
        for i in 0..4 {
            let mut times = [5.0, 6.0, 2.0, 3.0];
            times[i] = bad;
            assert!(
                f.covariance(times[0], times[1], times[2], times[3])
                    .is_err()
            );
        }
        assert!(AbcdSquared::new(0.2, 0.1, 0.5, 0.2, bad, 1.0).is_err());
        assert!(f.instantaneous_covariance(bad, 1.0, 2.0).is_err());
    }
    assert!(f.covariance(2.0, 1.0, 4.0, 5.0).is_err());
    assert!(
        f.covariance(-f64::MAX, f64::MAX, f64::MAX, f64::MAX)
            .is_err()
    );
    assert!(
        f.instantaneous_covariance(-f64::MAX, f64::MAX, 1.0)
            .is_err()
    );
    let huge = AbcdFunction::new(1e200, 0.0, 0.5, 0.0).unwrap();
    assert!(huge.instantaneous_covariance(0.0, 0.0, 0.0).is_err());
    assert!(huge.variance(0.0, 1.0, 1.0).is_err());
}
const NATIVE_VALUES: &[[f64; 6]] = &[
    [0.002, 0.001, 0.16, 0.0005, -2.0, 0.0],
    [0.002, 0.001, 0.16, 0.0005, 0.0, 0.0025],
    [0.002, 0.001, 0.16, 0.0005, 0.1, 0.0025666673721160987],
    [0.002, 0.001, 0.16, 0.0005, 1.0, 0.003056431366898634],
    [0.002, 0.001, 0.16, 0.0005, 5.0, 0.003645302748820551],
    [0.002, 0.001, 0.16, 0.0005, 25.0, 0.0009945222499958227],
    [-0.06, 0.17, 0.54, 0.17, -2.0, 0.0],
    [-0.06, 0.17, 0.54, 0.17, 0.0, 0.11000000000000001],
    [-0.06, 0.17, 0.54, 0.17, 0.1, 0.12926041942042268],
    [-0.06, 0.17, 0.54, 0.17, 1.0, 0.23410230776113888],
    [-0.06, 0.17, 0.54, 0.17, 5.0, 0.2230923550644023],
    [-0.06, 0.17, 0.54, 0.17, 25.0, 0.17000574431857196],
    [0.2, -0.1, 0.5, 0.2, -2.0, 0.0],
    [0.2, -0.1, 0.5, 0.2, 0.0, 0.4],
    [0.2, -0.1, 0.5, 0.2, 0.1, 0.3807335906551357],
    [0.2, -0.1, 0.5, 0.2, 1.0, 0.26065306597126336],
    [0.2, -0.1, 0.5, 0.2, 5.0, 0.17537450041283037],
    [0.2, -0.1, 0.5, 0.2, 25.0, 0.19999142869770423],
    [-0.1, 0.0, 0.6, 0.1, -2.0, 0.0],
    [-0.1, 0.0, 0.6, 0.1, 0.0, 0.0],
    [-0.1, 0.0, 0.6, 0.1, 0.1, 0.005823546641575129],
    [-0.1, 0.0, 0.6, 0.1, 1.0, 0.04511883639059736],
    [-0.1, 0.0, 0.6, 0.1, 5.0, 0.09502129316321362],
    [-0.1, 0.0, 0.6, 0.1, 25.0, 0.09999996940976795],
    [0.0, 0.0, 0.7, 0.2, -2.0, 0.0],
    [0.0, 0.0, 0.7, 0.2, 0.0, 0.2],
    [0.0, 0.0, 0.7, 0.2, 0.1, 0.2],
    [0.0, 0.0, 0.7, 0.2, 1.0, 0.2],
    [0.0, 0.0, 0.7, 0.2, 5.0, 0.2],
    [0.0, 0.0, 0.7, 0.2, 25.0, 0.2],
];
const NATIVE_COVARIANCES: &[[f64; 9]] = &[
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        0.0,
        1.0,
        2.0,
        3.0,
        1.1400014625654086e-5,
    ],
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        0.5,
        2.5,
        2.0,
        3.0,
        1.4568638392034518e-5,
    ],
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        -1.0,
        0.5,
        1.0,
        2.0,
        1.6294790811793132e-5,
    ],
    [0.002, 0.001, 0.16, 0.0005, 1.0, 1.0, 2.0, 3.0, 0.0],
    [0.002, 0.001, 0.16, 0.0005, 3.0, 5.0, 2.0, 3.0, 0.0],
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        0.0,
        3.0,
        2.0,
        2.0,
        1.8398516241042362e-5,
    ],
    [
        -0.06,
        0.17,
        0.54,
        0.17,
        0.0,
        1.0,
        2.0,
        3.0,
        0.06708480471382229,
    ],
    [
        -0.06,
        0.17,
        0.54,
        0.17,
        0.5,
        2.5,
        2.0,
        3.0,
        0.07974461079505814,
    ],
    [
        -0.06,
        0.17,
        0.54,
        0.17,
        -1.0,
        0.5,
        1.0,
        2.0,
        0.09510236496877894,
    ],
    [-0.06, 0.17, 0.54, 0.17, 1.0, 1.0, 2.0, 3.0, 0.0],
    [-0.06, 0.17, 0.54, 0.17, 3.0, 5.0, 2.0, 3.0, 0.0],
    [
        -0.06,
        0.17,
        0.54,
        0.17,
        0.0,
        3.0,
        2.0,
        2.0,
        0.09970718112371753,
    ],
    [
        0.2,
        -0.1,
        0.5,
        0.2,
        0.0,
        1.0,
        2.0,
        3.0,
        0.042283144472073604,
    ],
    [0.2, -0.1, 0.5, 0.2, 0.5, 2.5, 2.0, 3.0, 0.09642818560246019],
    [
        0.2,
        -0.1,
        0.5,
        0.2,
        -1.0,
        0.5,
        1.0,
        2.0,
        0.07258451153570684,
    ],
    [0.2, -0.1, 0.5, 0.2, 1.0, 1.0, 2.0, 3.0, 0.0],
    [0.2, -0.1, 0.5, 0.2, 3.0, 5.0, 2.0, 3.0, 0.0],
    [0.2, -0.1, 0.5, 0.2, 0.0, 3.0, 2.0, 2.0, 0.15615400492269854],
    [-0.1, 0.0, 0.6, 0.1, 0.0, 1.0, 2.0, 3.0, 0.00457071936757369],
    [
        -0.1,
        0.0,
        0.6,
        0.1,
        0.5,
        2.5,
        2.0,
        3.0,
        0.0034989170866218254,
    ],
    [
        -0.1,
        0.0,
        0.6,
        0.1,
        -1.0,
        0.5,
        1.0,
        2.0,
        0.005746813191394008,
    ],
    [-0.1, 0.0, 0.6, 0.1, 1.0, 1.0, 2.0, 3.0, 0.0],
    [-0.1, 0.0, 0.6, 0.1, 3.0, 5.0, 2.0, 3.0, 0.0],
    [
        -0.1,
        0.0,
        0.6,
        0.1,
        0.0,
        3.0,
        2.0,
        2.0,
        0.004283824119661631,
    ],
    [0.0, 0.0, 0.7, 0.2, 0.0, 1.0, 2.0, 3.0, 0.039999999999999994],
    [0.0, 0.0, 0.7, 0.2, 0.5, 2.5, 2.0, 3.0, 0.05999999999999999],
    [0.0, 0.0, 0.7, 0.2, -1.0, 0.5, 1.0, 2.0, 0.06],
    [0.0, 0.0, 0.7, 0.2, 1.0, 1.0, 2.0, 3.0, 0.0],
    [0.0, 0.0, 0.7, 0.2, 3.0, 5.0, 2.0, 3.0, 0.0],
    [0.0, 0.0, 0.7, 0.2, 0.0, 3.0, 2.0, 2.0, 0.08],
];
const NATIVE_INSTANTANEOUS: &[[f64; 8]] = &[
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        0.0,
        1.0,
        2.0,
        1.040591445927039e-5,
    ],
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        1.0,
        1.0,
        2.0,
        7.641078417246585e-6,
    ],
    [0.002, 0.001, 0.16, 0.0005, 2.0, 1.0, 2.0, 0.0],
    [
        0.002,
        0.001,
        0.16,
        0.0005,
        -1.0,
        1.0,
        2.0,
        1.2235835836007167e-5,
    ],
    [-0.06, 0.17, 0.54, 0.17, 0.0, 1.0, 2.0, 0.062057419271868045],
    [-0.06, 0.17, 0.54, 0.17, 1.0, 1.0, 2.0, 0.02575125385372528],
    [-0.06, 0.17, 0.54, 0.17, 2.0, 1.0, 2.0, 0.0],
    [-0.06, 0.17, 0.54, 0.17, -1.0, 1.0, 2.0, 0.0686718921058041],
    [0.2, -0.1, 0.5, 0.2, 0.0, 1.0, 2.0, 0.052130613194252676],
    [0.2, -0.1, 0.5, 0.2, 1.0, 1.0, 2.0, 0.10426122638850535],
    [0.2, -0.1, 0.5, 0.2, 2.0, 1.0, 2.0, 0.0],
    [0.2, -0.1, 0.5, 0.2, -1.0, 1.0, 2.0, 0.035537396797031405],
    [-0.1, 0.0, 0.6, 0.1, 0.0, 1.0, 2.0, 0.0031529304021535804],
    [-0.1, 0.0, 0.6, 0.1, 1.0, 1.0, 2.0, 0.0],
    [-0.1, 0.0, 0.6, 0.1, 2.0, 1.0, 2.0, 0.0],
    [-0.1, 0.0, 0.6, 0.1, -1.0, 1.0, 2.0, 0.005832939682340754],
    [0.0, 0.0, 0.7, 0.2, 0.0, 1.0, 2.0, 0.04000000000000001],
    [0.0, 0.0, 0.7, 0.2, 1.0, 1.0, 2.0, 0.04000000000000001],
    [0.0, 0.0, 0.7, 0.2, 2.0, 1.0, 2.0, 0.0],
    [0.0, 0.0, 0.7, 0.2, -1.0, 1.0, 2.0, 0.04000000000000001],
];

const HIGH_PRECISION_SMALL_DECAY: &[[f64; 2]] = &[
    [1e-50, 0.48833333333333334],
    [1e-15, 0.4883333333333322],
    [1e-8, 0.48833332186666684],
];
