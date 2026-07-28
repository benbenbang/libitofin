//! Finite-difference methods (L9).
//!
//! Port of `ql/methods/finitedifferences/`. The operator layer lands first,
//! then the meshers that give its index space a geometry, then the utilities
//! that compute over a finished grid, then the conditions a step must respect,
//! then the schemes that take one timestep; the solver driving them stacks on
//! all of them in #658.

mod boundarycondition;
mod stepcondition;

pub mod meshers;
pub mod operators;
pub mod schemes;
pub mod solvers;
pub mod stepconditions;
pub mod utilities;

pub use boundarycondition::{BoundaryCondition, BoundarySide};
pub use stepcondition::{NullCondition, StepCondition};
