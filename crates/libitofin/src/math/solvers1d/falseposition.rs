//! False-position 1-D solver.
//!
//! Port of `ql/math/solvers1d/falseposition.hpp`: regula falsi, stepping to the
//! x-intercept of the secant through the current bracket and replacing whichever
//! end shares the new point's sign. Following Press et al., "Numerical Recipes in
//! C", 2nd ed.

use crate::errors::QlResult;
use crate::fail;
use crate::math::comparison::close;
use crate::math::solver1d::{Solver1D, Solver1DState, SolverConfig, checked_value};
use crate::types::Real;

/// False-position root finder.
#[derive(Clone, Copy, Debug)]
pub struct FalsePosition {
    config: SolverConfig,
}

impl FalsePosition {
    /// A false-position solver with the default configuration.
    pub fn new() -> Self {
        FalsePosition {
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

impl Default for FalsePosition {
    fn default() -> Self {
        FalsePosition::new()
    }
}

impl Solver1D for FalsePosition {
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
        // Orient so (xl, fl) is the low side, where f < 0.
        let (mut xl, mut fl, mut xh, mut fh) = if st.fx_min < 0.0 {
            (st.x_min, st.fx_min, st.x_max, st.fx_max)
        } else {
            (st.x_max, st.fx_max, st.x_min, st.fx_min)
        };

        while st.evaluation_number <= self.max_evaluations() {
            // Step to the secant's x-intercept.
            st.root = xl + (xh - xl) * fl / (fl - fh);
            let froot = checked_value(f, st.root)?;
            st.evaluation_number += 1;
            // Replace the end whose sign matches the new point.
            let del = if froot < 0.0 {
                let del = xl - st.root;
                xl = st.root;
                fl = froot;
                del
            } else {
                let del = xh - st.root;
                xh = st.root;
                fh = froot;
                del
            };
            if del.abs() < x_accuracy || close(froot, 0.0) {
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
        testkit::check_finds_known_roots(FalsePosition::new);
    }

    #[test]
    fn last_call_is_made_with_the_root() {
        testkit::check_last_call_with_root(FalsePosition::new);
    }

    #[test]
    fn rejects_invalid_inputs() {
        testkit::check_rejects_invalid_inputs(FalsePosition::new);
    }

    #[test]
    fn honours_configured_bounds() {
        testkit::check_honours_bounds(FalsePosition::new);
    }
}
