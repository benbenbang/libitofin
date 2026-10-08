//! Finance-independent global solvers adapted to QuantLib calibration problems.

use crate::errors::{QlError, QlResult};
use crate::math::array::Array;
use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::require;
use itofin_optimize::{
    Bounds, Common, Converged, DifferentialEvolutionOptions, Method, Minimize, MinimizeError,
    ParticleSwarmOptions, Problem as ScalarProblem, Termination, minimize,
};

#[derive(Debug, Clone)]
struct CalibrationAdapter {
    bounds: Bounds,
    method: Method,
    common: Common,
    last_result: Option<Minimize>,
}

impl CalibrationAdapter {
    fn new(bounds: Bounds, method: Method, common: Common) -> QlResult<Self> {
        require!(
            bounds
                .lower
                .iter()
                .chain(&bounds.upper)
                .all(|v| v.is_finite()),
            "{} calibration requires finite bounds",
            solver_name(&method)
        );
        let validation = ScalarProblem {
            x0: bounds.lower.clone(),
            bounds: Some(bounds.clone()),
        };
        validate(&validation, &method, &common)?;
        let common = budgets(&method, &common)?;
        Ok(Self {
            bounds,
            method,
            common,
            last_result: None,
        })
    }

    fn last_result(&self) -> Option<&Minimize> {
        self.last_result.as_ref()
    }
}

fn budgets(method: &Method, common: &Common) -> QlResult<Common> {
    let result = match method {
        Method::DifferentialEvolution(options) => options.budgets(common),
        Method::ParticleSwarm(options) => options.budgets(common),
        _ => return Err(ql_error("calibration adapter requires a global solver")),
    };
    result.map_err(|e| ql_error(e.to_string()))
}

fn solver_name(method: &Method) -> &'static str {
    match method {
        Method::ParticleSwarm(_) => "particle swarm",
        _ => "differential evolution",
    }
}

fn ql_error(message: impl Into<String>) -> QlError {
    QlError::new(message, file!(), line!())
}

fn validate(problem: &ScalarProblem, method: &Method, common: &Common) -> QlResult<()> {
    problem.validate().map_err(|e| ql_error(e.to_string()))?;
    common.validate().map_err(|e| ql_error(e.to_string()))?;
    method
        .validate(problem)
        .map_err(|e| ql_error(e.to_string()))?;
    budgets(method, common)?;
    Ok(())
}

impl CalibrationAdapter {
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
        validate(&scalar, &self.method, &common)?;
        problem.reset();
        let mut objective = |coordinates: &[f64]| -> QlResult<f64> {
            let candidate = Array::from(coordinates.to_vec());
            require!(
                problem.constraint().test(&candidate),
                "{} candidate violates the calibration constraint; supply a wholly feasible box",
                solver_name(&self.method)
            );
            Ok(problem.value(&candidate))
        };
        let result =
            minimize(&mut objective, &scalar, &self.method, &common).map_err(
                |error| match error {
                    MinimizeError::InvalidInput(error) => ql_error(error.to_string()),
                    MinimizeError::Objective(error) => error,
                },
            )?;
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
}

/// Bounded differential evolution accepted by model calibration.
///
/// Bounds and explicit populations use the projected, free-parameter order.
/// The supplied box must be wholly feasible for the model constraint. Every
/// candidate is tested before pricing; a rejected candidate aborts the run
/// rather than evaluating invalid model parameters or inventing a penalty.
/// General coupled constraints are not supported by this box-based search.
#[derive(Debug, Clone)]
pub struct DifferentialEvolution {
    adapter: CalibrationAdapter,
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
        Ok(Self {
            adapter: CalibrationAdapter::new(
                bounds,
                Method::DifferentialEvolution(options),
                common,
            )?,
        })
    }

    /// The exact optimizer outcome of the last completed solver invocation.
    ///
    /// Exhaustion is retained without claiming convergence. Invalid input and
    /// objective failures during invocation clear previous results; earlier
    /// model preflight failures retain them. Nonfinite termination retains
    /// diagnostics while returning a calibration error.
    pub fn last_result(&self) -> Option<&Minimize> {
        self.adapter.last_result()
    }
}

impl OptimizationMethod for DifferentialEvolution {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        self.adapter.minimize(problem, end_criteria)
    }

    fn global_result(&self) -> Option<&Minimize> {
        self.last_result()
    }
}

/// Bounded particle swarm accepted by model calibration.
///
/// Bounds and explicit populations use the projected, free-parameter order.
/// The supplied box must be wholly feasible for the model constraint. Every
/// candidate is tested before pricing; a rejected candidate aborts the run
/// rather than evaluating invalid model parameters or inventing a penalty.
/// General coupled constraints are not supported by this box-based search.
#[derive(Debug, Clone)]
pub struct ParticleSwarm {
    adapter: CalibrationAdapter,
}

impl ParticleSwarm {
    /// Constructs a solver with explicit finite bounds and deterministic seed.
    ///
    /// # Errors
    /// Rejects invalid bounds, options or budgets. Calibration evaluation
    /// budgets are capped at ten million to keep legacy integer counts safe.
    pub fn new(bounds: Bounds, options: ParticleSwarmOptions, common: Common) -> QlResult<Self> {
        Ok(Self {
            adapter: CalibrationAdapter::new(bounds, Method::ParticleSwarm(options), common)?,
        })
    }

    /// The exact optimizer outcome of the last completed solver invocation.
    ///
    /// Exhaustion is retained without claiming convergence. Invalid input and
    /// objective failures during invocation clear previous results; earlier
    /// model preflight failures retain them. Nonfinite termination retains
    /// diagnostics while returning a calibration error.
    pub fn last_result(&self) -> Option<&Minimize> {
        self.adapter.last_result()
    }
}

impl OptimizationMethod for ParticleSwarm {
    fn minimize(
        &mut self,
        problem: &mut Problem<'_>,
        end_criteria: &EndCriteria,
    ) -> QlResult<EndCriteriaType> {
        self.adapter.minimize(problem, end_criteria)
    }

    fn global_result(&self) -> Option<&Minimize> {
        self.last_result()
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod particle_swarm_tests;
