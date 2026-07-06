//! Tanh-sinh double-exponential integration.
//!
//! Port of `TanhSinhIntegral` from `ql/math/integrals/tanhsinhintegral.hpp`.
//! QuantLib delegates the work to `boost::math::quadrature::tanh_sinh`, which
//! has no Rust equivalent, so the double-exponential scheme itself is
//! implemented here: the Takahasi-Mori transform `x = tanh(pi/2 sinh t)` turns
//! the integral over `[-1, 1]` into a doubly-exponentially decaying integral
//! over the real line, which the trapezoidal rule with successive halving
//! evaluates to rapidly increasing accuracy for holomorphic integrands, and
//! robustly for integrable endpoint singularities. As in Boost, the tolerance
//! is used against the error estimate for the L1 norm of the integral.

use std::f64::consts::{FRAC_PI_2, PI};

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::{Integrator, de_quadrature, require_accuracy};
use crate::types::{Real, Size};

/// Tanh-sinh quadrature over finite intervals.
///
/// Nodes whose distance to an endpoint of the unit interval falls below the
/// minimum complement, or whose mapped abscissa rounds onto an endpoint of
/// `[a, b]`, are truncated, so the integrand is never evaluated at `a` or `b`
/// themselves.
pub struct TanhSinhIntegral {
    rel_tolerance: Real,
    max_refinements: Size,
    min_complement: Real,
}

impl TanhSinhIntegral {
    /// QuantLib's defaults: relative tolerance `sqrt(eps)`, at most 15
    /// refinements, and a minimum endpoint complement of four times the
    /// smallest positive normal.
    pub fn new() -> Self {
        TanhSinhIntegral {
            rel_tolerance: Real::EPSILON.sqrt(),
            max_refinements: 15,
            min_complement: 4.0 * Real::MIN_POSITIVE,
        }
    }

    /// A tanh-sinh integrator with explicit parameters.
    ///
    /// # Errors
    ///
    /// Returns an error unless `rel_tolerance` is finite and above machine
    /// epsilon and `min_complement` lies in `[minimum normal, 1)`.
    pub fn with_params(
        rel_tolerance: Real,
        max_refinements: Size,
        min_complement: Real,
    ) -> QlResult<Self> {
        require_accuracy(rel_tolerance)?;
        if !min_complement.is_finite() || !(Real::MIN_POSITIVE..1.0).contains(&min_complement) {
            fail!(
                "minimum complement ({min_complement}) must lie between the smallest positive normal and 1"
            );
        }
        Ok(TanhSinhIntegral {
            rel_tolerance,
            max_refinements,
            min_complement,
        })
    }
}

impl Default for TanhSinhIntegral {
    fn default() -> Self {
        TanhSinhIntegral::new()
    }
}

impl Integrator for TanhSinhIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let center = 0.5 * (a + b);
        let half_width = 0.5 * (b - a);
        let min_complement = self.min_complement;
        // With e = exp(-2|u|), the unit-interval complement 1 - |x| equals
        // 2e / (1 + e) and sech^2(u) equals 4e / (1 + e)^2, both stable where
        // cosh^2(u) itself would overflow.
        let node = move |t: Real| {
            let u = FRAC_PI_2 * t.sinh();
            let e = (-2.0 * u.abs()).exp();
            if 2.0 * e < min_complement * (1.0 + e) {
                return None;
            }
            let x = center + half_width * u.tanh();
            if x <= a || x >= b {
                return None;
            }
            let w = FRAC_PI_2 * t.cosh() * 4.0 * e / ((1.0 + e) * (1.0 + e));
            Some((x, half_width * w))
        };
        let t_max = ((2.0 / min_complement).ln() / PI).asinh();
        de_quadrature(f, node, t_max, self.rel_tolerance, self.max_refinements)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    #[test]
    fn matches_known_integrals() {
        // QuantLib's testTanhSinh runs testSeveral (Abcd case omitted, not yet
        // ported), tolerance 1e-6.
        let ts = TanhSinhIntegral::new();
        assert!((ts.integrate(|_| 0.0, 0.0, 1.0).unwrap() - 0.0).abs() < TOL);
        assert!((ts.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((ts.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((ts.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!(
            (ts.integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 2.0)
                .abs()
                < TOL
        );
        assert!(
            (ts.integrate(|x| x.cos(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 0.0)
                .abs()
                < TOL
        );
        let g = NormalDistribution::standard();
        assert!((ts.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
    }

    #[test]
    fn handles_endpoint_singularities() {
        // The double-exponential transform's signature strength: integrable
        // singularities at the endpoints, which the integrand never sees.
        let ts = TanhSinhIntegral::new();
        assert!((ts.integrate(|x| 1.0 / x.sqrt(), 0.0, 1.0).unwrap() - 2.0).abs() < TOL);
        assert!((ts.integrate(|x| x.ln(), 0.0, 1.0).unwrap() - (-1.0)).abs() < TOL);
    }

    #[test]
    fn reversed_limits_negate() {
        let ts = TanhSinhIntegral::new();
        assert!((ts.integrate(|x| x, 1.0, 0.0).unwrap() - (-0.5)).abs() < TOL);
    }

    #[test]
    fn reports_non_convergence() {
        // A discontinuity in the interior defeats the DE error model; with the
        // refinement budget cut to two levels the driver must report failure
        // rather than return the stalled estimate.
        let ts = TanhSinhIntegral::with_params(1e-12, 2, 4.0 * Real::MIN_POSITIVE).unwrap();
        assert!(
            ts.integrate(|x| if x < 1.0 / 3.0 { 0.0 } else { 1.0 }, 0.0, 1.0)
                .is_err()
        );
    }

    #[test]
    fn rejects_non_finite_integrand_values() {
        let ts = TanhSinhIntegral::new();
        assert!(ts.integrate(|x| (x - 0.5).recip(), 0.0, 1.0).is_err());
    }

    #[test]
    fn invalid_configuration_rejected() {
        for tol in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(
                TanhSinhIntegral::with_params(tol, 15, 4.0 * Real::MIN_POSITIVE).is_err(),
                "tolerance {tol}"
            );
        }
        for mc in [0.0, -1.0, Real::MIN_POSITIVE / 2.0, 1.0, Real::NAN] {
            assert!(
                TanhSinhIntegral::with_params(1e-8, 15, mc).is_err(),
                "complement {mc}"
            );
        }
    }
}
