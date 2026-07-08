//! Adaptive Simpson integration.
//!
//! Port of `SimpsonIntegral` from `ql/math/integrals/simpsonintegral.hpp`: the
//! same interval-doubling refinement as the default trapezoid rule, but each
//! estimate is Richardson-extrapolated (`(4*newI - I)/3`) to Simpson order, and
//! convergence is judged on the extrapolated values.

use crate::errors::QlResult;
use crate::fail;
use crate::math::integrals::trapezoid::trapezoid_step;
use crate::math::integrals::{Integrator, require_accuracy};
use crate::types::{Real, Size};

/// Adaptive Simpson integrator with a target accuracy and iteration budget.
pub struct SimpsonIntegral {
    accuracy: Real,
    max_iterations: Size,
}

impl SimpsonIntegral {
    /// A Simpson integrator. `accuracy` must be finite and above machine epsilon.
    pub fn new(accuracy: Real, max_iterations: Size) -> QlResult<Self> {
        require_accuracy(accuracy)?;
        Ok(SimpsonIntegral {
            accuracy,
            max_iterations,
        })
    }
}

impl Integrator for SimpsonIntegral {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let mut n: Size = 1;
        let mut trapezoid = (f(a) + f(b)) * (b - a) / 2.0;
        let mut simpson = trapezoid;
        let mut i: Size = 1;
        loop {
            let refined = trapezoid_step(f, a, b, trapezoid, n);
            let refined_simpson = (4.0 * refined - trapezoid) / 3.0;
            // Convergence is judged on the extrapolated (Simpson) estimates.
            if (simpson - refined_simpson).abs() <= self.accuracy && i > 5 {
                return Ok(refined_simpson);
            }
            let Some(next_n) = n.checked_mul(2) else {
                fail!("Simpson integration did not converge before the grid overflowed");
            };
            n = next_n;
            trapezoid = refined;
            simpson = refined_simpson;
            i += 1;
            if i >= self.max_iterations {
                break;
            }
        }
        fail!(
            "Simpson integration did not converge within {} iterations",
            self.max_iterations
        );
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
        let s = SimpsonIntegral::new(TOL, 10_000).unwrap();
        assert!((s.integrate(|_| 1.0, 0.0, 1.0).unwrap() - 1.0).abs() < TOL);
        assert!((s.integrate(|x| x, 0.0, 1.0).unwrap() - 0.5).abs() < TOL);
        assert!((s.integrate(|x| x * x, 0.0, 1.0).unwrap() - 1.0 / 3.0).abs() < TOL);
        assert!((s.integrate(|x| x.sin(), 0.0, std::f64::consts::PI).unwrap() - 2.0).abs() < TOL);
        assert!((s.integrate(|x| x.cos(), 0.0, std::f64::consts::PI).unwrap() - 0.0).abs() < TOL);
        let g = NormalDistribution::standard();
        assert!((s.integrate(|x| g.value(x), -10.0, 10.0).unwrap() - 1.0).abs() < TOL);
        // testDegeneratedDomain.
        assert_eq!(s.integrate(|_| 0.0, 1.0, 1.0 + Real::EPSILON).unwrap(), 0.0);
    }

    #[test]
    fn too_few_iterations_fails_to_converge() {
        let s = SimpsonIntegral::new(TOL, 3).unwrap();
        assert!(s.integrate(|x| x.sin(), 0.0, std::f64::consts::PI).is_err());
    }

    #[test]
    fn invalid_accuracy_rejected() {
        for acc in [0.0, -1.0, Real::EPSILON, Real::NAN, Real::INFINITY] {
            assert!(SimpsonIntegral::new(acc, 10_000).is_err(), "accuracy={acc}");
        }
    }
}
