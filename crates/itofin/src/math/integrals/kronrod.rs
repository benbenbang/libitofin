//! Gauss-Kronrod integration.
//!
//! Port of `ql/math/integrals/kronrodintegral.{hpp,cpp}`. This module currently
//! provides the adaptive 15-point Gauss-Kronrod integrator; the non-adaptive
//! (10/21/43/87-point) variant lands in a follow-up.

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::{Integrator, require_accuracy};
use crate::types::{Real, Size};

// 7-point Gauss-Legendre weights (4 unique values; the rule is symmetric).
const G7W: [Real; 4] = [
    0.417959183673469,
    0.381830050505119,
    0.279705391489277,
    0.129484966168870,
];
// 15-point Gauss-Kronrod weights (8 unique values).
const K15W: [Real; 8] = [
    0.209482141084728,
    0.204432940075298,
    0.190350578064785,
    0.169004726639267,
    0.140653259715525,
    0.104790010322250,
    0.063092092629979,
    0.022935322010529,
];
// 15-point Gauss-Kronrod abscissae (8 unique values, scaled to [-1, 1]).
const K15T: [Real; 8] = [
    0.000000000000000,
    0.207784955007898,
    0.405845151377397,
    0.586087235467691,
    0.741531185599394,
    0.864864423359769,
    0.949107912342758,
    0.991455371120813,
];

/// Adaptive Gauss-Kronrod integrator using the 15-point rule with recursive
/// bisection. Robust for less-smooth integrands, but it does not reuse points
/// between refinement levels.
pub struct GaussKronrodAdaptive {
    tolerance: Real,
    max_evaluations: Size,
}

impl GaussKronrodAdaptive {
    /// A new adaptive integrator. `tolerance` must be finite and above machine
    /// epsilon, and `max_evaluations` must be at least 15 (one 15-point rule).
    pub fn new(tolerance: Real, max_evaluations: Size) -> QlResult<Self> {
        require_accuracy(tolerance)?;
        if max_evaluations < 15 {
            fail!("required max evaluations ({max_evaluations}) must be >= 15");
        }
        Ok(GaussKronrodAdaptive {
            tolerance,
            max_evaluations,
        })
    }

    /// Integrates `f` over `[a, b]` with the 15-point rule; if the Gauss(7) and
    /// Kronrod(15) estimates disagree by more than `tolerance`, the interval is
    /// bisected and each half integrated at half the tolerance. `evaluations`
    /// accumulates the running count across the whole recursion.
    fn integrate_recursively<F>(
        &self,
        f: &mut F,
        a: Real,
        b: Real,
        tolerance: Real,
        evaluations: &mut Size,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let halflength = (b - a) / 2.0;
        let center = (a + b) / 2.0;

        let fc = f(center);
        let mut g7 = fc * G7W[0];
        let mut k15 = fc * K15W[0];

        // The Gauss nodes are the even-indexed Kronrod nodes; accumulate g7 and
        // its share of k15 together (j2 = 2, 4, 6 alongside the Gauss weights).
        let mut j2 = 2;
        for &g7w in G7W.iter().skip(1) {
            let t = halflength * K15T[j2];
            let fsum = f(center - t) + f(center + t);
            g7 += fsum * g7w;
            k15 += fsum * K15W[j2];
            j2 += 2;
        }
        // The remaining odd-indexed Kronrod-only nodes.
        let mut j2 = 1;
        while j2 < 8 {
            let t = halflength * K15T[j2];
            let fsum = f(center - t) + f(center + t);
            k15 += fsum * K15W[j2];
            j2 += 2;
        }

        g7 *= halflength;
        k15 *= halflength;
        *evaluations += 15;

        // The error is bounded by |K15 - G7|; refine if it exceeds the tolerance.
        if (k15 - g7).abs() < tolerance {
            Ok(k15)
        } else {
            if *evaluations + 30 > self.max_evaluations {
                fail!("maximum number of function evaluations exceeded");
            }
            let left = self.integrate_recursively(f, a, center, tolerance / 2.0, evaluations)?;
            let right = self.integrate_recursively(f, center, b, tolerance / 2.0, evaluations)?;
            Ok(left + right)
        }
    }
}

impl Integrator for GaussKronrodAdaptive {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let mut evaluations = 0;
        self.integrate_recursively(f, a, b, self.tolerance, &mut evaluations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    #[test]
    fn matches_known_integrals() {
        // QuantLib's testSeveral (Abcd case omitted, not yet ported).
        let gk = GaussKronrodAdaptive::new(TOL, 1000).unwrap();
        assert!((gk.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((gk.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((gk.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!(
            (gk.integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 2.0)
                .abs()
                < TOL
        );
        assert!(
            (gk.integrate(|x| x.cos(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 0.0)
                .abs()
                < TOL
        );
        let g = NormalDistribution::standard();
        assert!((gk.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
        // testDegeneratedDomain.
        assert_eq!(
            gk.integrate(|_| 0.0, 1.0, 1.0 + Real::EPSILON).unwrap(),
            0.0
        );
    }

    #[test]
    fn too_small_budget_fails_to_converge() {
        // A tight tolerance on an oscillatory integrand exhausts a 15-evaluation
        // budget (one rule, no room to bisect).
        let gk = GaussKronrodAdaptive::new(1e-13, 15).unwrap();
        assert!(gk.integrate(|x| (50.0 * x).sin(), 0.0, 1.0).is_err());
    }

    #[test]
    fn invalid_configuration_rejected() {
        for acc in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(
                GaussKronrodAdaptive::new(acc, 1000).is_err(),
                "accuracy={acc}"
            );
        }
        // Fewer than 15 evaluations cannot fit a single rule.
        assert!(GaussKronrodAdaptive::new(TOL, 14).is_err());
    }
}
