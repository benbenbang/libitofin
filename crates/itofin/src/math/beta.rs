//! Beta functions.
//!
//! Port of `ql/math/beta.{hpp,cpp}`: the complete [`beta_function`] and the
//! regularized [`incomplete_beta`] (the CDF of the Beta distribution), the
//! latter via the Numerical Recipes continued-fraction expansion with a
//! [`log_gamma`] prefactor. Needed for the Student-t CDF.

use crate::errors::QlResult;
use crate::fail;
use crate::math::gammafunction::log_gamma;
use crate::types::Real;

const ACCURACY: Real = 1e-16;
const MAX_ITERATIONS: u32 = 100;

/// The complete beta function `B(z, w) = Γ(z)·Γ(w) / Γ(z+w)`.
///
/// # Errors
///
/// Returns an error if `z`, `w`, or `z + w` is not a valid `log_gamma`
/// argument (i.e. not strictly positive).
///
/// # Examples
///
/// ```
/// use itofin::math::beta::beta_function;
/// // B(2, 3) = 1!·2!/4! = 1/12
/// let b = beta_function(2.0, 3.0)?;
/// assert!((b - 1.0 / 12.0).abs() < 1e-12);
/// # Ok::<(), itofin::errors::QlError>(())
/// ```
pub fn beta_function(z: Real, w: Real) -> QlResult<Real> {
    Ok((log_gamma(z)? + log_gamma(w)? - log_gamma(z + w)?).exp())
}

/// The regularized incomplete beta function `I_x(a, b)` (the Beta(a, b) CDF).
///
/// `a` and `b` must be strictly positive and `x` must lie in `[0, 1]`.
///
/// # Errors
///
/// Returns an error if `a <= 0`, `b <= 0`, `x` is outside `[0, 1]`, any
/// argument is `NaN`, or the continued fraction fails to converge.
///
/// # Examples
///
/// ```
/// use itofin::math::beta::incomplete_beta;
/// // Beta(1, 1) is uniform, so I_x(1, 1) = x
/// let p = incomplete_beta(1.0, 1.0, 0.25)?;
/// assert!((p - 0.25).abs() < 1e-12);
/// # Ok::<(), itofin::errors::QlError>(())
/// ```
pub fn incomplete_beta(a: Real, b: Real, x: Real) -> QlResult<Real> {
    // `is_finite()` is required: a bare `<= 0.0` lets NaN and +infinity through
    // (all NaN comparisons are false, and +infinity is not <= 0), but
    // QuantLib's QL_REQUIRE(a > 0) rejects them
    if !a.is_finite() || a <= 0.0 {
        fail!("incomplete_beta requires a finite a > 0, got a={a}");
    }
    if !b.is_finite() || b <= 0.0 {
        fail!("incomplete_beta requires a finite b > 0, got b={b}");
    }
    // `contains` is false for NaN, so this also rejects NaN
    if !(0.0..=1.0).contains(&x) {
        fail!("incomplete_beta requires x in [0, 1], got x={x}");
    }
    if x == 0.0 {
        return Ok(0.0);
    }
    if x == 1.0 {
        return Ok(1.0);
    }

    let prefactor =
        (log_gamma(a + b)? - log_gamma(a)? - log_gamma(b)? + a * x.ln() + b * (1.0 - x).ln()).exp();

    // use the expansion that converges fastest for this x
    if x < (a + 1.0) / (a + b + 2.0) {
        Ok(prefactor * beta_continued_fraction(a, b, x)? / a)
    } else {
        Ok(1.0 - prefactor * beta_continued_fraction(b, a, 1.0 - x)? / b)
    }
}

/// Lentz's continued-fraction evaluation for [`incomplete_beta`] (NR `betacf`).
fn beta_continued_fraction(a: Real, b: Real, x: Real) -> QlResult<Real> {
    let eps = Real::EPSILON;
    let qab = a + b;
    let qap = a + 1.0;
    let qam = a - 1.0;
    let mut c = 1.0;
    let mut d = 1.0 - qab * x / qap;
    if d.abs() < eps {
        d = eps;
    }
    d = 1.0 / d;
    let mut result = d;

    for iter in 1..=MAX_ITERATIONS {
        let m = iter as Real;
        let m2 = 2.0 * m;

        // even step
        let aa = m * (b - m) * x / ((qam + m2) * (a + m2));
        d = 1.0 + aa * d;
        if d.abs() < eps {
            d = eps;
        }
        c = 1.0 + aa / c;
        if c.abs() < eps {
            c = eps;
        }
        d = 1.0 / d;
        result *= d * c;

        // odd step
        let aa = -(a + m) * (qab + m) * x / ((a + m2) * (qap + m2));
        d = 1.0 + aa * d;
        if d.abs() < eps {
            d = eps;
        }
        c = 1.0 + aa / c;
        if c.abs() < eps {
            c = eps;
        }
        d = 1.0 / d;
        let del = d * c;
        result *= del;
        if (del - 1.0).abs() < ACCURACY {
            return Ok(result);
        }
    }
    fail!("incomplete_beta continued fraction did not converge (a={a}, b={b})");
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Real = 1e-12;

    fn assert_close(got: Real, expected: Real) {
        let tol = TOL * (1.0 + expected.abs());
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}, diff {}",
            (got - expected).abs()
        );
    }

    #[test]
    fn beta_function_known_values() {
        // B(2,3) = 1!·2!/4! = 1/12
        assert_close(beta_function(2.0, 3.0).unwrap(), 1.0 / 12.0);
        // B(1/2,1/2) = Γ(1/2)²/Γ(1) = π
        assert_close(beta_function(0.5, 0.5).unwrap(), std::f64::consts::PI);
    }

    #[test]
    fn boundaries_and_uniform() {
        assert_eq!(incomplete_beta(2.0, 3.0, 0.0).unwrap(), 0.0);
        assert_eq!(incomplete_beta(2.0, 3.0, 1.0).unwrap(), 1.0);
        // Beta(1,1) is uniform: I_x(1,1) = x
        assert_close(incomplete_beta(1.0, 1.0, 0.3).unwrap(), 0.3);
        assert_close(incomplete_beta(1.0, 1.0, 0.5).unwrap(), 0.5);
    }

    #[test]
    fn known_values() {
        // symmetric Beta(2,2): I_{0.5} = 0.5
        assert_close(incomplete_beta(2.0, 2.0, 0.5).unwrap(), 0.5);
        // Beta(2,3) CDF at 0.5 = 0.6875 (ELSE branch, x > 3/7; computed by hand)
        assert_close(incomplete_beta(2.0, 3.0, 0.5).unwrap(), 0.6875);
        // Beta(2,3) CDF at 0.3 = 0.3483 (IF/direct-CF branch, x < 3/7)
        assert_close(incomplete_beta(2.0, 3.0, 0.3).unwrap(), 0.3483);
    }

    #[test]
    fn symmetry_identity() {
        // I_x(a,b) = 1 - I_{1-x}(b,a)
        for &(a, b, x) in &[(2.0, 3.0, 0.3), (0.5, 2.5, 0.7), (4.0, 1.5, 0.2)] {
            let lhs = incomplete_beta(a, b, x).unwrap();
            let rhs = 1.0 - incomplete_beta(b, a, 1.0 - x).unwrap();
            assert!(
                (lhs - rhs).abs() < TOL,
                "symmetry failed at a={a}, b={b}, x={x}"
            );
        }
    }

    #[test]
    fn invalid_args_rejected() {
        assert!(incomplete_beta(0.0, 1.0, 0.5).is_err()); // a <= 0
        assert!(incomplete_beta(1.0, -1.0, 0.5).is_err()); // b <= 0
        assert!(incomplete_beta(1.0, 1.0, 1.5).is_err()); // x out of range
        assert!(incomplete_beta(1.0, 1.0, Real::NAN).is_err()); // NaN x
        assert!(incomplete_beta(Real::NAN, 1.0, 0.5).is_err()); // NaN a
        assert!(incomplete_beta(1.0, Real::NAN, 0.5).is_err()); // NaN b
        assert!(incomplete_beta(Real::INFINITY, 1.0, 0.5).is_err()); // +inf a
        assert!(incomplete_beta(1.0, Real::INFINITY, 0.5).is_err()); // +inf b
        // beta_function propagates through log_gamma, so infinity errors too
        assert!(beta_function(Real::INFINITY, 1.0).is_err());
    }
}
