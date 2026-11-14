//! Student's t-distribution.
//!
//! Port of `ql/math/distributions/studenttdistribution.{hpp,cpp}`: the standard
//! (location 0, scale 1) Student-t [`StudentT`] with `ν` degrees of freedom,
//! providing the density, the cumulative distribution via the regularized
//! [`incomplete_beta`] function, and the inverse via Boost's Student-t quantile
//! that QuantLib v1.44's `InverseCumulativeStudent` calls.

use std::f64::consts::PI;

use super::{Cdf, Density, Probability, Quantile, Support};
use crate::errors::QlResult;
use crate::fail;
use crate::math::beta::incomplete_beta;
use crate::math::gammafunction::log_gamma;
use crate::require;
use crate::types::Real;

mod inverse;

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

impl StudentT {
    /// One Halley step on the cdf from a lower-tail estimate `t < 0` at
    /// `u <= 0.5`, as Boost's `fast_students_t_quantile_imp`.
    ///
    /// The residual `p0 = cdf(t) - u` is the forward `I_x(ν/2, 1/2) / 2 - u`
    /// in `x = ν/(ν+t²)` when `y = t²/(ν+t²) >= 3/(ν+5)`, else, near the
    /// centre where `I_y` is small and the forward form cancels,
    /// `(0.5 - u) - I_y(1/2, ν/2) / 2`.
    /// Boost switches at `x < y` and evaluates the complement
    /// `f0 = 1 - I_y(1/2, ν/2)` to full precision inside its ibeta; this
    /// [`incomplete_beta`] has no complemented entry point, so `p0 = f0/2 - u`
    /// is rearranged to avoid the subtraction and used only where `I_y` is
    /// small (Boost's switch would cancel in the large-`ν` tails, where
    /// `t² < ν` but `I_y` is near `1`). The pdf stands in for Boost's ibeta
    /// derivative.
    ///
    /// The step ratio `p0 / pdf(t)` is formed in log space, so it stays
    /// defined where the density is subnormal (the deep tail); only Boost's
    /// `x == 0` bail-out returns the estimate unrefined.
    fn halley_step(&self, t: Real, u: Real) -> QlResult<Real> {
        let nu = self.degrees_of_freedom;
        let t2 = t * t;
        let xb = nu / (nu + t2);
        if xb == 0.0 {
            return Ok(t);
        }
        let y = t2 / (nu + t2);
        let p0 = if y * (nu + 5.0) >= 3.0 {
            0.5 * incomplete_beta(0.5 * nu, 0.5, xb)? - u
        } else {
            (0.5 - u) - 0.5 * incomplete_beta(0.5, 0.5 * nu, y)?
        };
        let r = p0.signum() * (p0.abs().ln() - self.ln_pdf(t)).exp();
        let p2 = t * (nu + 1.0) / (t2 + nu);
        Ok(-(t.abs() + r / (1.0 + r * p2 / 2.0)))
    }

    /// Newton-Raphson from `x = 0` with QuantLib v1.43's defaults (accuracy
    /// `1e-6`, 50 iterations), kept for non-integer `ν < 2` until #1452 ports
    /// Boost's inverse incomplete beta for that domain.
    fn newton_quantile(&self, p: Real) -> QlResult<Real> {
        const ACCURACY: Real = 1e-6;
        const MAX_ITERATIONS: u32 = 50;
        let mut x = 0.0;
        for _ in 0..MAX_ITERATIONS {
            let diff = (self.cdf(x) - p) / self.pdf(x);
            x -= diff;
            if diff.abs() <= ACCURACY {
                return Ok(x);
            }
        }
        fail!(
            "Student-t quantile did not converge for p={p} (df={})",
            self.degrees_of_freedom
        );
    }
}

impl Quantile for StudentT {
    /// The quantile by Boost's `students_t` quantile, which QuantLib v1.44's
    /// `InverseCumulativeStudent` calls: the `inverse_students_t` estimate for the lower
    /// tail `u = min(p, 1 - p)`, refined by one Halley step unless the estimate
    /// is exact, and reflected for `p > 0.5`.
    ///
    /// `quantile(0)` is `-∞` and `quantile(1)` is `+∞`, a deliberate divergence
    /// from v1.44's `QL_MIN_REAL` / `QL_MAX_REAL`: the generalized inverse maps
    /// the closed endpoints to the support. `quantile(0.5)` is `0`, as in
    /// v1.44's `InverseCumulativeStudent`. Non-integer `ν < 2` keeps the
    /// v1.43 Newton iteration (tracked as #1452).
    ///
    /// # Errors
    ///
    /// Returns an error if the incomplete beta of the Halley step does not
    /// converge, or if the non-integer `ν < 2` Newton iteration exhausts its
    /// budget.
    fn quantile(&self, p: Probability) -> QlResult<Real> {
        let p = p.value();
        if p == 0.0 {
            return Ok(self.lower_bound());
        }
        if p == 1.0 {
            return Ok(self.upper_bound());
        }
        if p == 0.5 {
            return Ok(0.0);
        }
        let nu = self.degrees_of_freedom;
        if nu < 2.0 && nu.floor() != nu {
            return self.newton_quantile(p);
        }
        let (u, invert) = if p > 0.5 { (1.0 - p, true) } else { (p, false) };
        let (estimate, exact) = inverse::inverse_students_t(nu, self.ln_normalization, u)?;
        let t = if exact || estimate == 0.0 {
            estimate
        } else {
            self.halley_step(estimate, u)?
        };
        Ok(if invert { -t } else { t })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::check_close;

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
        let dist = StudentT::new(4.0).unwrap();
        assert!(Probability::try_from(-0.1).is_err());
        assert!(Probability::try_from(1.1).is_err());
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

    fn q(dist: &StudentT, p: Real) -> Real {
        dist.quantile(Probability::try_from(p).unwrap()).unwrap()
    }

    const ORACLE_PROBABILITIES: [Real; 17] = [
        1.0e-12,
        1.0e-9,
        1.0e-6,
        1.0e-4,
        0.001,
        0.01,
        0.10,
        0.25,
        0.50,
        0.75,
        0.90,
        0.99,
        0.999,
        1.0 - 1.0e-4,
        1.0 - 1.0e-6,
        1.0 - 1.0e-9,
        1.0 - 1.0e-12,
    ];

    #[test]
    fn inverse_cumulative_cauchy_matches_closed_form() {
        let dist = StudentT::new(1.0).unwrap();
        for p in ORACLE_PROBABILITIES {
            let expected = if p < 0.5 {
                -1.0 / (PI * p).tan()
            } else if p > 0.5 {
                1.0 / (PI * (1.0 - p)).tan()
            } else {
                0.0
            };
            let calculated = q(&dist, p);
            let tolerance = 1.0e-10 * expected.abs().max(1.0);
            assert!(
                (calculated - expected).abs() <= tolerance,
                "p={p}: calculated {calculated}, expected {expected}"
            );
        }
    }

    #[test]
    fn inverse_cumulative_student2_matches_closed_form() {
        let dist = StudentT::new(2.0).unwrap();
        for p in ORACLE_PROBABILITIES {
            let expected = (2.0 * p - 1.0) / (2.0 * p * (1.0 - p)).sqrt();
            let calculated = q(&dist, p);
            let tolerance = 1.0e-12 * expected.abs().max(1.0);
            assert!(
                (calculated - expected).abs() <= tolerance,
                "p={p}: calculated {calculated}, expected {expected}"
            );
        }
    }

    #[test]
    fn inverse_cumulative_student_round_trips_monotone_and_symmetric() {
        let round_trip = [
            1.0e-10,
            1.0e-8,
            1.0e-6,
            1.0e-4,
            0.001,
            0.01,
            0.10,
            0.25,
            0.50,
            0.75,
            0.90,
            0.99,
            0.999,
            1.0 - 1.0e-4,
            1.0 - 1.0e-6,
            1.0 - 1.0e-8,
            1.0 - 1.0e-10,
        ];
        let symmetry = [1.0e-8, 1.0e-6, 1.0e-4, 0.001, 0.01, 0.10, 0.25];
        for n in [3.0, 4.0, 5.0, 10.0, 30.0, 100.0] {
            let dist = StudentT::new(n).unwrap();
            let mut previous = Real::MIN;
            for p in round_trip {
                let x = q(&dist, p);
                assert!(x > previous, "n={n}, p={p}: {x} <= {previous}");
                previous = x;
                assert_close(dist.cdf(x), p, 1.0e-8);
            }
            for p in symmetry {
                let lower = q(&dist, p);
                let upper = q(&dist, 1.0 - p);
                let tolerance = 1.0e-8 * lower.abs().max(upper.abs()).max(1.0);
                assert!(
                    (lower + upper).abs() <= tolerance,
                    "n={n}, p={p}: {lower} vs {upper}"
                );
            }
        }
    }

    #[test]
    fn deep_tail_quantiles_stay_finite_when_the_density_underflows() {
        for nu in [2.5, 3.0, 5.0, 10.0, 30.0] {
            let dist = StudentT::new(nu).unwrap();
            for p in [1.0e-250, 1.0e-300, 5.0e-324] {
                let x = q(&dist, p);
                assert!(x.is_finite() && x < 0.0, "nu={nu}, p={p}: {x}");
                if p >= 1.0e-300 {
                    assert_close(dist.cdf(x) / p, 1.0, 1.0e-6);
                }
            }
        }
    }

    #[test]
    fn deep_tail_quantiles_are_strictly_increasing_across_the_old_guard() {
        let (lo, hi) = (1.0e-320_f64.ln(), 1.0e-200_f64.ln());
        for nu in [3.0, 5.0, 10.0, 30.0, 100.0, 1000.0] {
            let dist = StudentT::new(nu).unwrap();
            let mut previous = Real::NEG_INFINITY;
            for k in 0..200 {
                let p = (lo + (hi - lo) * Real::from(k) / 199.0).exp();
                let x = q(&dist, p);
                assert!(x.is_finite() && x < 0.0, "nu={nu}, p={p}: {x}");
                assert!(x > previous, "nu={nu}, p={p}: {x} <= {previous}");
                previous = x;
            }
        }
    }

    #[test]
    fn huge_dof_normal_quantile_holds_the_acklam_bound() {
        let dist = StudentT::new(1.0e20).unwrap();
        for (p, expected) in [
            (1.0e-10, -6.361_340_902_404_057),
            (0.01, -2.326_347_874_040_841),
            (0.3, -0.524_400_512_708_040_8),
        ] {
            let x = q(&dist, p);
            assert!(
                ((x - expected) / expected).abs() <= 1.15e-9,
                "p={p}: {x} vs {expected}"
            );
        }
    }

    #[test]
    fn non_integer_dof_below_two_keeps_newton() {
        let dist = StudentT::new(1.5).unwrap();
        for p in [0.05, 0.25, 0.75, 0.95] {
            assert_close(dist.cdf(q(&dist, p)), p, 1.0e-6);
        }
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

    /// QuantLib v1.44 `testStudentDistribution` (`distributions.cpp:839`):
    /// `StudentDistribution` and `CumulativeStudentDistribution` are both
    /// [`StudentT`].
    #[test]
    fn v144_student_distribution() {
        let cauchy = StudentT::new(1.0).unwrap();
        check_close(cauchy.pdf(0.0), 1.0 / PI, 1.0e-10);
        check_close(cauchy.pdf(1.0), 0.5 / PI, 1.0e-10);

        let student = StudentT::new(5.0).unwrap();
        check_close(student.pdf(-1.0), student.pdf(1.0), 1.0e-12);
        check_close(student.cdf(0.0), 0.5, 1.0e-12);
        check_close(student.cdf(-1.0), 1.0 - student.cdf(1.0), 1.0e-12);

        assert!(StudentT::new(0.0).is_err());
    }
}
