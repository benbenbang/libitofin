//! Secant 1-D solver.
//!
//! Port of `ql/math/solvers1d/secant.hpp`: the secant method, stepping along the
//! line through the two most recent points. Derivative-free but not bracket-
//! preserving. Following Press et al., "Numerical Recipes in C", 2nd ed.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::math::solver1d::{Solver1D, Solver1DState, SolverConfig, checked_value};
use crate::types::Real;

/// Secant root finder.
#[derive(Clone, Copy, Debug)]
pub struct Secant {
    config: SolverConfig,
}

impl Secant {
    /// A secant solver with the default configuration.
    pub fn new() -> Self {
        Secant {
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

impl Default for Secant {
    fn default() -> Self {
        Secant::new()
    }
}

impl Solver1D for Secant {
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
        // Start from the bound with the smaller |f| as the most recent guess;
        // (xl, fl) holds the previous point.
        let (mut froot, mut xl, mut fl) = if st.fx_min.abs() < st.fx_max.abs() {
            st.root = st.x_min;
            (st.fx_min, st.x_max, st.fx_max)
        } else {
            st.root = st.x_max;
            (st.fx_max, st.x_min, st.fx_min)
        };

        while st.evaluation_number <= self.max_evaluations() {
            let dx = (xl - st.root) * froot / (froot - fl);
            xl = st.root;
            fl = froot;
            st.root += dx;
            froot = checked_value(f, st.root)?;
            st.evaluation_number += 1;
            if dx.abs() < x_accuracy || close(froot, 0.0) {
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
        testkit::check_finds_known_roots(Secant::new);
    }

    #[test]
    fn last_call_is_made_with_the_root() {
        testkit::check_last_call_with_root(Secant::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_rejects_invalid_inputs(Secant::new);
    }

    #[test]
    fn honours_configured_bounds() {
        testkit::check_honours_bounds(Secant::new);
    }
}
