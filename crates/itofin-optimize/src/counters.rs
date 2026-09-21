use crate::error::MinimizeError;
use crate::objective::{Flow, IterationState, Objective};
use crate::outcome::Termination;
use crate::problem::Common;

/// Why a step returned no value: the run is over, or it failed.
#[derive(Debug)]
pub enum Halt<E: std::error::Error + 'static> {
    /// Stop and report this status together with the best iterate so far.
    Terminated(Termination),
    /// Give up and return this error.
    Failed(MinimizeError<E>),
}

/// Evaluation and iteration accounting. Every counter moves here and nowhere
/// else, and the budgets are enforced at the same place.
#[derive(Debug, Clone)]
pub struct Counters {
    nit: usize,
    nfev: usize,
    njev: usize,
    maxiter: Option<usize>,
    maxfev: Option<usize>,
}

impl Counters {
    /// Starts accounting against the budgets in `common`.
    pub fn new(common: &Common) -> Self {
        Self {
            nit: 0,
            nfev: 0,
            njev: 0,
            maxiter: common.maxiter,
            maxfev: common.maxfev,
        }
    }

    /// Iterations completed.
    pub fn nit(&self) -> usize {
        self.nit
    }

    /// Objective evaluations charged.
    pub fn nfev(&self) -> usize {
        self.nfev
    }

    /// Gradient evaluations charged.
    pub fn njev(&self) -> usize {
        self.njev
    }

    /// Evaluates the objective and charges one evaluation.
    ///
    /// The budget is checked before the call, so `nfev` never exceeds `maxfev`
    /// and an exhausted budget costs no evaluation.
    pub fn value<O: Objective>(
        &mut self,
        objective: &mut O,
        x: &[f64],
    ) -> Result<f64, Halt<O::Error>> {
        if self.maxfev.is_some_and(|budget| self.nfev >= budget) {
            return Err(Halt::Terminated(Termination::MaxEvaluations));
        }
        self.nfev += 1;
        objective.value(x).map_err(failed)
    }

    /// Asks the objective for its gradient, charging one gradient evaluation
    /// only when it supplies one.
    pub fn gradient<O: Objective>(
        &mut self,
        objective: &mut O,
        x: &[f64],
        out: &mut [f64],
    ) -> Result<bool, Halt<O::Error>> {
        let supplied = objective.gradient(x, out).map_err(failed)?;
        if supplied {
            self.njev += 1;
        }
        Ok(supplied)
    }

    /// Closes an iteration: counts it, dispatches the callback, then checks the
    /// iteration budget. A cancellation wins over budget exhaustion.
    pub fn end_iteration<O: Objective>(
        &mut self,
        objective: &mut O,
        x: &[f64],
        fun: f64,
    ) -> Result<(), Halt<O::Error>> {
        self.nit += 1;
        let state = IterationState {
            x,
            fun,
            nit: self.nit,
            nfev: self.nfev,
            njev: self.njev,
        };
        if objective.callback(&state).map_err(failed)? == Flow::Stop {
            return Err(Halt::Terminated(Termination::Cancelled));
        }
        if self.maxiter.is_some_and(|budget| self.nit >= budget) {
            return Err(Halt::Terminated(Termination::MaxIterations));
        }
        Ok(())
    }
}

fn failed<E: std::error::Error + 'static>(error: E) -> Halt<E> {
    Halt::Failed(MinimizeError::Objective(error))
}
