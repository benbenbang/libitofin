//! Gamma function.
//!
//! Port of `GammaFunction::{logValue, value}` from
//! `ql/math/distributions/gammadistribution.{hpp,cpp}`: [`log_gamma`] (the
//! Lanczos / Numerical Recipes `gammln` approximation) and [`gamma`] itself
//! (`Γ(x)` via recurrence and reflection). They underpin the Student-t and
//! gamma-family distributions. Exposed as free functions, like [`erf`].
//!
//! [`erf`]: crate::math::errorfunction::erf

// Coefficients are transcribed verbatim from QuantLib; their precision exceeds
// f64 but rounds to the intended bit pattern.
#![allow(clippy::excessive_precision)]

use std::f64::consts::PI;

use crate::errors::QlResult;
use crate::fail;
use crate::types::Real;

// Lanczos series coefficients (g = 5, n = 6).
const COEFFS: [Real; 6] = [
    76.18009172947146,
    -86.50532032941677,
    24.01409824083091,
    -1.231739572450155,
    0.1208650973866179e-2,
    -0.5395239384953e-5,
];

// √(2π), the Lanczos normalization.
const SQRT_TWO_PI: Real = 2.5066282746310005;

/// The natural logarithm of the gamma function, `ln Γ(x)`, for `x > 0`.
///
/// Accurate to a relative error of about `2e-10` (the Lanczos approximation).
///
/// # Errors
///
/// Returns an error if `x` is not strictly positive, including `NaN` - matching
/// QuantLib's `QL_REQUIRE(x > 0)`.
///
/// # Examples
///
/// ```
/// use libitofin::math::gammafunction::log_gamma;
/// // Γ(5) = 4! = 24
/// let v = log_gamma(5.0)?;
/// assert!((v - 24.0_f64.ln()).abs() < 1e-9);
/// # Ok::<(), libitofin::errors::QlError>(())
/// ```
pub fn log_gamma(x: Real) -> QlResult<Real> {
    // reject non-finite and x <= 0, matching QuantLib's QL_REQUIRE(x > 0); a
    // bare `x <= 0.0` would let NaN and +infinity through (all NaN comparisons
    // are false, and +infinity is not <= 0)
    if !x.is_finite() || x <= 0.0 {
        fail!("log_gamma requires a finite positive argument, got {x}");
    }
    let mut temp = x + 5.5;
    temp -= (x + 0.5) * temp.ln();
    let mut ser = 1.000000000190015;
    for (i, &c) in COEFFS.iter().enumerate() {
        ser += c / (x + (i as Real + 1.0));
    }
    Ok(-temp + (SQRT_TWO_PI * ser / x).ln())
}

/// The gamma function `Γ(x)`.
///
/// Port of `GammaFunction::value`. For `x >= 1` it exponentiates [`log_gamma`];
/// for `-20 < x < 1` it applies the recurrence `Γ(x) = Γ(x+1) / x`; for
/// `x <= -20` it applies the reflection formula
/// `Γ(x) = -π / (Γ(-x)·x·sin(πx))`.
///
/// At the non-positive integer poles `Γ` is undefined. Poles in `-20 < x <= 0`
/// surface as `±∞` (the recurrence divides by zero), but poles `x <= -20` reach
/// the reflection branch, where `sin(πx)` is only approximately zero in floating
/// point, so they return a finite (meaningless) value rather than `±∞` - this
/// matches QuantLib. A `NaN` argument returns `NaN`.
///
/// # Examples
///
/// ```
/// use libitofin::math::gammafunction::gamma;
/// // Γ(5) = 4! = 24
/// assert!((gamma(5.0) - 24.0).abs() < 1e-6);
/// // Γ(1/2) = √π
/// assert!((gamma(0.5) - std::f64::consts::PI.sqrt()).abs() < 1e-9);
/// ```
pub fn gamma(x: Real) -> Real {
    // Non-finite inputs are handled up front: +inf would hit log_gamma's
    // finite-only domain and panic the expect below, -inf would recurse into
    // gamma(+inf) and do the same, and NaN would recurse forever on the
    // reflection branch. Gamma diverges to +inf at +inf and has no limit at
    // -inf (NaN), as for NaN input.
    if !x.is_finite() {
        return if x > 0.0 { Real::INFINITY } else { Real::NAN };
    }
    if x >= 1.0 {
        // x >= 1 > 0 is always a valid log_gamma argument.
        log_gamma(x).expect("log_gamma is valid for x >= 1").exp()
    } else if x > -20.0 {
        // recurrence: Γ(x) = Γ(x+1) / x
        gamma(x + 1.0) / x
    } else {
        // reflection: Γ(x) = -π / (Γ(-x)·x·sin(πx))
        -PI / (gamma(-x) * x * (PI * x).sin())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Mixed absolute/relative tolerance: the Lanczos approximation is good to
    // ~2e-10 relative, which is looser in absolute terms as |ln Γ| grows.
    const TOL: Real = 1e-9;

    fn assert_close(got: Real, expected: Real) {
        let tol = TOL * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}, diff {}",
            (got - expected).abs()
        );
    }

    #[test]
    fn matches_known_values() {
        // Γ(1) = Γ(2) = 1  →  ln Γ = 0
        assert_close(log_gamma(1.0).unwrap(), 0.0);
        assert_close(log_gamma(2.0).unwrap(), 0.0);
        // Γ(1/2) = √π  →  ln Γ(1/2) = ln √π
        assert_close(log_gamma(0.5).unwrap(), 0.572_364_942_924_700_1);
        // Γ(n) = (n-1)!
        assert_close(log_gamma(5.0).unwrap(), 3.178_053_830_347_945_8); // ln 24
        assert_close(log_gamma(10.0).unwrap(), 12.801_827_480_081_469); // ln 362880
    }

    #[test]
    fn small_and_large_arguments() {
        assert_close(log_gamma(0.1).unwrap(), 2.252_712_651_734_206);
        assert_close(log_gamma(100.0).unwrap(), 359.134_205_369_575_4); // ln 99!
    }

    #[test]
    fn recurrence_ln_gamma_x_plus_1() {
        // ln Γ(x+1) = ln Γ(x) + ln x; two log_gamma calls plus a ln, so allow a
        // slightly looser bound than a single evaluation
        for &x in &[0.3, 1.7, 4.2, 9.9] {
            let lhs = log_gamma(x + 1.0).unwrap();
            let rhs = log_gamma(x).unwrap() + x.ln();
            assert!((lhs - rhs).abs() < 1e-8, "recurrence failed at {x}");
        }
    }

    #[test]
    fn nonpositive_or_nan_argument_is_rejected() {
        assert!(log_gamma(0.0).is_err());
        assert!(log_gamma(-1.0).is_err());
        // NaN must be rejected too, matching QuantLib's QL_REQUIRE(x > 0)
        assert!(log_gamma(Real::NAN).is_err());
        // +infinity must be rejected: a bare `x <= 0.0` would let it through
        assert!(log_gamma(Real::INFINITY).is_err());
        assert!(log_gamma(Real::NEG_INFINITY).is_err());
    }

    #[test]
    fn matches_cumulative_log_factorial_through_9000() {
        // Faithful port of QuantLib's testGammaFunction: ln Γ(1) = 0, then
        // ln Γ(i+1) = Σ_{k=2}^{i} ln k, checked to 1e-9 relative across i < 9000.
        assert!(log_gamma(1.0).unwrap().abs() <= 1e-15);
        let mut expected = 0.0;
        for i in 2..9000 {
            expected += Real::from(i).ln();
            let calculated = log_gamma(Real::from(i + 1)).unwrap();
            assert!(
                (calculated - expected).abs() / expected <= 1e-9,
                "log_gamma({}) rel err {}",
                i + 1,
                (calculated - expected).abs() / expected
            );
        }
    }

    #[test]
    fn value_matches_reference_table() {
        // Faithful port of QuantLib's testGammaValues: (x, Γ(x) from R, tol
        // multiplier), checked to `multiplier * EPSILON * |expected|`.
        let tasks: [(Real, Real, Real); 10] = [
            (0.0001, 9999.422883231624, 1e3),
            (1.2, 0.9181687423997607, 1e3),
            (7.3, 1271.4236336639089586, 1e3),
            (-1.1, 9.7148063829028946, 1e3),
            (-4.001, -41.6040228304425312, 1e3),
            (-4.999, -8.347576090315059, 1e3),
            (-19.000001, 8.220610833201313e-12, 1e8),
            (-19.5, 5.811045977502255e-18, 1e3),
            (-21.000001, 1.957288098276488e-14, 1e8),
            (-21.5, 1.318444918321553e-20, 1e6),
        ];
        for (x, expected, multiplier) in tasks {
            let calculated = gamma(x);
            let tol = multiplier * Real::EPSILON * expected.abs();
            assert!(
                (calculated - expected).abs() <= tol,
                "Γ({x}): got {calculated}, expected {expected}, diff {}, tol {tol}",
                (calculated - expected).abs()
            );
        }
    }

    #[test]
    fn value_handles_poles_and_nan() {
        // Poles in -20 < x <= 0 hit an exact divide-by-zero in the recurrence,
        // so they surface as ±∞.
        assert!(gamma(0.0).is_infinite());
        assert!(gamma(-1.0).is_infinite());
        assert!(gamma(-19.0).is_infinite());
        // A pole x <= -20 goes through the reflection branch, where sin(πx) is
        // only approximately zero, so it returns a finite (non-Γ) value, not
        // ±∞. This matches QuantLib; we pin it so the doc claim cannot drift.
        assert!(gamma(-20.0).is_finite());
        // NaN propagates.
        assert!(gamma(Real::NAN).is_nan());
    }

    #[test]
    fn value_at_infinities_does_not_panic() {
        // Regression: log_gamma now rejects non-finite x, so gamma must handle
        // +-inf before the expect path rather than panic. Gamma diverges to +inf
        // at +inf and has no limit at -inf.
        assert_eq!(gamma(Real::INFINITY), Real::INFINITY);
        assert!(gamma(Real::NEG_INFINITY).is_nan());
    }
}
