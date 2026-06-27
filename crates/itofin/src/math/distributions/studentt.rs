//! Student's t-distribution.
//!
//! Port of `ql/math/distributions/studenttdistribution.{hpp,cpp}`: the standard
//! (location 0, scale 1) Student-t [`StudentT`] with `ν` degrees of freedom,
//! providing the density, the cumulative distribution via the regularized
//! [`incomplete_beta`] function, and the inverse via the Newton iteration that
//! QuantLib's `InverseCumulativeStudent` uses.

use std::f64::consts::PI;

use super::{Cdf, Density, Probability, Quantile, Support};
use crate::errors::QlResult;
use crate::fail;
use crate::math::beta::incomplete_beta;
use crate::math::gammafunction::log_gamma;
use crate::require;
use crate::types::Real;

// Defaults transcribed from QuantLib's InverseCumulativeStudent (accuracy 1e-6,
// 50 iterations). Kept private until a consumer needs them configurable, rather
// than introducing a generic solver-options type speculatively.
const QUANTILE_ACCURACY: Real = 1e-6;
const QUANTILE_MAX_ITERATIONS: u32 = 50;

/// The standard Student-t distribution with `ν` degrees of freedom.
#[derive(Clone, Copy, Debug)]
pub struct StudentT {
    degrees_of_freedom: Real,
    // log of the density normalization: ln Γ((ν+1)/2) - ln Γ(ν/2) - ½ ln(νπ).
    ln_normalization: Real,
}

impl StudentT {
    /// A Student-t distribution with the given degrees of freedom.
    ///
    /// `ν` may be any finite, strictly positive real. QuantLib takes an integer
    /// degree count; accepting a real `ν` is a deliberate generalization (the
    /// Student-t is defined for all real `ν > 0`), not QuantLib parity.
    ///
    /// # Errors
    ///
    /// Returns an error unless `degrees_of_freedom` is finite and `> 0` (so
    /// `NaN` and the infinities are rejected).
    pub fn new(degrees_of_freedom: Real) -> QlResult<Self> {
        // `is_finite` rejects NaN and both infinities; a non-finite ν would
        // make `ln_normalization` NaN and later panic the cdf's `expect`.
        require!(
            degrees_of_freedom.is_finite() && degrees_of_freedom > 0.0,
            "degrees of freedom must be a finite positive number, got {degrees_of_freedom}"
        );
        let half = 0.5 * degrees_of_freedom;
        // half + 0.5 >= 0.5 > 0 and half > 0, so both are valid log_gamma args.
        let ln_normalization =
            log_gamma(half + 0.5)? - log_gamma(half)? - 0.5 * (degrees_of_freedom * PI).ln();
        Ok(StudentT {
            degrees_of_freedom,
            ln_normalization,
        })
    }
}

impl Density for StudentT {
    fn pdf(&self, x: Real) -> Real {
        // Evaluate through the log-density to avoid forming exp(lgamma) factors
        // that overflow for large ν. Mathematically equal to QuantLib's direct
        // formula, but not intended to be bit-for-bit identical.
        self.ln_pdf(x).exp()
    }

    fn ln_pdf(&self, x: Real) -> Real {
        let nu = self.degrees_of_freedom;
        self.ln_normalization - 0.5 * (nu + 1.0) * (1.0 + x * x / nu).ln()
    }
}

impl Cdf for StudentT {
    fn cdf(&self, x: Real) -> Real {
        if x.is_nan() {
            return Real::NAN;
        }
        let nu = self.degrees_of_freedom;
        let z = nu / (nu + x * x);
        // For any valid StudentT the incomplete-beta arguments are restricted to
        //   a = ν/2 >= 0.5,  b = 0.5,  z = ν/(ν + x²) in [0, 1],
        // and the z = 0 (x = ±∞) and z = 1 (x = 0) endpoints short-circuit in
        // incomplete_beta before its continued fraction runs. Failure to
        // converge on this domain is a defect in the special function, not a
        // caller-recoverable condition, so we assert rather than propagate.
        let tail = 0.5
            * incomplete_beta(0.5 * nu, 0.5, z)
                .expect("incomplete beta must converge for valid Student-t parameters");
        if x <= 0.0 { tail } else { 1.0 - tail }
    }

    fn survival(&self, x: Real) -> Real {
        // Exact for a symmetric distribution and accurate in the upper tail,
        // where `1 - cdf(x)` would cancel; `cdf(-x)` evaluates the small tail
        // probability directly.
        self.cdf(-x)
    }
}

impl Support for StudentT {
    fn lower_bound(&self) -> Real {
        Real::NEG_INFINITY
    }

    fn upper_bound(&self) -> Real {
        Real::INFINITY
    }
}

impl Quantile for StudentT {
    fn quantile(&self, p: Probability) -> QlResult<Real> {
        let p = p.value();
        // The generalized inverse maps the closed endpoints to the support.
        if p == 0.0 {
            return Ok(self.lower_bound());
        }
        if p == 1.0 {
            return Ok(self.upper_bound());
        }
        // Newton-Raphson from x = 0 using the density as derivative, exactly as
        // QuantLib's InverseCumulativeStudent. Genuinely fallible: extreme-tail
        // probabilities can exhaust the iteration budget.
        let mut x = 0.0;
        for _ in 0..QUANTILE_MAX_ITERATIONS {
            let diff = (self.cdf(x) - p) / self.pdf(x);
            x -= diff;
            if diff.abs() <= QUANTILE_ACCURACY {
                return Ok(x);
            }
        }
        fail!(
            "Student-t quantile did not converge for p={p} (df={})",
            self.degrees_of_freedom
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Real = 1e-12;

    fn assert_close(got: Real, expected: Real, tol: Real) {
        assert!(
            (got - expected).abs() <= tol,
            "got {got}, expected {expected}, diff {}",
            (got - expected).abs()
        );
    }

    // I. The invariant behind the infallible `cdf`: across the full supported
    // parameter domain the incomplete beta converges (otherwise `expect` panics
    // and this test fails) and the result is a valid probability.
    #[test]
    fn cdf_stays_in_unit_interval_across_supported_domain() {
        let dofs = [1.0, 2.0, 3.0, 5.0, 10.0, 30.0, 100.0, 1_000.0, 1.0e6];
        let xs = [
            Real::NEG_INFINITY,
            -1.0e100,
            -1.0e10,
            -100.0,
            -10.0,
            -1.0,
            -Real::MIN_POSITIVE,
            0.0,
            Real::MIN_POSITIVE,
            1.0,
            10.0,
            100.0,
            1.0e10,
            1.0e100,
            Real::INFINITY,
        ];
        for nu in dofs {
            let dist = StudentT::new(nu).unwrap();
            for x in xs {
                let p = dist.cdf(x);
                assert!((0.0..=1.0).contains(&p), "cdf({x}) = {p} for nu = {nu}");
            }
        }
    }

    // II. Reference values from closed forms that need no external table.
    #[test]
    fn cdf_matches_cauchy_closed_form() {
        // nu = 1 is the standard Cauchy: F(x) = 1/2 + atan(x)/pi.
        let dist = StudentT::new(1.0).unwrap();
        for x in [-3.0, -1.0, -0.25, 0.0, 0.25, 1.0, 3.0_f64] {
            let expected = 0.5 + x.atan() / PI;
            assert_close(dist.cdf(x), expected, 1e-12);
        }
    }

    #[test]
    fn cdf_matches_nu2_closed_form() {
        // nu = 2 has the closed form F(x) = 1/2 (1 + x / sqrt(x² + 2)).
        let dist = StudentT::new(2.0).unwrap();
        for x in [-4.0, -1.0, 0.0, 0.5, 1.0, 4.0_f64] {
            let expected = 0.5 * (1.0 + x / (x * x + 2.0).sqrt());
            assert_close(dist.cdf(x), expected, 1e-12);
        }
    }

    #[test]
    fn cdf_approaches_normal_for_large_dof() {
        // As nu -> infinity the Student-t tends to the standard normal.
        let dist = StudentT::new(1.0e8).unwrap();
        // Phi(1) = 0.8413447460685429, Phi(1.96) = 0.9750021048517795.
        assert_close(dist.cdf(1.0), 0.841_344_746_068_542_9, 1e-6);
        assert_close(dist.cdf(1.96), 0.975_002_104_851_779_5, 1e-6);
    }

    // III. Density: log-density-primary path and its analytic value at 0.
    // The ln_pdf -> exp evaluation is not bit-for-bit equal to the closed form
    // (it trades exactness for overflow resistance, see `pdf`), so these compare
    // to a tight but non-zero tolerance rather than asserting equality.
    #[test]
    fn pdf_at_zero_matches_analytic() {
        // nu = 1 (Cauchy): f(0) = 1/pi.
        assert_close(StudentT::new(1.0).unwrap().pdf(0.0), 1.0 / PI, 1e-13);
        // nu = 2: f(0) = 1 / (2 sqrt(2)).
        assert_close(
            StudentT::new(2.0).unwrap().pdf(0.0),
            1.0 / (2.0 * 2.0_f64.sqrt()),
            1e-13,
        );
    }

    #[test]
    fn pdf_is_exp_of_ln_pdf() {
        let dist = StudentT::new(7.0).unwrap();
        for x in [-5.0, -1.0, 0.0, 0.5, 3.0] {
            assert_close(dist.pdf(x), dist.ln_pdf(x).exp(), TOL);
        }
    }

    // IV. Limits at the boundary of the support.
    #[test]
    fn limits_at_infinity() {
        let dist = StudentT::new(4.0).unwrap();
        assert_eq!(dist.cdf(Real::NEG_INFINITY), 0.0);
        assert_eq!(dist.cdf(Real::INFINITY), 1.0);
        assert_eq!(dist.pdf(Real::INFINITY), 0.0);
        assert_eq!(dist.pdf(Real::NEG_INFINITY), 0.0);
        assert_eq!(dist.ln_pdf(Real::INFINITY), Real::NEG_INFINITY);
    }

    #[test]
    fn cdf_of_nan_is_nan() {
        assert!(StudentT::new(3.0).unwrap().cdf(Real::NAN).is_nan());
    }

    // V. Structural properties: symmetry, monotonicity, survival.
    #[test]
    fn cdf_is_symmetric() {
        for nu in [1.0, 3.0, 12.0, 250.0] {
            let dist = StudentT::new(nu).unwrap();
            for x in [0.1, 0.7, 1.5, 4.0, 25.0] {
                assert_close(dist.cdf(-x), 1.0 - dist.cdf(x), 1e-12);
            }
        }
    }

    #[test]
    fn cdf_is_strictly_increasing() {
        let dist = StudentT::new(6.0).unwrap();
        let mut prev = dist.cdf(-50.0);
        let mut x = -50.0;
        while x < 50.0 {
            x += 0.1;
            let cur = dist.cdf(x);
            assert!(cur > prev, "cdf not increasing at x = {x}: {prev} -> {cur}");
            prev = cur;
        }
    }

    #[test]
    fn survival_is_complement_and_uses_symmetry() {
        let dist = StudentT::new(9.0).unwrap();
        for x in [-2.0, 0.0, 1.0, 8.0] {
            assert_close(dist.survival(x), 1.0 - dist.cdf(x), 1e-12);
            // override is exactly cdf(-x)
            assert_eq!(dist.survival(x), dist.cdf(-x));
        }
    }

    // VI. Inverse: endpoints, the symmetric centre, round trips, a known value.
    #[test]
    fn quantile_endpoints_are_support_bounds() {
        let dist = StudentT::new(5.0).unwrap();
        assert_eq!(
            dist.quantile(Probability::try_from(0.0).unwrap()).unwrap(),
            Real::NEG_INFINITY
        );
        assert_eq!(
            dist.quantile(Probability::try_from(1.0).unwrap()).unwrap(),
            Real::INFINITY
        );
    }

    #[test]
    fn quantile_of_half_is_zero() {
        for nu in [1.0, 4.0, 40.0] {
            let dist = StudentT::new(nu).unwrap();
            let x = dist.quantile(Probability::try_from(0.5).unwrap()).unwrap();
            assert_close(x, 0.0, 1e-12);
        }
    }

    #[test]
    fn quantile_round_trips_with_cdf() {
        for nu in [1.0, 3.0, 10.0, 100.0] {
            let dist = StudentT::new(nu).unwrap();
            for p in [0.05, 0.25, 0.5, 0.75, 0.95] {
                let x = dist.quantile(Probability::try_from(p).unwrap()).unwrap();
                assert_close(dist.cdf(x), p, 1e-6);
            }
        }
    }

    #[test]
    fn quantile_matches_cauchy_known_value() {
        // Cauchy: F(1) = 0.75, so the 0.75 quantile is 1.
        let dist = StudentT::new(1.0).unwrap();
        let x = dist.quantile(Probability::try_from(0.75).unwrap()).unwrap();
        assert_close(x, 1.0, 1e-6);
    }

    // VII. Construction validation.
    #[test]
    fn new_rejects_nonpositive_nan_and_infinite_dof() {
        assert!(StudentT::new(0.0).is_err());
        assert!(StudentT::new(-2.0).is_err());
        assert!(StudentT::new(Real::NAN).is_err());
        // Non-finite dof would make ln_normalization NaN and panic cdf's expect.
        assert!(StudentT::new(Real::INFINITY).is_err());
        assert!(StudentT::new(Real::NEG_INFINITY).is_err());
    }
}
