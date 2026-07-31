//! Backward solvers over a finite-difference grid.
//!
//! Port of `ql/methods/finitedifferences/solvers/`. The descriptor a solver
//! picks its scheme from landed first, then the backward solver that switches
//! on it, then the descriptor of everything else a solver rolls back, and on
//! top of the three the one-dimensional driver that seeds a grid, hands it to
//! the backward solver and reads the answer off a spline. On top of that
//! driver again sits the Black-Scholes wrapper, which builds the generator the
//! driver rolls and turns its log-space read-offs into greeks at a spot.

mod fdm1dimsolver;
mod fdmbackwardsolver;
mod fdmblackscholessolver;
mod fdmschemedesc;
mod fdmsolverdesc;

pub use fdm1dimsolver::Fdm1DimSolver;
pub use fdmbackwardsolver::FdmBackwardSolver;
pub use fdmblackscholessolver::FdmBlackScholesSolver;
pub use fdmschemedesc::{FdmSchemeDesc, FdmSchemeType};
pub use fdmsolverdesc::FdmSolverDesc;
