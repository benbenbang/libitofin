//! Finite-difference methods (L9).
//!
//! Port of `ql/methods/finitedifferences/`. The operator layer lands first,
//! then the meshers that give its index space a geometry, then the utilities
//! that compute over a finished grid, then the conditions a step must respect,
//! then the schemes that take one timestep, and last the rollback loop and the
//! backward solver that drive them.

mod boundarycondition;
mod finitedifferencemodel;
mod stepcondition;

pub mod meshers;
pub mod operators;
pub mod schemes;
pub mod solvers;
pub mod stepconditions;
pub mod utilities;

pub use boundarycondition::{BoundaryCondition, BoundarySide};
pub use finitedifferencemodel::FiniteDifferenceModel;
pub use stepcondition::{NullCondition, StepCondition};
