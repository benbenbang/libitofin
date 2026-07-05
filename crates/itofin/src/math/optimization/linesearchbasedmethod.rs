//! Shared driver for line-search based optimization methods.
//!
//! Port of `ql/math/optimization/linesearchbasedmethod.{hpp,cpp}`. The C++
//! abstract base class' `minimize` becomes a free function taking the
//! `getUpdatedDirection` override as a closure; concrete methods
//! (conjugate gradient, steepest descent) wrap it. One deviation: on the
//! convergence exit the accepted point is stored in the problem, so the
//! reported minimum and its function value belong to the same point;
//! QuantLib returns there without `setCurrentValue`, leaving the problem's
//! current value one iterate behind.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::linesearch::LineSearch;
use crate::math::optimization::problem::Problem;
use crate::types::{Real, Size};

/// Minimizes `problem` by repeated line searches, with `updated_direction`
/// computing each new search direction from `(problem, gold2, old_gradient,
/// line_search)`, where `gold2` is the previous squared gradient norm.
pub(crate) fn line_search_based_minimize(
    problem: &mut Problem<'_>,
    end_criteria: &EndCriteria,
    line_search: &mut dyn LineSearch,
    mut updated_direction: impl FnMut(&Problem<'_>, Real, &Array, &dyn LineSearch) -> Array,
) -> QlResult<EndCriteriaType> {
    let ftol = end_criteria.function_epsilon();
    let mut max_stationary_state_iterations = end_criteria.max_stationary_state_iterations();
    let mut ec_type = EndCriteriaType::None;
    problem.reset();
    line_search.reset();
    let mut x = problem.current_value().clone();
    let mut iteration_number: Size = 0;
    // classical initial value for the line-search step
    let mut t = 1.0;

    let mut prev_gradient = Array::with_size(x.size());
    let function_value = problem.value_and_gradient(&mut prev_gradient, &x);
    problem.set_function_value(function_value);
    problem.set_gradient_norm_value(prev_gradient.dot(&prev_gradient));
    line_search.set_search_direction(-&prev_gradient);

    let mut first_time = true;
    loop {
        if !first_time {
            prev_gradient = line_search.last_gradient().clone();
        }
        t = line_search.search(problem, &mut ec_type, end_criteria, t)?;
        // don't fail here: the search can stop just because maxIterations
        // was exceeded
        if !line_search.succeeded() {
            problem.set_current_value(x);
            return Ok(ec_type);
        }

        x = line_search.last_x().clone();
        let fold = problem.function_value();
        problem.set_function_value(line_search.last_function_value());
        // orthogonalization coefficient
        let gold2 = problem.gradient_norm_value();
        problem.set_gradient_norm_value(line_search.last_gradient_norm2());

        let direction = updated_direction(problem, gold2, &prev_gradient, line_search);
        line_search.set_search_direction(direction);

        // Numerical Recipes exit strategy on f(x) (NR in C++, p.423)
        let fnew = problem.function_value();
        let fdiff = 2.0 * (fnew - fold).abs() / (fnew.abs() + fold.abs() + Real::EPSILON);
        if fdiff < ftol || end_criteria.check_max_iterations(iteration_number, &mut ec_type) {
            end_criteria.check_stationary_function_value(
                0.0,
                0.0,
                &mut max_stationary_state_iterations,
                &mut ec_type,
            );
            end_criteria.check_max_iterations(iteration_number, &mut ec_type);
            problem.set_current_value(x);
            return Ok(ec_type);
        }
        problem.set_current_value(x.clone());
        iteration_number += 1;
        first_time = false;
    }
}
