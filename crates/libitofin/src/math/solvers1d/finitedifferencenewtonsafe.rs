//! Finite-difference Newton-safe 1-D solver.
//!
//! Port of `ql/math/solvers1d/finitedifferencenewtonsafe.hpp`: the safe-Newton
//! (`rtsafe`) algorithm, but with the derivative estimated by finite differences
//! rather than supplied. It therefore needs only the function value, so it is a
//! plain [`Solver1D`] like Bisection or Brent. Following Press et al., "Numerical
//! Recipes in C", 2nd ed.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close_n;
use crate::math::solver1d::{Solver1D, Solver1DState, SolverConfig, checked_value};
use crate::types::Real;

/// Finite-difference Newton-safe root finder.
#[derive(Clone, Copy, Debug)]
pub struct FiniteDifferenceNewtonSafe {
    config: SolverConfig,
}

impl FiniteDifferenceNewtonSafe {
    /// A finite-difference Newton-safe solver with the default configuration.
    pub fn new() -> Self {
        FiniteDifferenceNewtonSafe {
            config: SolverConfig::new(),
        }
    }

    /// Set the evaluation cap (builder form).
    pub fn with_max_evaluations(mut self, evaluations: usize) -> Self {
        self.config.max_evaluations = evaluations;
        self
    }

    /// Restrict the search to `x >= lower_bound` (builder form).
    pub fn with_lower_bound(mut self, lower_bound: Real) -> Self {
        self.config.lower_bound = Some(lower_bound);
        self
    }

    /// Restrict the search to `x <= upper_bound` (builder form).
    pub fn with_upper_bound(mut self, upper_bound: Real) -> Self {
        self.config.upper_bound = Some(upper_bound);
        self
    }
}

impl Default for FiniteDifferenceNewtonSafe {
    fn default() -> Self {
        FiniteDifferenceNewtonSafe::new()
    }
}

impl Solver1D for FiniteDifferenceNewtonSafe {
    fn config(&self) -> &SolverConfig {
        &self.config
    }

    fn config_mut(&mut self) -> &mut SolverConfig {
        &mut self.config
    }

    fn solve_impl<F>(
        &mut self,
        f: &mut F,
        x_accuracy: Real,
        st: &mut Solver1DState,
    ) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        // Orient so f(xl) < 0.
        let (mut xl, mut xh) = if st.fx_min < 0.0 {
            (st.x_min, st.x_max)
        } else {
            (st.x_max, st.x_min)
        };

        let mut froot = checked_value(f, st.root)?;
        st.evaluation_number += 1;
        // First-order finite-difference derivative, taken from the closer end.
        let mut dfroot = if st.x_max - st.root < st.root - st.x_min {
            (st.fx_max - froot) / (st.x_max - st.root)
        } else {
            (st.fx_min - froot) / (st.x_min - st.root)
        };
        let mut dx = st.x_max - st.x_min;

        while st.evaluation_number <= self.config.max_evaluations {
            let mut frootold = froot;
            let mut rootold = st.root;
            let dxold = dx;
            // Bisect if the Newton step would leave [xl, xh] or is not shrinking
            // the bracket fast enough; otherwise take the (finite-difference)
            // Newton step.
            if ((st.root - xh) * dfroot - froot) * ((st.root - xl) * dfroot - froot) > 0.0
                || (2.0 * froot).abs() > (dxold * dfroot).abs()
            {
                dx = 0.5 * (xh - xl);
                st.root = xl + dx;
                // If the bisection landed essentially on the previous root, take
                // the difference against xh instead so the secant stays defined.
                if close_n(st.root, rootold, 2500) {
                    rootold = xh;
                    frootold = checked_value(f, xh)?;
                }
            } else {
                dx = froot / dfroot;
                st.root -= dx;
            }
            if dx.abs() < x_accuracy {
                return Ok(st.root);
            }
            froot = checked_value(f, st.root)?;
            st.evaluation_number += 1;
            // Secant-style derivative against the previous point.
            dfroot = (frootold - froot) / (rootold - st.root);
            if froot < 0.0 {
                xl = st.root;
            } else {
                xh = st.root;
            }
        }
        fail!(
            "maximum number of function evaluations ({}) exceeded",
            self.config.max_evaluations
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::solvers1d::testkit;

    #[test]
    fn finds_known_roots() {
        testkit::check_finds_known_roots(FiniteDifferenceNewtonSafe::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_rejects_invalid_inputs(FiniteDifferenceNewtonSafe::new);
    }

    #[test]
    fn honours_configured_bounds() {
        testkit::check_honours_bounds(FiniteDifferenceNewtonSafe::new);
    }

    // No `last_call_with_root` check: the convergence return does not re-evaluate
    // the final root, so the last function call is not at the returned value
    // (QuantLib skips this test for this solver too, via a null accuracy).
}
