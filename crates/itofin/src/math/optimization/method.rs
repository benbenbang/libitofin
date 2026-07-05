//! Abstract optimization method.
//!
//! Port of `ql/math/optimization/method.hpp`. The C++ failures raised inside
//! `minimize` (infeasible starting points, stalled updates) surface as
//! `Err` instead of exceptions.

use crate::errors::QlResult;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::problem::Problem;

/// A constrained optimization method.
pub trait OptimizationMethod {
    /// Minimizes `problem`, stopping when `end_criteria` are met, and returns
    /// the criterion that ended the run.
    ///
    /// # Errors
    ///
    /// Fails when the method cannot make progress, e.g. an infeasible
    /// starting point or invalid method inputs.
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType>;
}
