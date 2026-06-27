//! Logarithm of the gamma function.
//!
//! Port of `GammaFunction::logValue` from
//! `ql/math/distributions/gammadistribution.{hpp,cpp}` - the Lanczos
//! approximation (Numerical Recipes `gammln`). It underpins the Student-t and
//! gamma-family distributions. Exposed as a free function, like [`erf`].
//!
//! [`erf`]: crate::math::errorfunction::erf

// Coefficients are transcribed verbatim from QuantLib; their precision exceeds
// f64 but rounds to the intended bit pattern.
#![allow(clippy::excessive_precision)]

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
/// use itofin::math::gammafunction::log_gamma;
/// // Γ(5) = 4! = 24
/// let v = log_gamma(5.0)?;
/// assert!((v - 24.0_f64.ln()).abs() < 1e-9);
/// # Ok::<(), itofin::errors::QlError>(())
/// ```
pub fn log_gamma(x: Real) -> QlResult<Real> {
    // reject x <= 0 and NaN, matching QuantLib's QL_REQUIRE(x > 0); a bare
    // `x <= 0.0` would let NaN through (all NaN comparisons are false)
    if x <= 0.0 || x.is_nan() {
        fail!("log_gamma requires a positive argument, got {x}");
    }
    let mut temp = x + 5.5;
    temp -= (x + 0.5) * temp.ln();
    let mut ser = 1.000000000190015;
    for (i, &c) in COEFFS.iter().enumerate() {
        ser += c / (x + (i as Real + 1.0));
    }
    Ok(-temp + (SQRT_TWO_PI * ser / x).ln())
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
}
