//! Typed projection of the independent pinned native GBM CSV oracle.
//!
//! Canonical inputs and expected results live in
//! `sdk/go/testdata/geometric-brownian/cases.csv`; this projection keeps the
//! core crate's tests self-contained when packaged without the SDK directory.

use super::*;

const CASES: &[(&str, [Real; 14])] = &[
    (
        "positive",
        [
            100.0, 0.05, 0.2, 0.0, 100.0, 0.25, 0.5, 100.0, 5.0, 20.0, 101.25, 100.0, 10.0, 106.25,
        ],
    ),
    (
        "negative_state",
        [
            -100.0,
            0.05,
            0.2,
            1.0,
            -80.0,
            0.5,
            -0.75,
            -100.0,
            -4.0,
            -16.0,
            -82.0,
            128.0,
            -11.313708498984761,
            -73.51471862576143,
        ],
    ),
    (
        "zero_state",
        [
            0.0, -0.03, 0.2, 2.0, 0.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0,
        ],
    ),
    (
        "negative_drift",
        [
            12.0,
            -0.3,
            0.45,
            0.75,
            15.0,
            0.1,
            -1.5,
            12.0,
            -4.5,
            6.75,
            14.55,
            4.55625,
            2.1345374206136563,
            11.348193869079516,
        ],
    ),
    (
        "zero_volatility",
        [
            -20.0, 0.2, 0.0, 0.0, -10.0, 0.5, 2.0, -20.0, -2.0, 0.0, -11.0, 0.0, 0.0, -11.0,
        ],
    ),
    (
        "zero_dt",
        [
            100.0, 0.1, 0.4, 5.0, 90.0, 0.0, -3.0, 100.0, 9.0, 36.0, 90.0, 0.0, 0.0, 90.0,
        ],
    ),
    (
        "cross_zero",
        [
            100.0, 0.1, 0.8, 0.0, 100.0, 1.0, -2.0, 100.0, 10.0, 80.0, 110.0, 6400.0, 80.0, -50.0,
        ],
    ),
    (
        "fractional",
        [
            0.123,
            -0.007,
            0.031,
            0.37,
            0.234,
            0.019,
            -0.127,
            0.123,
            -0.0016380000000000001,
            0.007254,
            0.23396887800000002,
            9.99789804e-07,
            0.0009998948964766248,
            0.23384189134814748,
        ],
    ),
    (
        "zero_initial_positive_state",
        [
            0.0,
            0.03,
            0.2,
            0.0,
            30.0,
            2.0,
            0.1,
            0.0,
            0.8999999999999999,
            6.0,
            31.8,
            72.0,
            8.485281374238571,
            32.64852813742386,
        ],
    ),
];

fn native_close(actual: Real, expected: Real, case: &str, quantity: usize) {
    let tolerance = 2e-14_f64.max(3e-12 * expected.abs());
    assert!(
        actual.is_finite() && (actual - expected).abs() <= tolerance,
        "{case} quantity {quantity}: {actual} != {expected}, tolerance {tolerance}"
    );
}

#[test]
fn all_nine_native_gbm_cases_match_every_coefficient_and_transition() {
    assert_eq!(CASES.len(), 9);
    for (name, values) in CASES {
        let p = GeometricBrownianMotionProcess::new(values[0], values[1], values[2]).unwrap();
        let (t, x, dt, dw) = (values[3], values[4], values[5], values[6]);
        let actual = [
            p.x0().unwrap(),
            p.drift(t, x).unwrap(),
            p.diffusion(t, x).unwrap(),
            p.expectation(t, x, dt).unwrap(),
            p.variance(t, x, dt).unwrap(),
            p.std_deviation(t, x, dt).unwrap(),
            p.evolve(t, x, dt, dw).unwrap(),
        ];
        for (quantity, (actual, expected)) in actual.iter().zip(&values[7..]).enumerate() {
            native_close(*actual, *expected, name, quantity);
        }
    }
}
