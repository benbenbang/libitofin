//! SciPy-inspired numerical optimization over plain `f64` slices.
//!
//! This crate is finance-independent and never depends on `libitofin`. It holds
//! the contracts every solver shares: the [`Objective`] trait, the [`Minimize`]
//! result with its [`Termination`] status, budget and cancellation accounting,
//! and input validation.
//!
//! There is no public `minimize` entry point yet. It arrives with the first
//! solver (Nelder-Mead).
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
