/// What a per-iteration callback asks the solver to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    /// Keep iterating.
    Continue,
    /// Stop now and report [`Termination::Cancelled`](crate::Termination::Cancelled).
    Stop,
}

/// A read-only view of the solver state at the end of an iteration.
#[derive(Debug, Clone, Copy)]
pub struct IterationState<'a> {
    /// The current best point.
    pub x: &'a [f64],
    /// The objective value at `x`.
    pub fun: f64,
    /// Iterations completed.
    pub nit: usize,
    /// Objective evaluations charged so far.
    pub nfev: usize,
    /// Gradient evaluations charged so far.
    pub njev: usize,
}

/// The function a solver minimizes.
pub trait Objective {
    /// How an evaluation fails. Carried by value through
    /// [`MinimizeError::Objective`](crate::MinimizeError::Objective), never stringified.
    type Error: std::error::Error + 'static;

    /// Evaluates the objective at `x`.
    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error>;

    /// Writes the gradient at `x` into `out` and returns `true`.
    ///
    /// The default returns `false`: there is no analytic gradient, so a method
    /// that needs one approximates it.
    fn gradient(&mut self, _x: &[f64], _out: &mut [f64]) -> Result<bool, Self::Error> {
        Ok(false)
    }

    /// Runs once at the end of every iteration.
    ///
    /// `Ok(Flow::Stop)` cancels the run; `Err` fails it. A cancellation is not
    /// an error and an error is not a cancellation.
    fn callback(&mut self, _state: &IterationState<'_>) -> Result<Flow, Self::Error> {
        Ok(Flow::Continue)
    }
}

impl<F, E> Objective for F
where
    F: FnMut(&[f64]) -> Result<f64, E>,
    E: std::error::Error + 'static,
{
    type Error = E;

    fn value(&mut self, x: &[f64]) -> Result<f64, Self::Error> {
        self(x)
    }
}
