//! Adaptive trapezoid integration.
//!
//! Port of `TrapezoidIntegral` from `ql/math/integrals/trapezoidintegral.hpp`:
//! the interval count is repeatedly refined until successive estimates agree to
//! the requested accuracy, or the iteration budget is exhausted. The `Default`
//! and `MidPoint` refinement policies are both supported.

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::{Integrator, require_accuracy};
use crate::types::{Real, Size};

/// Refinement policy for the adaptive trapezoid rule. Private: the two modes are
/// exposed through [`TrapezoidIntegral::new`] and [`TrapezoidIntegral::midpoint`].
#[derive(Clone, Copy)]
enum TrapezoidPolicy {
    /// Standard trapezoid refinement (adds the `N` midpoints each step).
    Default,
    /// Mid-point refinement (adds `2N` interior points each step).
    MidPoint,
}

impl TrapezoidPolicy {
    /// The factor by which the interval count grows per refinement.
    fn refinement_factor(self) -> Size {
        match self {
            TrapezoidPolicy::Default => 2,
            TrapezoidPolicy::MidPoint => 3,
        }
    }

    /// One refinement step, combining the previous estimate `prev` with the new
    /// nodes introduced by subdividing each of the `n` current intervals.
    fn refine<F: FnMut(Real) -> Real>(
        self,
        f: &mut F,
        a: Real,
        b: Real,
        prev: Real,
        n: Size,
    ) -> Real {
        match self {
            TrapezoidPolicy::Default => trapezoid_step(f, a, b, prev, n),
            TrapezoidPolicy::MidPoint => {
                let dx = (b - a) / n as Real;
                let d = 2.0 * dx / 3.0;
                let mut x = a + dx / 6.0;
                let mut sum = 0.0;
                for _ in 0..n {
                    sum += f(x) + f(x + d);
                    x += dx;
                }
                (prev + dx * sum) / 3.0
            }
        }
    }
}

/// One standard trapezoid refinement: `prev` combined with the `n` midpoints of
/// the current grid. Shared with [`SimpsonIntegral`](super::simpson).
pub(super) fn trapezoid_step<F: FnMut(Real) -> Real>(
    f: &mut F,
    a: Real,
    b: Real,
    prev: Real,
    n: Size,
) -> Real {
    let dx = (b - a) / n as Real;
    let mut x = a + dx / 2.0;
    let mut sum = 0.0;
    for _ in 0..n {
        sum += f(x);
        x += dx;
    }
    (prev + dx * sum) / 2.0
}

/// Adaptive trapezoid integrator with a target accuracy and iteration budget.
pub struct TrapezoidIntegral {
    accuracy: Real,
    max_iterations: Size,
    policy: TrapezoidPolicy,
}

impl TrapezoidIntegral {
    /// A standard adaptive trapezoid integrator. `accuracy` must be finite and
    /// above machine epsilon.
    pub fn new(accuracy: Real, max_iterations: Size) -> QlResult<Self> {
        Self::with_policy(accuracy, max_iterations, TrapezoidPolicy::Default)
    }

    /// A mid-point adaptive trapezoid integrator.
    pub fn midpoint(accuracy: Real, max_iterations: Size) -> QlResult<Self> {
        Self::with_policy(accuracy, max_iterations, TrapezoidPolicy::MidPoint)
    }

    fn with_policy(
        accuracy: Real,
        max_iterations: Size,
        policy: TrapezoidPolicy,
    ) -> QlResult<Self> {
        require_accuracy(accuracy)?;
        Ok(TrapezoidIntegral {
            accuracy,
            max_iterations,
            policy,
        })
    }
}

impl Integrator for TrapezoidIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let mut n: Size = 1;
        let mut estimate = (f(a) + f(b)) * (b - a) / 2.0;
        let mut i: Size = 1;
        loop {
            let refined = self.policy.refine(f, a, b, estimate, n);
            // Don't accept before a few refinements (matching QuantLib's i > 5).
            if (estimate - refined).abs() <= self.accuracy && i > 5 {
                return Ok(refined);
            }
            let Some(next_n) = n.checked_mul(self.policy.refinement_factor()) else {
                fail!("trapezoid integration did not converge before the grid overflowed");
            };
            n = next_n;
            estimate = refined;
            i += 1;
            if i >= self.max_iterations {
                break;
            }
        }
        fail!(
            "trapezoid integration did not converge within {} iterations",
            self.max_iterations
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::distributions::normal::NormalDistribution;

    const TOL: Real = 1e-6;

    fn check(integ: &TrapezoidIntegral) {
        // QuantLib's testSeveral (Abcd case omitted, not yet ported).
        assert!((integ.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((integ.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((integ.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!(
            (integ
                .integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 2.0)
                .abs()
                < TOL
        );
        assert!(
            (integ
                .integrate(|x| x.cos(), 0.0, std::f64::consts::PI)
                .unwrap()
                - 0.0)
                .abs()
                < TOL
        );
        let g = NormalDistribution::standard();
        assert!((integ.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
        // testDegeneratedDomain.
        assert_eq!(
            integ.integrate(|_| 0.0, 1.0, 1.0 + Real::EPSILON).unwrap(),
            0.0
        );
    }

    #[test]
    fn default_policy_matches_known_integrals() {
        check(&TrapezoidIntegral::new(TOL, 10_000).unwrap());
    }

    #[test]
    fn midpoint_policy_matches_known_integrals() {
        check(&TrapezoidIntegral::midpoint(TOL, 10_000).unwrap());
    }

    #[test]
    fn too_few_iterations_fails_to_converge() {
        // The i > 5 guard means a budget below 6 iterations can never converge.
        let integ = TrapezoidIntegral::new(TOL, 3).unwrap();
        assert!(
            integ
                .integrate(|x| x.sin(), 0.0, std::f64::consts::PI)
                .is_err()
        );
    }

    #[test]
    fn invalid_accuracy_rejected() {
        for acc in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(
                TrapezoidIntegral::new(acc, 10_000).is_err(),
                "accuracy={acc}"
            );
        }
    }
}
