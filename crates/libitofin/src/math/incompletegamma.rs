//! Incomplete gamma function.
//!
//! Port of `ql/math/incompletegamma.{hpp,cpp}`: the regularized lower
//! incomplete gamma function `P(a, x)` (the Gamma(a, 1) CDF), evaluated via the
//! Numerical Recipes series expansion for `x < a + 1` and the continued
//! fraction (for the complement `Q = 1 - P`) for `x >= a + 1`. It underpins the
//! gamma and chi-square distribution CDFs.

use crate::errors::QlResult;
use crate::fail;
use crate::math::gammafunction::log_gamma;
use crate::types::Real;

const ACCURACY: Real = 1.0e-13;

/// Iteration cap for both the series and continued-fraction expansions.
///
/// QuantLib hard-codes 100 (Numerical Recipes' `ITMAX`), but near `x ≈ a` both
/// expansions need on the order of `8·√a` terms to reach `ACCURACY`, so 100 is
/// exceeded once `a` grows past ~150 (e.g. a central chi-square CDF with `df`
/// above ~190) and the caller's infallible `.expect()` would panic. The cap
/// `a + 100` provably dominates that `8·√a` requirement for every `a > 0` (the
/// minimum of `a + 100 - 8·√a` is ~+84 at `a = 16`) with ample headroom, so a
/// valid evaluation always converges before the cap; the cap survives only as a
/// backstop against a non-convergent loop. The `as u32` cast saturates rather
/// than wrapping, and the `saturating_add` keeps even an unphysically large `a`
/// (whose cast already hit `u32::MAX`) from overflowing the `+ 100`.
fn max_iterations(a: Real) -> u32 {
    100u32.saturating_add(a.ceil() as u32)
}

/// The regularized lower incomplete gamma function `P(a, x)`.
///
/// This is the CDF of the Gamma(`a`, 1) distribution: `P(a, x) = γ(a, x) / Γ(a)`
/// where `γ` is the lower incomplete gamma integral. `a` must be strictly
/// positive and `x` non-negative.
///
/// # Errors
///
/// Returns an error if `a <= 0`, `x < 0`, either argument is `NaN`, or the
/// series / continued fraction fails to converge.
///
/// # Examples
///
/// ```
/// use libitofin::math::incompletegamma::incomplete_gamma;
/// // P(1, x) is the exponential CDF 1 - e^{-x}
/// let p = incomplete_gamma(1.0, 2.0)?;
/// assert!((p - (1.0 - (-2.0_f64).exp())).abs() < 1e-12);
/// # Ok::<(), libitofin::errors::QlError>(())
/// ```
pub fn incomplete_gamma(a: Real, x: Real) -> QlResult<Real> {
    // `is_finite` is explicit: a bare comparison lets NaN and ±infinity through
    // (all NaN comparisons are false, and +infinity is not <= 0 / not < 0), but
    // QuantLib's QL_REQUIRE rejects them.
    if !a.is_finite() || a <= 0.0 {
        fail!("incomplete_gamma requires a finite a > 0, got a={a}");
    }
    if !x.is_finite() || x < 0.0 {
        fail!("incomplete_gamma requires a finite x >= 0, got x={x}");
    }
    if x < a + 1.0 {
        series_repr(a, x)
    } else {
        // the continued fraction yields Q(a, x) = 1 - P(a, x)
        Ok(1.0 - continued_fraction_repr(a, x)?)
    }
}

/// Series representation of `P(a, x)`, used for `x < a + 1` (NR `gser`).
fn series_repr(a: Real, x: Real) -> QlResult<Real> {
    if x == 0.0 {
        return Ok(0.0);
    }
    let gln = log_gamma(a)?;
    let mut ap = a;
    let mut del = 1.0 / a;
    let mut sum = del;
    for _ in 1..=max_iterations(a) {
        ap += 1.0;
        del *= x / ap;
        sum += del;
        if del.abs() < sum.abs() * ACCURACY {
            return Ok(sum * (-x + a * x.ln() - gln).exp());
        }
    }
    fail!("incomplete_gamma series did not converge (a={a}, x={x})");
}

/// Continued-fraction representation of `Q(a, x) = 1 - P(a, x)`, used for
/// `x >= a + 1` (NR `gcf`, Lentz's method).
fn continued_fraction_repr(a: Real, x: Real) -> QlResult<Real> {
    let eps = Real::EPSILON;
    let gln = log_gamma(a)?;
    let mut b = x + 1.0 - a;
    let mut c = 1.0 / eps;
    let mut d = 1.0 / b;
    let mut h = d;
    for i in 1..=max_iterations(a) {
        let an = -(i as Real) * (i as Real - a);
        b += 2.0;
        d = an * d + b;
        if d.abs() < eps {
            d = eps;
        }
        c = b + an / c;
        if c.abs() < eps {
            c = eps;
        }
        d = 1.0 / d;
        let del = d * c;
        h *= del;
        if (del - 1.0).abs() < ACCURACY {
            return Ok((-x + a * x.ln() - gln).exp() * h);
        }
    }
    fail!("incomplete_gamma continued fraction did not converge (a={a}, x={x})");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::errorfunction::erf;

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
    fn boundary_at_zero_is_zero() {
        for &a in &[0.5, 1.0, 2.0, 7.5] {
            assert_eq!(incomplete_gamma(a, 0.0).unwrap(), 0.0);
        }
    }

    #[test]
    fn matches_exponential_cdf_a_eq_1() {
        // P(1, x) = 1 - e^{-x}; x spans the series (x<2) and CF (x>=2) branches.
        for &x in &[0.1, 0.5, 1.0, 1.9, 2.0, 5.0, 20.0_f64] {
            assert_close(incomplete_gamma(1.0, x).unwrap(), 1.0 - (-x).exp());
        }
    }

    #[test]
    fn matches_integer_a_closed_forms() {
        // P(2, x) = 1 - e^{-x}(1 + x); P(3, x) = 1 - e^{-x}(1 + x + x²/2).
        for &x in &[0.3, 1.0, 2.5, 3.0, 8.0_f64] {
            let e = (-x).exp();
            assert_close(incomplete_gamma(2.0, x).unwrap(), 1.0 - e * (1.0 + x));
            assert_close(
                incomplete_gamma(3.0, x).unwrap(),
                1.0 - e * (1.0 + x + 0.5 * x * x),
            );
        }
    }

    #[test]
    fn matches_erf_a_eq_half() {
        // P(1/2, x) = erf(sqrt(x)).
        for &x in &[0.05, 0.5, 1.0, 1.5, 4.0, 10.0_f64] {
            assert_close(incomplete_gamma(0.5, x).unwrap(), erf(x.sqrt()));
        }
    }

    #[test]
    fn approaches_one_in_the_tail() {
        assert_close(incomplete_gamma(3.0, 60.0).unwrap(), 1.0);
    }

    #[test]
    fn is_monotonic_increasing_in_x() {
        let a = 4.0;
        let mut prev = incomplete_gamma(a, 0.0).unwrap();
        let mut x = 0.0;
        while x < 30.0 {
            x += 0.1;
            let cur = incomplete_gamma(a, x).unwrap();
            assert!(cur >= prev, "not increasing at x={x}: {prev} -> {cur}");
            prev = cur;
        }
    }

    #[test]
    fn converges_for_large_a() {
        // Regression: with the old fixed cap of 100, both branches hit the
        // ~8·√a iteration requirement and failed to converge for a ≳ 150.
        // Converged values, each cross-checked to ~1e-10 against an independent
        // high-iteration reference implementation of the same expansions.
        // Series branch (x < a+1), x = a:
        assert_close(incomplete_gamma(500.0, 500.0).unwrap(), 0.5059471460854907);
        assert_close(incomplete_gamma(200.0, 200.0).unwrap(), 0.5094034179355048);
        // Continued-fraction branch (x >= a+1) with large a near the boundary.
        assert_close(incomplete_gamma(500.0, 600.0).unwrap(), 0.9999877440576714);
    }

    #[test]
    fn invalid_args_rejected() {
        assert!(incomplete_gamma(0.0, 1.0).is_err()); // a <= 0
        assert!(incomplete_gamma(-1.0, 1.0).is_err()); // a < 0
        assert!(incomplete_gamma(1.0, -1.0).is_err()); // x < 0
        assert!(incomplete_gamma(Real::NAN, 1.0).is_err()); // NaN a
        assert!(incomplete_gamma(1.0, Real::NAN).is_err()); // NaN x
        assert!(incomplete_gamma(Real::INFINITY, 1.0).is_err()); // +inf a
        assert!(incomplete_gamma(1.0, Real::INFINITY).is_err()); // +inf x
    }
}
