//! Exp-sinh double-exponential integration over semi-infinite domains.
//!
//! Port of `ExpSinhIntegral` from `ql/math/integrals/expsinhintegral.hpp`.
//! QuantLib delegates the work to `boost::math::quadrature::exp_sinh`, which
//! has no Rust equivalent, so the double-exponential scheme itself is
//! implemented here: the transform `x = exp(pi/2 sinh t)` turns an integral
//! over `[0, inf)` into a doubly-exponentially decaying integral over the real
//! line, evaluated by the trapezoidal rule with successive halving. As in
//! Boost, the tolerance is used against the error estimate for the L1 norm of
//! the integral, and a bound at or beyond `Real::MAX` in magnitude counts as
//! infinite.

use std::f64::consts::FRAC_PI_2;

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::{Integrator, de_quadrature, require_accuracy};
use crate::types::{Real, Size};

/// Past this |t| the abscissa `exp(pi/2 sinh t)` falls outside the positive
/// finite `f64` range on one side or the other, so every node is dropped by
/// the range clamp anyway.
const T_MAX: Real = 7.0;

/// Exp-sinh quadrature over `[a, inf)` or `(-inf, b]`.
///
/// Tail nodes whose abscissa or weight leaves the positive finite `f64` range
/// are truncated; their true contribution is below the underflow threshold for
/// any integrand the scheme converges on.
pub struct ExpSinhIntegral {
    rel_tolerance: Real,
    max_refinements: Size,
}

impl ExpSinhIntegral {
    /// QuantLib's defaults: relative tolerance `sqrt(eps)` and at most 9
    /// refinements.
    pub fn new() -> Self {
        ExpSinhIntegral {
            rel_tolerance: Real::EPSILON.sqrt(),
            max_refinements: 9,
        }
    }

    /// An exp-sinh integrator with explicit parameters.
    ///
    /// # Errors
    ///
    /// Returns an error unless `rel_tolerance` is finite and above machine
    /// epsilon.
    pub fn with_params(rel_tolerance: Real, max_refinements: Size) -> QlResult<Self> {
        require_accuracy(rel_tolerance)?;
        Ok(ExpSinhIntegral {
            rel_tolerance,
            max_refinements,
        })
    }

    /// Integrates `f` over `[0, inf)`, the transform's native domain; ports
    /// QuantLib's single-argument `integrate` overload.
    ///
    /// # Errors
    ///
    /// Returns an error when the integrand yields a non-finite value or the
    /// refinement budget is exhausted before the tolerance is met (as for a
    /// divergent integral).
    pub fn integrate_semi_infinite<F>(&self, mut f: F) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        self.semi_infinite(&mut f)
    }

    fn semi_infinite<F>(&self, f: &mut F) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let node = |t: Real| {
            let x = (FRAC_PI_2 * t.sinh()).exp();
            if x < Real::MIN_POSITIVE {
                return None;
            }
            let w = x * FRAC_PI_2 * t.cosh();
            if !w.is_finite() {
                return None;
            }
            Some((x, w))
        };
        de_quadrature(f, node, T_MAX, self.rel_tolerance, self.max_refinements)
    }
}

impl Default for ExpSinhIntegral {
    fn default() -> Self {
        ExpSinhIntegral::new()
    }
}

impl Integrator for ExpSinhIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        if a <= -Real::MAX && b >= Real::MAX {
            fail!(
                "doubly infinite domains require a sinh-sinh quadrature, which is not ported; got [{a}, {b}]"
            );
        }
        if b >= Real::MAX {
            self.semi_infinite(&mut |u: Real| f(a + u))
        } else if a <= -Real::MAX {
            self.semi_infinite(&mut |u: Real| f(b - u))
        } else {
            fail!("exp-sinh quadrature integrates semi-infinite domains only, got [{a}, {b}]");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    #[test]
    fn matches_known_integrals() {
        // QuantLib's testExpSinh, tolerance 1e-6: the standard normal density
        // and x * exp(-x) over [0, DBL_MAX].
        let integrator = ExpSinhIntegral::new();
        let g = NormalDistribution::standard();
        assert!(
            (integrator
                .integrate(|x| g.value(x), 0.0, Real::MAX)
                .unwrap()
                - 0.5)
                .abs()
                < TOL
        );
        assert!(
            (integrator
                .integrate(|x| x * (-x).exp(), 0.0, Real::MAX)
                .unwrap()
                - 1.0)
                .abs()
                < TOL
        );
    }

    #[test]
    fn native_semi_infinite_overload_matches() {
        let integrator = ExpSinhIntegral::new();
        let g = NormalDistribution::standard();
        assert!((integrator.integrate_semi_infinite(|x| g.value(x)).unwrap() - 0.5).abs() < TOL);
        assert!(
            (integrator
                .integrate_semi_infinite(|x| x * (-x).exp())
                .unwrap()
                - 1.0)
                .abs()
                < TOL
        );
    }

    #[test]
    fn shifted_and_reflected_domains() {
        let integrator = ExpSinhIntegral::new();
        assert!(
            (integrator
                .integrate(|x| (-(x - 1.0)).exp(), 1.0, Real::MAX)
                .unwrap()
                - 1.0)
                .abs()
                < TOL
        );
        assert!((integrator.integrate(|x| x.exp(), -Real::MAX, 0.0).unwrap() - 1.0).abs() < TOL);
    }

    #[test]
    fn rejects_unsupported_domains() {
        let integrator = ExpSinhIntegral::new();
        assert!(integrator.integrate(|x| x, 0.0, 1.0).is_err());
        assert!(
            integrator
                .integrate(|_| 0.0, -Real::MAX, Real::MAX)
                .is_err()
        );
    }

    #[test]
    fn reports_non_convergence_on_divergent_integrand() {
        let integrator = ExpSinhIntegral::new();
        assert!(
            integrator
                .integrate_semi_infinite(|x| 1.0 / (1.0 + x))
                .is_err()
        );
    }

    #[test]
    fn invalid_configuration_rejected() {
        for tol in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(
                ExpSinhIntegral::with_params(tol, 9).is_err(),
                "tolerance {tol}"
            );
        }
    }
}
