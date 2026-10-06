//! Finance-independent global solvers adapted to QuantLib calibration problems.

use crate::errors::{QlError, QlResult};
use crate::math::array::Array;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::require;
use itofin_optimize::{
    Bounds, Common, Converged, DifferentialEvolutionOptions, Method, Minimize, MinimizeError,
    Problem as ScalarProblem, Termination, minimize,
};

/// Bounded differential evolution accepted by model calibration.
///
/// Bounds and explicit populations use the projected, free-parameter order.
/// The supplied box must be wholly feasible for the model constraint. Every
/// candidate is tested before pricing; a rejected candidate aborts the run
/// rather than evaluating invalid model parameters or inventing a penalty.
/// General coupled constraints are not supported by this box-based search.
#[derive(Debug, Clone)]
pub struct DifferentialEvolution {
    bounds: Bounds,
    options: DifferentialEvolutionOptions,
    common: Common,
    last_result: Option<Minimize>,
}

impl DifferentialEvolution {
    /// Constructs a solver with explicit finite bounds and deterministic seed.
    ///
    /// # Errors
    /// Rejects invalid bounds, options or budgets. Calibration evaluation
    /// budgets are capped at ten million to keep legacy integer counts safe.
    pub fn new(
        bounds: Bounds,
        options: DifferentialEvolutionOptions,
        common: Common,
    ) -> QlResult<Self> {
        require!(
            bounds
                .lower
                .iter()
                .chain(&bounds.upper)
                .all(|v| v.is_finite()),
            "differential evolution calibration requires finite bounds"
        );
        let validation = ScalarProblem {
            x0: bounds.lower.clone(),
            bounds: Some(bounds.clone()),
        };
        validate(&validation, &options, &common)?;
        let common = options
            .budgets(&common)
            .map_err(|e| ql_error(e.to_string()))?;
        Ok(Self {
            bounds,
            options,
            common,
            last_result: None,
        })
    }

    /// The exact optimizer outcome of the last completed solver invocation.
    ///
    /// Iteration/evaluation exhaustion is retained without claiming convergence.
    /// Invalid input and objective failures inside a solver invocation clear previous
    /// results; earlier model preflight failures retain them. Nonfinite
    /// termination retains its diagnostics while returning a calibration error.
    pub fn last_result(&self) -> Option<&Minimize> {
        self.last_result.as_ref()
    }
}

fn ql_error(message: impl Into<String>) -> QlError {
    QlError::new(message, file!(), line!())
}

fn validate(
    problem: &ScalarProblem,
    options: &DifferentialEvolutionOptions,
    common: &Common,
) -> QlResult<()> {
    problem.validate().map_err(|e| ql_error(e.to_string()))?;
    common.validate().map_err(|e| ql_error(e.to_string()))?;
    Method::DifferentialEvolution(options.clone())
        .validate(problem)
        .map_err(|e| ql_error(e.to_string()))?;
    options
        .budgets(common)
        .map_err(|e| ql_error(e.to_string()))?;
    Ok(())
}

impl OptimizationMethod for DifferentialEvolution {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        self.last_result = None;
        let initial = problem.current_value().clone();
        require!(
            initial.size() == self.bounds.lower.len(),
            "calibration bounds must match the free parameter count"
        );
        require!(
            problem.constraint().test(&initial),
            "initial calibration parameters violate the constraint"
        );
        let lower = problem.constraint().lower_bound(&initial);
        let upper = problem.constraint().upper_bound(&initial);
        require!(
            lower.size() == initial.size() && upper.size() == initial.size(),
            "constraint bounds must match the free parameter count"
        );
        require!(
            lower.iter().chain(upper.iter()).all(|v| !v.is_nan()),
            "constraint bounds must not contain NaN"
        );
        let bounds = Bounds {
            lower: self
                .bounds
                .lower
                .iter()
                .zip(lower.iter())
                .map(|(a, b)| a.max(*b))
                .collect(),
            upper: self
                .bounds
                .upper
                .iter()
                .zip(upper.iter())
                .map(|(a, b)| a.min(*b))
                .collect(),
        };
        require!(
            bounds
                .lower
                .iter()
                .zip(&bounds.upper)
                .all(|(lower, upper)| lower <= upper),
            "calibration bounds have an empty constraint intersection"
        );
        let x0: Vec<f64> = initial
            .iter()
            .zip(bounds.lower.iter().zip(&bounds.upper))
            .map(|(value, (lower, upper))| value.clamp(*lower, *upper))
            .collect();
        require!(
            problem.constraint().test(&Array::from(x0.clone())),
            "bounded initial calibration candidate violates the constraint"
        );
        let scalar = ScalarProblem {
            x0,
            bounds: Some(bounds),
        };
        let common = Common {
            maxiter: Some(
                self.common
                    .maxiter
                    .unwrap_or(end_criteria.max_iterations())
                    .min(end_criteria.max_iterations()),
            ),
            ..self.common.clone()
        };
        validate(&scalar, &self.options, &common)?;
        problem.reset();
        let mut objective = |coordinates: &[f64]| -> QlResult<f64> {
            let candidate = Array::from(coordinates.to_vec());
            require!(
                problem.constraint().test(&candidate),
                "differential evolution candidate violates the calibration constraint; supply a wholly feasible box"
            );
            Ok(problem.value(&candidate))
        };
        let result = minimize(
            &mut objective,
            &scalar,
            &Method::DifferentialEvolution(self.options.clone()),
            &common,
        )
        .map_err(|error| match error {
            MinimizeError::InvalidInput(error) => ql_error(error.to_string()),
            MinimizeError::Objective(error) => error,
        })?;
        let outcome = match result.status {
            Termination::Converged(Converged::XTol) => Ok(EndCriteriaType::StationaryPoint),
            Termination::Converged(Converged::FTol) => Ok(EndCriteriaType::StationaryFunctionValue),
            Termination::MaxIterations => Ok(EndCriteriaType::MaxIterations),
            Termination::MaxEvaluations => Ok(EndCriteriaType::Unknown),
            status => Err(ql_error(format!("global calibration stopped: {status}"))),
        };
        if outcome.is_ok() {
            problem.set_current_value(Array::from(result.x.clone()));
            problem.set_function_value(result.fun);
        }
        self.last_result = Some(result);
        outcome
    }

    fn global_result(&self) -> Option<&Minimize> {
        self.last_result()
    }
}

#[cfg(test)]
mod tests;
