//! Backward solvers over a finite-difference grid.
//!
//! Port of `ql/methods/finitedifferences/solvers/`. The descriptor a solver
//! picks its scheme from landed first, then the backward solver that switches
//! on it, then the descriptor of everything else a solver rolls back.

mod fdmbackwardsolver;
mod fdmschemedesc;
mod fdmsolverdesc;

pub use fdmbackwardsolver::FdmBackwardSolver;
pub use fdmschemedesc::{FdmSchemeDesc, FdmSchemeType};
pub use fdmsolverdesc::FdmSolverDesc;
