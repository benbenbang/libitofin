//! Backward solvers over a finite-difference grid.
//!
//! Port of `ql/methods/finitedifferences/solvers/`. The descriptor a solver
//! picks its scheme from landed first, then the backward solver that switches
//! on it.

mod fdmbackwardsolver;
mod fdmschemedesc;

pub use fdmbackwardsolver::FdmBackwardSolver;
pub use fdmschemedesc::{FdmSchemeDesc, FdmSchemeType};
