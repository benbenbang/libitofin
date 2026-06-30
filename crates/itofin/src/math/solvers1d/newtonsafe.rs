//! Newton-safe 1-D solver.
//!
//! Port of `ql/math/solvers1d/newtonsafe.hpp`: Newton-Raphson that takes a
//! bisection step whenever a Newton step would leave the bracket or fail to
//! shrink it fast enough (Numerical Recipes' `rtsafe`). Unlike [`Newton`] it
//! never escapes the bracket and tolerates a zero derivative (it just bisects),
//! so it handles the cases pure Newton rejects. Following Press et al.,
//! "Numerical Recipes in C", 2nd ed.
//!
//! [`Newton`]: super::newton::Newton

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::math::solver1d::{
    Bracketed, DerivativeSolver, Function1D, Solver1DState, SolverConfig, bracket_by_stepping,
    bracket_given,
};
use crate::types::Real;

/// Newton-safe root finder (Newton with a bisection fallback).
#[derive(Clone, Copy, Debug)]
pub struct NewtonSafe {
    config: SolverConfig,
}

impl NewtonSafe {
    /// A Newton-safe solver with the default configuration.
    pub fn new() -> Self {
        NewtonSafe {
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

    /// Safe Newton iteration on a prepared bracket.
    fn refine<G: Function1D>(
        &self,
        g: &mut G,
        x_accuracy: Real,
        mut st: Solver1DState,
    ) -> QlResult<Real> {
        // Orient so f(xl) < 0.
        let (mut xl, mut xh) = if st.fx_min < 0.0 {
            (st.x_min, st.x_max)
        } else {
            (st.x_max, st.x_min)
        };
        // The bracket width is guaranteed positive by the bracketing phase.
        let mut dxold = st.x_max - st.x_min;
        let mut dx = dxold;

        let mut froot = g.value(st.root);
        // A root with a zero (or near-zero) derivative - e.g. the flat inflection
        // of (x-1)^3 - is detected here so the step below never divides 0 by 0.
        if close(froot, 0.0) {
            return Ok(st.root);
        }
        let mut dfroot = g.derivative(st.root);
        st.evaluation_number += 1;

        while st.evaluation_number <= self.config.max_evaluations {
            // Bisect if the Newton step would leave [xl, xh] or is not shrinking
            // the bracket fast enough; otherwise take the Newton step.
            if ((st.root - xh) * dfroot - froot) * ((st.root - xl) * dfroot - froot) > 0.0
                || (2.0 * froot).abs() > (dxold * dfroot).abs()
            {
                dxold = dx;
                dx = 0.5 * (xh - xl);
                st.root = xl + dx;
            } else {
                dxold = dx;
                dx = froot / dfroot;
                st.root -= dx;
            }
            if dx.abs() < x_accuracy {
                // Final call at the root so a stateful functor records it.
                let _ = g.value(st.root);
                st.evaluation_number += 1;
                return Ok(st.root);
            }
            froot = g.value(st.root);
            if close(froot, 0.0) {
                return Ok(st.root);
            }
            dfroot = g.derivative(st.root);
            st.evaluation_number += 1;
            // Keep the new point as the matching bracket end.
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

impl Default for NewtonSafe {
    fn default() -> Self {
        NewtonSafe::new()
    }
}

impl DerivativeSolver for NewtonSafe {
    fn solve<G: Function1D>(
        &self,
        mut g: G,
        accuracy: Real,
        guess: Real,
        step: Real,
    ) -> QlResult<Real> {
        if accuracy <= 0.0 {
            fail!("accuracy ({accuracy}) must be positive");
        }
        let accuracy = accuracy.max(Real::EPSILON);
        // Bind before matching so the value-closure's borrow of `g` is released
        // before `refine` takes `&mut g`.
        let bracketed = bracket_by_stepping(&self.config, &mut |x| g.value(x), guess, step)?;
        match bracketed {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(st) => self.refine(&mut g, accuracy, st),
        }
    }

    fn solve_bracketed<G: Function1D>(
        &self,
        mut g: G,
        accuracy: Real,
        guess: Real,
        x_min: Real,
        x_max: Real,
    ) -> QlResult<Real> {
        if accuracy <= 0.0 {
            fail!("accuracy ({accuracy}) must be positive");
        }
        let accuracy = accuracy.max(Real::EPSILON);
        let bracketed = bracket_given(&self.config, &mut |x| g.value(x), guess, x_min, x_max)?;
        match bracketed {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(st) => self.refine(&mut g, accuracy, st),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::solver1d::func1d;
    use crate::math::solvers1d::testkit;

    #[test]
    fn finds_known_roots() {
        testkit::check_derivative_solver_finds_roots(NewtonSafe::new);
    }

    // The safe variant owns the f3 = atan(x-1), guess=1.00001 stress case that
    // forces a Newton step out of the bracket - it bisects and still converges.
    #[test]
    fn handles_the_bracket_escape_stress_case() {
        testkit::check_safe_derivative_solver(NewtonSafe::new);
    }

    #[test]
    fn last_call_is_made_with_the_root() {
        testkit::check_derivative_last_call(NewtonSafe::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_derivative_rejects(NewtonSafe::new);
    }

    #[test]
    fn honours_configured_bounds() {
        let solver = NewtonSafe::new().with_upper_bound(2.0);
        assert!(
            solver
                .solve_bracketed(func1d(testkit::f1, testkit::d1), 1e-8, 1.5, 0.0, 3.0)
                .is_err()
        );
    }

    // Regression: a flat root where f and f' both vanish - (x-1)^3 has
    // f(1) = f'(1) = 0 - must return the root, not 0/0 = NaN. The guess sits
    // exactly on it.
    #[test]
    fn flat_root_returns_the_root_not_nan() {
        let g = func1d(
            |x: Real| (x - 1.0).powi(3),
            |x: Real| 3.0 * (x - 1.0).powi(2),
        );
        let root = NewtonSafe::new()
            .solve_bracketed(g, 1e-10, 1.0, 0.0, 2.0)
            .unwrap();
        assert_eq!(root, 1.0);
    }

    // With a useless (always-zero) derivative NewtonSafe degrades to pure
    // bisection instead of erroring like Newton. Root of x^2 - 2 is sqrt(2),
    // which bisection approaches but never lands on exactly.
    #[test]
    fn degrades_to_bisection_with_a_useless_derivative() {
        let g = func1d(|x: Real| x * x - 2.0, |_: Real| 0.0);
        let root = NewtonSafe::new()
            .solve_bracketed(g, 1e-10, 1.0, 0.0, 2.0)
            .unwrap();
        assert!((root - 2.0_f64.sqrt()).abs() <= 1e-9, "root={root}");
    }
}
