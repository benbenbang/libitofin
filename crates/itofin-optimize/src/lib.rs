//! SciPy-inspired numerical optimization over plain `f64` slices.
//!
//! This crate is finance-independent and never depends on `libitofin`. It holds
//! the contracts every solver shares: the [`Objective`] trait, the [`Minimize`]
//! result with its [`Termination`] status, budget and cancellation accounting,
//! and input validation.
//!
//! [`minimize`] is the single entry point: it validates the problem, the shared
//! options and the method, then dispatches to the solver the method names.
//!
//! # Provenance
//!
//! An independent implementation from the papers below. Nothing is adapted from
//! another optimizer; see `THIRD_PARTY_NOTICES.md` in this crate.
//!
//! - Nelder, J. A. and Mead, R. (1965), "A simplex method for function
//!   minimization", The Computer Journal 7(4), 308-313.
//! - Gao, F. and Han, L. (2012), "Implementing the Nelder-Mead simplex
//!   algorithm with adaptive parameters", Computational Optimization and
//!   Applications 51(1), 259-277.
//! - Nocedal, J. and Wright, S. J. (2006), Numerical Optimization, 2nd edition,
//!   Springer.
//! - Byrd, R. H., Lu, P., Nocedal, J. and Zhu, C. (1995), "A limited memory
//!   algorithm for bound constrained optimization", SIAM Journal on Scientific
//!   Computing 16(5), 1190-1208.
//! - Kraft, D. (1988), "A software package for sequential quadratic
//!   programming", DFVLR-FB 88-28, DLR German Aerospace Center.
//! - Lawson, C. L. and Hanson, R. J. (1974), Solving Least Squares Problems,
//!   Prentice-Hall.

mod counters;
mod error;
mod nelder_mead;
mod objective;
mod outcome;
mod problem;

#[cfg(test)]
mod tests;

pub use counters::{Counters, Halt};
pub use error::{InvalidInput, MinimizeError};
pub use objective::{Flow, IterationState, Objective};
pub use outcome::{Converged, Minimize, Termination};
pub use problem::{Bounds, Common, Method, NelderMeadOptions, Problem};

/// Minimizes `objective` from `problem.x0` with the chosen `method`.
///
/// Validation runs first and in a fixed order: the problem, then the shared
/// options, then the method against the problem. A run that reaches a solver
/// always returns `Ok`, carrying the best point found and the
/// [`Termination`] that ended it; only a rejected input or a failing objective
/// is an `Err`.
///
/// # Errors
///
/// [`MinimizeError::InvalidInput`] when the problem, the budgets, the
/// tolerances or the method combination is rejected, and
/// [`MinimizeError::Objective`] when the objective or its callback fails,
/// carrying that error by value.
///
/// # Examples
///
/// ```
/// use itofin_optimize::{Common, Method, NelderMeadOptions, Problem, minimize};
/// use std::convert::Infallible;
///
/// let mut objective = |x: &[f64]| -> Result<f64, Infallible> { Ok((x[0] - 3.0).powi(2)) };
/// let problem = Problem {
///     x0: vec![0.0],
///     bounds: None,
/// };
/// let method = Method::NelderMead(NelderMeadOptions::default());
/// let result = minimize(&mut objective, &problem, &method, &Common::default())?;
///
/// assert!(result.success);
/// assert!((result.x[0] - 3.0).abs() < 1e-3);
/// # Ok::<(), itofin_optimize::MinimizeError<Infallible>>(())
/// ```
pub fn minimize<O: Objective>(
    objective: &mut O,
    problem: &Problem,
    method: &Method,
    common: &Common,
) -> Result<Minimize, MinimizeError<O::Error>> {
    problem.validate()?;
    common.validate()?;
    method.validate(problem)?;
    match method {
        Method::NelderMead(options) => nelder_mead::minimize(objective, problem, options, common),
    }
}
