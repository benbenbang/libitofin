//! Newton 1-D solver.
//!
//! Port of `ql/math/solvers1d/newton.hpp`: Newton-Raphson stepping
//! `x -= f(x)/f'(x)` from the bracket midpoint. QuantLib hands off to NewtonSafe
//! when a step leaves the bracket; here - keeping Newton conceptually pure - we
//! instead return an explicit error, leaving safe bracketing to [`NewtonSafe`].
//! Following Press et al., "Numerical Recipes in C", 2nd ed.
//!
//! [`NewtonSafe`]: super
// (NewtonSafe is a later ticket; the link target is the module for now.)

use crate::errors::QlResult;
use crate::fail;
use crate::math::solver1d::{
    DerivativeSolver, Function1D, Solver1DState, SolverConfig, checked_derivative,
    checked_function_value,
};
use crate::types::Real;

/// Newton-Raphson root finder.
#[derive(Clone, Copy, Debug)]
pub struct Newton {
    config: SolverConfig,
}

impl Newton {
    /// A Newton solver with the default configuration.
    pub fn new() -> Self {
        Newton {
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

impl Default for Newton {
    fn default() -> Self {
        Newton::new()
    }
}

impl DerivativeSolver for Newton {
    fn config(&self) -> &SolverConfig {
        &self.config
    }

    fn refine<G: Function1D>(
        &self,
        g: &mut G,
        x_accuracy: Real,
        mut st: Solver1DState,
    ) -> QlResult<Real> {
        let mut froot = checked_function_value(g, st.root)?;
        let mut dfroot = checked_derivative(g, st.root)?;
        st.evaluation_number += 1;

        while st.evaluation_number <= self.config.max_evaluations {
            if dfroot == 0.0 || !dfroot.is_finite() {
                fail!(
                    "Newton solver hit an unusable derivative ({dfroot}) at x = {}",
                    st.root
                );
            }
            let dx = froot / dfroot;
            st.root -= dx;
            // Pure Newton: a step out of the bracket is an error (QuantLib would
            // hand off to NewtonSafe here).
            if (st.x_min - st.root) * (st.root - st.x_max) < 0.0 {
                fail!(
                    "Newton solver left the bracket: root ({}) not in [{}, {}]",
                    st.root,
                    st.x_min,
                    st.x_max
                );
            }
            if dx.abs() < x_accuracy {
                // Final call at the root so a stateful functor records it.
                let _ = checked_function_value(g, st.root)?;
                st.evaluation_number += 1;
                return Ok(st.root);
            }
            froot = checked_function_value(g, st.root)?;
            dfroot = checked_derivative(g, st.root)?;
            st.evaluation_number += 1;
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
    use crate::math::solver1d::func1d;
    use crate::math::solvers1d::testkit;

    #[test]
    fn finds_known_roots() {
        testkit::check_derivative_solver_finds_roots(Newton::new);
    }

    #[test]
    fn last_call_is_made_with_the_root() {
        testkit::check_derivative_last_call(Newton::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_derivative_rejects(Newton::new);
    }

    #[test]
    fn honours_configured_bounds() {
        // A bracket past the upper bound is rejected by the shared driver.
        let solver = Newton::new().with_upper_bound(2.0);
        assert!(
            solver
                .solve_bracketed(func1d(testkit::f1, testkit::d1), 1e-8, 1.5, 0.0, 3.0)
                .is_err()
        );
    }

    // Pure Newton: a step that leaves the bracket is an explicit error, not a
    // silent fallback. atan diverges under Newton from |x| > ~1.39, so guessing
    // 1.5 inside [-2, 2] forces a step outside.
    #[test]
    fn errors_explicitly_when_a_step_leaves_the_bracket() {
        let atan = func1d(|x: Real| x.atan(), |x: Real| 1.0 / (1.0 + x * x));
        let err = Newton::new()
            .solve_bracketed(atan, 1e-12, 1.5, -2.0, 2.0)
            .unwrap_err();
        assert!(
            err.message().contains("left the bracket"),
            "unexpected error: {}",
            err.message()
        );
    }

    // A genuinely FnMut functor (mutating captured state directly, not via a
    // Cell) must be accepted - the reason Function1D takes &mut self. This would
    // not compile if func1d required Fn.
    #[test]
    fn accepts_a_stateful_fnmut_functor() {
        let mut calls = 0_usize;
        let g = func1d(
            |x: Real| {
                calls += 1;
                x * x - 1.0
            },
            |x: Real| 2.0 * x,
        );
        let root = Newton::new()
            .solve_bracketed(g, 1e-10, 0.5, 0.0, 2.0)
            .unwrap();
        assert!((root - 1.0).abs() <= 1e-9, "root={root}");
        assert!(calls > 0, "the functor was never called");
    }

    // A zero (or non-finite) derivative makes the Newton step undefined; report it
    // explicitly rather than dividing by zero into an escape.
    #[test]
    fn errors_on_unusable_derivative() {
        let g = func1d(|x: Real| x * x - 1.0, |_: Real| 0.0);
        let err = Newton::new()
            .solve_bracketed(g, 1e-8, 0.5, 0.0, 2.0)
            .unwrap_err();
        assert!(
            err.message().contains("unusable derivative"),
            "unexpected error: {}",
            err.message()
        );
    }
}
