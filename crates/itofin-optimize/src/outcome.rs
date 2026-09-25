use std::fmt;

/// Which tolerance a converged run satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Converged {
    /// The step or simplex size fell below the `x` tolerance.
    XTol,
    /// The objective spread fell below the `f` tolerance.
    FTol,
    /// The gradient norm fell below the `g` tolerance.
    GTol,
}

/// Why a run stopped. Later tickets add variants, hence `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Termination {
    /// A tolerance was met.
    Converged(Converged),
    /// The iteration budget was exhausted.
    MaxIterations,
    /// The evaluation budget was exhausted.
    MaxEvaluations,
    /// A callback returned [`Flow::Stop`](crate::Flow::Stop).
    Cancelled,
    /// A value the method cannot continue from ended the search.
    Nonfinite,
    /// No step along the search direction satisfied the strong Wolfe
    /// conditions.
    LineSearchFailed,
}

impl Termination {
    /// Whether this status counts as a successful run. Only convergence does.
    pub fn is_success(self) -> bool {
        matches!(self, Termination::Converged(_))
    }
}

impl fmt::Display for Termination {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Termination::Converged(Converged::XTol) => "converged: step below the x tolerance",
            Termination::Converged(Converged::FTol) => "converged: change below the f tolerance",
            Termination::Converged(Converged::GTol) => "converged: gradient below the g tolerance",
            Termination::MaxIterations => "maximum number of iterations reached",
            Termination::MaxEvaluations => "maximum number of function evaluations reached",
            Termination::Cancelled => "stopped by the callback",
            Termination::Nonfinite => "a nonfinite value ended the search",
            Termination::LineSearchFailed => {
                "line search failed: no step satisfies the strong Wolfe conditions"
            }
        })
    }
}

/// The outcome of a run. Later tickets add fields, hence `#[non_exhaustive]`.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq)]
pub struct Minimize {
    /// The best point found.
    pub x: Vec<f64>,
    /// The objective value at `x`.
    pub fun: f64,
    /// Iterations completed.
    pub nit: usize,
    /// Objective evaluations charged.
    pub nfev: usize,
    /// Gradient evaluations charged.
    pub njev: usize,
    /// Why the run stopped.
    pub status: Termination,
    /// Whether `status` is a convergence.
    pub success: bool,
    /// A human reading of `status`.
    pub message: String,
}

impl Minimize {
    /// Builds a result from the best iterate and the counts that produced it.
    ///
    /// `success` and `message` are derived from `status`, so a solver never sets
    /// them out of step and the struct stays constructible despite
    /// `#[non_exhaustive]`.
    pub fn new(
        x: Vec<f64>,
        fun: f64,
        nit: usize,
        nfev: usize,
        njev: usize,
        status: Termination,
    ) -> Self {
        Self {
            x,
            fun,
            nit,
            nfev,
            njev,
            status,
            success: status.is_success(),
            message: status.to_string(),
        }
    }
}
