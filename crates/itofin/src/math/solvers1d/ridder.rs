//! Ridder 1-D solver.
//!
//! Port of `ql/math/solvers1d/ridder.hpp`: Ridder's method, which fits an
//! exponential to the bracket midpoint and ends and steps to its root, keeping
//! the bracket. Two function evaluations per iteration. Following Press et al.,
//! "Numerical Recipes in C", 2nd ed.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::math::solver1d::{Solver1D, Solver1DState, SolverConfig};
use crate::types::Real;

/// Ridder's method root finder.
#[derive(Clone, Copy, Debug)]
pub struct Ridder {
    config: SolverConfig,
}

impl Ridder {
    /// A Ridder solver with the default configuration.
    pub fn new() -> Self {
        Ridder {
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

impl Default for Ridder {
    fn default() -> Self {
        Ridder::new()
    }
}

/// `true` iff `a` and `b` have strictly opposite signs, i.e. QuantLib's
/// `sign(a, b) != a`. Uses ordered comparisons, so it is robust where `a * b`
/// would underflow to a signed zero.
fn sign_differs(a: Real, b: Real) -> bool {
    (b >= 0.0 && a < 0.0) || (b < 0.0 && a > 0.0)
}

impl Solver1D for Ridder {
    fn config(&self) -> &SolverConfig {
        &self.config
    }

    fn config_mut(&mut self) -> &mut SolverConfig {
        &mut self.config
    }

    fn solve_impl<F>(&mut self, f: &mut F, x_acc: Real, st: &mut Solver1DState) -> QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        // Black-Scholes implied-vol tests show Ridder delivers ~100x the promised
        // accuracy, so QuantLib tightens the target by that factor.
        let x_accuracy = x_acc / 100.0;
        // Any highly unlikely value, so the first `next_root` comparison is false.
        st.root = Real::MIN;

        while st.evaluation_number <= self.max_evaluations() {
            let x_mid = 0.5 * (st.x_min + st.x_max);
            // First of two evaluations per iteration.
            let fx_mid = f(x_mid);
            st.evaluation_number += 1;
            let s = (fx_mid * fx_mid - st.fx_min * st.fx_max).sqrt();
            if close(s, 0.0) {
                let _ = f(st.root);
                st.evaluation_number += 1;
                return Ok(st.root);
            }
            // Updating formula.
            let direction = if st.fx_min >= st.fx_max { 1.0 } else { -1.0 };
            let next_root = x_mid + (x_mid - st.x_min) * (direction * fx_mid / s);
            if (next_root - st.root).abs() <= x_accuracy {
                let _ = f(st.root);
                st.evaluation_number += 1;
                return Ok(st.root);
            }

            st.root = next_root;
            // Second of two evaluations per iteration.
            let froot = f(st.root);
            st.evaluation_number += 1;
            if close(froot, 0.0) {
                return Ok(st.root);
            }

            // Keep the root bracketed for the next iteration.
            if sign_differs(fx_mid, froot) {
                st.x_min = x_mid;
                st.fx_min = fx_mid;
                st.x_max = st.root;
                st.fx_max = froot;
            } else if sign_differs(st.fx_min, froot) {
                st.x_max = st.root;
                st.fx_max = froot;
            } else if sign_differs(st.fx_max, froot) {
                st.x_min = st.root;
                st.fx_min = froot;
            } else {
                fail!("Ridder solver reached an unreachable bracketing state");
            }

            if (st.x_max - st.x_min).abs() <= x_accuracy {
                let _ = f(st.root);
                st.evaluation_number += 1;
                return Ok(st.root);
            }
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
    use crate::math::solvers1d::testkit;

    #[test]
    fn finds_known_roots() {
        testkit::check_finds_known_roots(Ridder::new);
    }

    #[test]
    fn last_call_is_made_with_the_root() {
        testkit::check_last_call_with_root(Ridder::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_rejects_invalid_inputs(Ridder::new);
    }

    #[test]
    fn honours_configured_bounds() {
        testkit::check_honours_bounds(Ridder::new);
    }
}
