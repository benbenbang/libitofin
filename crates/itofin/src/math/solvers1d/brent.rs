//! Brent 1-D solver.
//!
//! Port of `ql/math/solvers1d/brent.hpp`: Brent's method (inverse quadratic
//! interpolation with a bisection fallback), following the QuantLib
//! implementation which itself follows Press et al., "Numerical Recipes in C",
//! 2nd ed.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::math::solver1d::{Solver1D, Solver1DState, SolverConfig};
use crate::types::Real;

/// Brent's method root finder.
#[derive(Clone, Copy, Debug)]
pub struct Brent {
    config: SolverConfig,
}

impl Brent {
    /// A Brent solver with the default configuration.
    pub fn new() -> Self {
        Brent {
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

impl Default for Brent {
    fn default() -> Self {
        Brent::new()
    }
}

/// `|a|` with the sign of `b` (NR's `SIGN`).
fn sign(a: Real, b: Real) -> Real {
    if b >= 0.0 { a.abs() } else { -a.abs() }
}

impl Solver1D for Brent {
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
        // Start with root_ (the guess) on one side of the bracket and both
        // x_min and x_max on the other.
        let mut froot = f(st.root);
        st.evaluation_number += 1;
        if froot * st.fx_min < 0.0 {
            st.x_max = st.x_min;
            st.fx_max = st.fx_min;
        } else {
            st.x_min = st.x_max;
            st.fx_min = st.fx_max;
        }
        let mut d = st.root - st.x_max;
        let mut e = d;

        while st.evaluation_number <= self.max_evaluations() {
            if (froot > 0.0 && st.fx_max > 0.0) || (froot < 0.0 && st.fx_max < 0.0) {
                // rename x_min, root, x_max and adjust bounds
                st.x_max = st.x_min;
                st.fx_max = st.fx_min;
                e = st.root - st.x_min;
                d = e;
            }
            if st.fx_max.abs() < froot.abs() {
                st.x_min = st.root;
                st.root = st.x_max;
                st.x_max = st.x_min;
                st.fx_min = froot;
                froot = st.fx_max;
                st.fx_max = st.fx_min;
            }
            // convergence check
            let x_acc1 = 2.0 * Real::EPSILON * st.root.abs() + 0.5 * x_accuracy;
            let x_mid = (st.x_max - st.root) / 2.0;
            if x_mid.abs() <= x_acc1 || close(froot, 0.0) {
                // QuantLib makes one last call with the root so a stateful
                // functor records it; preserve that side effect.
                let _ = f(st.root);
                st.evaluation_number += 1;
                return Ok(st.root);
            }
            if e.abs() >= x_acc1 && st.fx_min.abs() > froot.abs() {
                // attempt inverse quadratic interpolation
                let s = froot / st.fx_min;
                let mut p;
                let mut q;
                if close(st.x_min, st.x_max) {
                    p = 2.0 * x_mid * s;
                    q = 1.0 - s;
                } else {
                    let qq = st.fx_min / st.fx_max;
                    let r = froot / st.fx_max;
                    p = s * (2.0 * x_mid * qq * (qq - r) - (st.root - st.x_min) * (r - 1.0));
                    q = (qq - 1.0) * (r - 1.0) * (s - 1.0);
                }
                if p > 0.0 {
                    q = -q; // check whether in bounds
                }
                p = p.abs();
                let min1 = 3.0 * x_mid * q - (x_acc1 * q).abs();
                let min2 = (e * q).abs();
                if 2.0 * p < min1.min(min2) {
                    e = d; // accept interpolation
                    d = p / q;
                } else {
                    d = x_mid; // interpolation failed, use bisection
                    e = d;
                }
            } else {
                // bounds decreasing too slowly, use bisection
                d = x_mid;
                e = d;
            }
            st.x_min = st.root;
            st.fx_min = froot;
            if d.abs() > x_acc1 {
                st.root += d;
            } else {
                st.root += sign(x_acc1, x_mid);
            }
            froot = f(st.root);
            st.evaluation_number += 1;
        }
        fail!(
            "maximum number of function evaluations ({}) exceeded",
            self.max_evaluations()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    // F1, F2, F3 from solvers.cpp: each has its single root at x = 1.
    fn f1(x: Real) -> Real {
        x * x - 1.0
    }
    fn f2(x: Real) -> Real {
        1.0 - x * x
    }
    fn f3(x: Real) -> Real {
        (x - 1.0).atan()
    }

    const ACCURACIES: [Real; 3] = [1.0e-4, 1.0e-6, 1.0e-8];

    fn test_not_bracketed(f: impl Fn(Real) -> Real + Copy, guess: Real) {
        for acc in ACCURACIES {
            let root = Brent::new().solve(f, acc, guess, 0.1).unwrap();
            assert!(
                (root - 1.0).abs() <= acc,
                "not bracketed: guess={guess} acc={acc} root={root}"
            );
        }
    }

    fn test_bracketed(f: impl Fn(Real) -> Real + Copy, guess: Real) {
        for acc in ACCURACIES {
            let root = Brent::new()
                .solve_bracketed(f, acc, guess, 0.0, 2.0)
                .unwrap();
            assert!(
                (root - 1.0).abs() <= acc,
                "bracketed: guess={guess} acc={acc} root={root}"
            );
        }
    }

    // Port of test_solver(Brent, ...): roots of x^2-1, 1-x^2 and atan(x-1),
    // guessing from either side, both auto-bracketing and pre-bracketed.
    #[test]
    fn finds_known_roots() {
        for guess in [0.5, 1.5] {
            test_not_bracketed(f1, guess);
            test_bracketed(f1, guess);
            test_not_bracketed(f2, guess);
            test_bracketed(f2, guess);
        }
        test_not_bracketed(f3, 1.00001);
    }

    // Port of test_last_call_with_root: the solver's final function call must be
    // made at the value it returns. Probe records the last argument it saw; that
    // must equal the returned root. Probe is stateful, exercising the FnMut path.
    #[test]
    fn last_call_is_made_with_the_root() {
        let mins = [3.0, 2.25, 1.5, 1.0];
        let maxs = [7.0, 5.75, 4.5, 3.0];
        let steps = [0.2, 0.2, 0.1, 0.1];
        let offsets = [25.0, 11.0, 5.0, 1.0];
        let guesses = [4.5, 4.5, 2.5, 2.5];
        let accuracy = 1.0e-6;

        for bracketed in [false, true] {
            // `argument` persists across the four probes; `previous` is captured
            // at each probe's construction from the leftover argument, so the
            // roots chain as sqrt(previous + offset) = 5, 4, 3, 2.
            let argument = Cell::new(0.0);
            for i in 0..4 {
                let previous = argument.get();
                // Probe(x) = previous + offset - x^2, recording x into `argument`.
                let probe = |x: Real| {
                    argument.set(x);
                    previous + offsets[i] - x * x
                };
                let result = if bracketed {
                    Brent::new()
                        .solve_bracketed(probe, accuracy, guesses[i], mins[i], maxs[i])
                        .unwrap()
                } else {
                    Brent::new()
                        .solve(probe, accuracy, guesses[i], steps[i])
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

    #[test]
    fn rejects_invalid_inputs() {
        // non-positive accuracy
        assert!(Brent::new().solve(f1, 0.0, 0.5, 0.1).is_err());
        // unbracketed range (f1 > 0 on [2, 3])
        assert!(
            Brent::new()
                .solve_bracketed(f1, 1e-8, 2.5, 2.0, 3.0)
                .is_err()
        );
        // guess outside the range
        assert!(
            Brent::new()
                .solve_bracketed(f1, 1e-8, 5.0, 0.0, 2.0)
                .is_err()
        );
    }

    #[test]
    fn honours_configured_bounds() {
        // The builders feed the shared driver: a bracket past the upper bound is
        // rejected, while a bounded auto-bracketing search still finds the root.
        let mut solver = Brent::new().with_upper_bound(2.0);
        assert!(solver.solve_bracketed(f1, 1e-8, 1.5, 0.0, 3.0).is_err());

        let mut solver = Brent::new().with_lower_bound(0.0).with_upper_bound(5.0);
        let root = solver.solve(f1, 1e-10, 0.5, 0.1).unwrap();
        assert!((root - 1.0).abs() <= 1e-9, "root={root}");
    }
}
