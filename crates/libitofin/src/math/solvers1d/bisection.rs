//! Bisection 1-D solver.
//!
//! Port of `ql/math/solvers1d/bisection.hpp`: repeated interval halving, keeping
//! the half that brackets the sign change. Robust and unconditionally convergent
//! for a valid bracket, following Press et al., "Numerical Recipes in C", 2nd ed.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::math::solver1d::{Solver1D, Solver1DState, SolverConfig, checked_value};
use crate::types::Real;

/// Bisection root finder.
#[derive(Clone, Copy, Debug)]
pub struct Bisection {
    config: SolverConfig,
}

impl Bisection {
    /// A bisection solver with the default configuration.
    pub fn new() -> Self {
        Bisection {
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

impl Default for Bisection {
    fn default() -> Self {
        Bisection::new()
    }
}

impl Solver1D for Bisection {
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
        // Orient the search so that f > 0 lies at root + dx.
        let mut dx = if st.fx_min < 0.0 {
            st.root = st.x_min;
            st.x_max - st.x_min
        } else {
            st.root = st.x_max;
            st.x_min - st.x_max
        };

        while st.evaluation_number <= self.max_evaluations() {
            dx /= 2.0;
            let x_mid = st.root + dx;
            let f_mid = checked_value(f, x_mid)?;
            st.evaluation_number += 1;
            if f_mid <= 0.0 {
                st.root = x_mid;
            }
            if dx.abs() < x_accuracy || close(f_mid, 0.0) {
                // Final call at the root so a stateful functor records it.
                let _ = checked_value(f, st.root)?;
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
        testkit::check_finds_known_roots(Bisection::new);
    }

    #[test]
    fn last_call_is_made_with_the_root() {
        testkit::check_last_call_with_root(Bisection::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_rejects_invalid_inputs(Bisection::new);
    }

    #[test]
    fn honours_configured_bounds() {
        testkit::check_honours_bounds(Bisection::new);
    }
}
