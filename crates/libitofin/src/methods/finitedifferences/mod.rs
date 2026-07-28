//! Finite-difference methods (L9).
//!
//! Port of `ql/methods/finitedifferences/`. The operator layer lands first,
//! then the meshers that give its index space a geometry, then the utilities
//! that compute over a finished grid, then the conditions a step must respect;
//! the schemes and solvers stack on all of them in later tickets.

mod boundarycondition;
mod stepcondition;

pub mod meshers;
pub mod operators;
pub mod schemes;
pub mod stepconditions;
pub mod utilities;

pub use boundarycondition::{BoundaryCondition, BoundarySide};
pub use stepcondition::{NullCondition, StepCondition};
