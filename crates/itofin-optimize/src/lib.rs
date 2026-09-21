//! SciPy-inspired numerical optimization over plain `f64` slices.
//!
//! This crate is finance-independent and never depends on `libitofin`. It holds
//! the contracts every solver shares: the [`Objective`] trait, the [`Minimize`]
//! result with its [`Termination`] status, budget and cancellation accounting,
//! and input validation.
//!
//! There is no public `minimize` entry point yet. It arrives with the first
//! solver (Nelder-Mead).

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
