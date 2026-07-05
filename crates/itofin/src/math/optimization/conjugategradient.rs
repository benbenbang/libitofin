//! Conjugate gradient optimization method.
//!
//! Port of `ql/math/optimization/conjugategradient.{hpp,cpp}`:
//! Fletcher-Reeves-Polak-Ribiere as adapted from Numerical Recipes in C,
//! 2nd edition. The search direction is `d_i = -f'(x_i) + c_i * d_(i-1)`
//! with `c_i = ||f'(x_i)||^2 / ||f'(x_(i-1))||^2` and `d_1 = -f'(x_1)`;
//! it requires the gradient of the cost function.

use crate::errors::QlResult;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::linesearch::{ArmijoLineSearch, LineSearch};
use crate::math::optimization::linesearchbasedmethod::line_search_based_minimize;
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;

/// Multi-dimensional conjugate gradient method.
pub struct ConjugateGradient {
    line_search: Box<dyn LineSearch>,
}

impl ConjugateGradient {
    /// A conjugate gradient method with the default Armijo line search.
    pub fn new() -> Self {
        ConjugateGradient::with_line_search(Box::new(ArmijoLineSearch::default()))
    }

    /// A conjugate gradient method with the given line search.
    pub fn with_line_search(line_search: Box<dyn LineSearch>) -> Self {
        ConjugateGradient { line_search }
    }
}

impl Default for ConjugateGradient {
    fn default() -> Self {
        ConjugateGradient::new()
    }
}

impl OptimizationMethod for ConjugateGradient {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        line_search_based_minimize(
            problem,
            end_criteria,
            &mut *self.line_search,
            |problem, gold2, _old_gradient, line_search| {
                &(-line_search.last_gradient())
                    + &(line_search.search_direction() * (problem.gradient_norm_value() / gold2))
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::array::Array;
    use crate::math::optimization::constraint::NoConstraint;
    use crate::math::optimization::costfunction::CostFunction;
    use crate::math::optimization::testsupport::{
        OneDimensionalPolynomialDegreeN, check_parabola_minimization,
    };

    #[test]
    fn minimizes_one_dimensional_parabola() {
        check_parabola_minimization(&mut ConjugateGradient::new(), "Conjugate Gradient");
    }

    #[test]
    fn can_be_reused_on_a_different_problem() {
        // 2-D extension of the parabola oracle: minimum 0.75 at (-0.5, 0)
        struct TwoDimQuadratic;
        impl CostFunction for TwoDimQuadratic {
            fn values(&self, x: &Array) -> Array {
                Array::from([x[0] * x[0] + x[0] + 1.0 + x[1] * x[1]])
            }
        }
        let mut method = ConjugateGradient::new();
        check_parabola_minimization(&mut method, "Conjugate Gradient (first run)");

        let cost = TwoDimQuadratic;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([-100.0, 50.0]));
        let end_criteria = EndCriteria::new(10000, Some(100), 1e-8, 1e-8, Some(1e-8)).unwrap();
        method.minimize(&mut problem, &end_criteria).unwrap();
        let x = problem.current_value();
        assert!((x[0] + 0.5).abs() < 1e-3, "x[0] = {}", x[0]);
        assert!(x[1].abs() < 1e-3, "x[1] = {}", x[1]);
        assert!(
            (problem.function_value() - 0.75).abs() <= 1e-8,
            "f = {}",
            problem.function_value()
        );
    }

    #[test]
    fn stores_the_accepted_point_with_its_function_value() {
        let cost = OneDimensionalPolynomialDegreeN::new(Array::from([1.0, 1.0, 1.0]));
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([-100.0]));
        let end_criteria = EndCriteria::new(10000, Some(100), 1e-8, 1e-8, Some(1e-8)).unwrap();
        ConjugateGradient::new()
            .minimize(&mut problem, &end_criteria)
            .unwrap();
        // the reported minimum and its function value must belong to the
        // same point
        assert_eq!(
            problem.function_value(),
            cost.value(problem.current_value())
        );
    }
}
