//! Halley 1-D solver.
//!
//! Port of `ql/math/solvers1d/halley.hpp`: Halley's method, a cubically-convergent
//! step using the value, first and second derivatives. Like QuantLib's Newton it
//! hands the rest of the search to [`NewtonSafe`] if a step leaves the bracket
//! (and `NewtonSafe` needs only the value and first derivative, which a
//! [`Function2D`] also provides).
//!
//! [`NewtonSafe`]: super::newtonsafe::NewtonSafe

use crate::errors::QlResult;
use crate::fail;
use crate::math::solver1d::{
    Bracketed, DerivativeSolver, Function2D, Solver1DState, SolverConfig, bracket_by_stepping,
    bracket_given,
};
use crate::math::solvers1d::newtonsafe::NewtonSafe;
use crate::types::Real;

/// Halley's method root finder.
#[derive(Clone, Copy, Debug)]
pub struct Halley {
    config: SolverConfig,
}

impl Halley {
    /// A Halley solver with the default configuration.
    pub fn new() -> Self {
        Halley {
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

    /// Find a zero of `g` near `guess`, auto-bracketing in steps of `step`.
    ///
    /// # Errors
    ///
    /// Returns an error if `accuracy <= 0`, no bracket is found, or the refinement
    /// (including any NewtonSafe handoff) exhausts the evaluation budget.
    pub fn solve<G: Function2D>(
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
        // before `refine` takes ownership.
        let bracketed = bracket_by_stepping(&self.config, &mut |x| g.value(x), guess, step)?;
        match bracketed {
            Bracketed::Root(x) => Ok(x),
            Bracketed::Ready(st) => self.refine(g, accuracy, st),
        }
    }

    /// Find a zero of `g` in the caller-supplied bracket `[x_min, x_max]`.
    ///
    /// # Errors
    ///
    /// As for [`solve`](Self::solve), plus the bracket-validation errors of the
    /// shared driver.
    pub fn solve_bracketed<G: Function2D>(
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
            Bracketed::Ready(st) => self.refine(g, accuracy, st),
        }
    }

    /// Halley iteration on a prepared bracket. Takes `g` by value so it can be
    /// handed to [`NewtonSafe`] on a bracket escape.
    fn refine<G: Function2D>(
        &self,
        mut g: G,
        x_accuracy: Real,
        mut st: Solver1DState,
    ) -> QlResult<Real> {
        loop {
            st.evaluation_number += 1;
            if st.evaluation_number > self.config.max_evaluations {
                break;
            }
            let fx = g.value(st.root);
            let f_prime = g.derivative(st.root);
            let lf = fx * g.second_derivative(st.root) / (f_prime * f_prime);
            let step = 1.0 / (1.0 - 0.5 * lf) * fx / f_prime;

            // A zero or non-finite first derivative (or the degenerate
            // 1 - lf/2 = 0) makes the step non-finite; stepping to NaN would then
            // silently defeat the bracket-escape check below and burn the whole
            // budget. Newton/NewtonSafe guard the same divisions - here we hand
            // the rest of the search to NewtonSafe, which bisects safely from the
            // current, still-in-bracket root.
            if !step.is_finite() {
                let remaining = self.config.max_evaluations - st.evaluation_number;
                return NewtonSafe::new()
                    .with_max_evaluations(remaining)
                    .solve_bracketed(g, x_accuracy, st.root, st.x_min, st.x_max);
            }
            st.root -= step;

            // Jumped out of the bracket: hand the rest to NewtonSafe (which needs
            // only value + first derivative, both carried by a Function2D).
            if (st.x_min - st.root) * (st.root - st.x_max) < 0.0 {
                let remaining = self.config.max_evaluations - st.evaluation_number;
                return NewtonSafe::new()
                    .with_max_evaluations(remaining)
                    .solve_bracketed(g, x_accuracy, st.root + step, st.x_min, st.x_max);
            }
            if step.abs() < x_accuracy {
                // Final call at the root so a stateful functor records it.
                let _ = g.value(st.root);
                st.evaluation_number += 1;
                return Ok(st.root);
            }
        }
        fail!(
            "maximum number of function evaluations ({}) exceeded",
            self.config.max_evaluations
        )
    }
}

impl Default for Halley {
    fn default() -> Self {
        Halley::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::solver1d::func2d;
    use crate::math::solvers1d::testkit::{d1, d2, d3, dd1, dd2, dd3, f1, f2, f3};
    use std::cell::Cell;

    const ACCURACIES: [Real; 3] = [1.0e-4, 1.0e-6, 1.0e-8];

    // Port of test_solver for Halley: roots of f1, f2 (both sides, auto + pre-
    // bracketed), and the f3 = atan(x-1), guess=1.00001 stress case where a Halley
    // step leaves the bracket and the search is handed to NewtonSafe.
    #[test]
    fn finds_known_roots() {
        let cases = [
            (
                f1 as fn(Real) -> Real,
                d1 as fn(Real) -> Real,
                dd1 as fn(Real) -> Real,
            ),
            (f2, d2, dd2),
        ];
        for (f, d, dd) in cases {
            for guess in [0.5, 1.5] {
                for acc in ACCURACIES {
                    let root = Halley::new()
                        .solve(func2d(f, d, dd), acc, guess, 0.1)
                        .unwrap();
                    assert!(
                        (root - 1.0).abs() <= acc,
                        "auto: guess={guess} acc={acc} root={root}"
                    );
                    let root = Halley::new()
                        .solve_bracketed(func2d(f, d, dd), acc, guess, 0.0, 2.0)
                        .unwrap();
                    assert!(
                        (root - 1.0).abs() <= acc,
                        "bracketed: guess={guess} acc={acc} root={root}"
                    );
                }
            }
        }
        for acc in ACCURACIES {
            let root = Halley::new()
                .solve(func2d(f3, d3, dd3), acc, 1.00001, 0.1)
                .unwrap();
            assert!((root - 1.0).abs() <= acc, "f3: acc={acc} root={root}");
        }
    }

    // Port of test_last_call_with_root: the final function call is made at the
    // returned root (Probe with its true derivatives, f' = -2x, f'' = -2).
    #[test]
    fn last_call_is_made_with_the_root() {
        let mins = [3.0, 2.25, 1.5, 1.0];
        let maxs = [7.0, 5.75, 4.5, 3.0];
        let steps = [0.2, 0.2, 0.1, 0.1];
        let offsets = [25.0, 11.0, 5.0, 1.0];
        let guesses = [4.5, 4.5, 2.5, 2.5];
        let accuracy = 1.0e-6;

        for bracketed in [false, true] {
            let argument = Cell::new(0.0);
            for i in 0..4 {
                let previous = argument.get();
                let g = func2d(
                    |x: Real| {
                        argument.set(x);
                        previous + offsets[i] - x * x
                    },
                    |x: Real| -2.0 * x,
                    |_: Real| -2.0,
                );
                let result = if bracketed {
                    Halley::new()
                        .solve_bracketed(g, accuracy, guesses[i], mins[i], maxs[i])
                        .unwrap()
                } else {
                    Halley::new()
                        .solve(g, accuracy, guesses[i], steps[i])
                        .unwrap()
                };
                assert!(
                    (result - argument.get()).abs() <= 2.0 * Real::EPSILON,
                    "bracketed={bracketed} i={i}: result={result} last_arg={}",
                    argument.get()
                );
            }
        }
    }

    // Regression: a zero (or non-finite) first derivative makes the Halley step
    // NaN. The old code did `root -= NaN`, which slipped past the bracket-escape
    // check (NaN comparisons are false) and exhausted the whole budget with an
    // unhelpful error. The unusable step is now detected and the search handed to
    // NewtonSafe, which bisects and still finds the root.
    #[test]
    fn unusable_derivative_hands_off_to_newton_safe() {
        // f(x) = x^2 - 2, root sqrt(2) in [1, 2]. The derivative reports 0 on its
        // first call (at the guess 1.5), as a finite-difference derivative might
        // when it underflows, then behaves normally.
        let first = Cell::new(true);
        let g = func2d(
            |x: Real| x * x - 2.0,
            |x: Real| if first.replace(false) { 0.0 } else { 2.0 * x },
            |_: Real| 2.0,
        );
        let root = Halley::new()
            .solve_bracketed(g, 1e-10, 1.5, 1.0, 2.0)
            .unwrap();
        assert!((root - 2.0_f64.sqrt()).abs() <= 1e-9, "root={root}");
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(
            Halley::new()
                .solve(func2d(f1, d1, dd1), 0.0, 0.5, 0.1)
                .is_err()
        );
        assert!(
            Halley::new()
                .solve_bracketed(func2d(f1, d1, dd1), 1e-8, 2.5, 2.0, 3.0)
                .is_err()
        );
        assert!(
            Halley::new()
                .solve_bracketed(func2d(f1, d1, dd1), 1e-8, 5.0, 0.0, 2.0)
                .is_err()
        );
    }

    #[test]
    fn honours_configured_bounds() {
        let solver = Halley::new().with_upper_bound(2.0);
        assert!(
            solver
                .solve_bracketed(func2d(f1, d1, dd1), 1e-8, 1.5, 0.0, 3.0)
                .is_err()
        );
    }
}
