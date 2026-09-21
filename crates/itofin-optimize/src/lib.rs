//! SciPy-inspired numerical optimization over plain `f64` slices.
//!
//! This crate is finance-independent and never depends on `libitofin`. It holds
//! the contracts every solver shares: the [`Objective`] trait, the [`Minimize`]
//! result with its [`Termination`] status, budget and cancellation accounting,
//! and input validation.
//!
//! There is no public `minimize` entry point yet. It arrives with the first
//! solver (Nelder-Mead).

mod error;
mod objective;
mod outcome;

#[cfg(test)]
mod tests;

pub use error::{InvalidInput, MinimizeError};
pub use objective::{Flow, IterationState, Objective};
pub use outcome::{Converged, Minimize, Termination};
