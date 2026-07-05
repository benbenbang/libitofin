//! Steepest descent optimization method.
//!
//! Port of `ql/math/optimization/steepestdescent.{hpp,cpp}`. The search
//! direction is `-f'(x)`; it requires the gradient of the cost function.

use crate::errors::QlResult;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::linesearch::{ArmijoLineSearch, LineSearch};
use crate::math::optimization::linesearchbasedmethod::line_search_based_minimize;
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;

/// Multi-dimensional steepest descent method.
pub struct SteepestDescent {
    line_search: Box<dyn LineSearch>,
}

impl SteepestDescent {
    /// A steepest descent method with the default Armijo line search.
    pub fn new() -> Self {
        SteepestDescent::with_line_search(Box::new(ArmijoLineSearch::default()))
    }

    /// A steepest descent method with the given line search.
    pub fn with_line_search(line_search: Box<dyn LineSearch>) -> Self {
        SteepestDescent { line_search }
    }
}

impl Default for SteepestDescent {
    fn default() -> Self {
        SteepestDescent::new()
    }
}

impl OptimizationMethod for SteepestDescent {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        line_search_based_minimize(
            problem,
            end_criteria,
            &mut *self.line_search,
            |_problem, _gold2, _old_gradient, line_search| -line_search.last_gradient(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::optimization::testsupport::check_parabola_minimization;

    #[test]
    fn minimizes_one_dimensional_parabola() {
        check_parabola_minimization(&mut SteepestDescent::new(), "Steepest Descent");
    }
}
