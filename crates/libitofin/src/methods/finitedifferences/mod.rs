//! Finite-difference methods (L9).
//!
//! Port of `ql/methods/finitedifferences/`. The operator layer lands first,
//! then the meshers that give its index space a geometry, then the utilities
//! that compute over a finished grid; the schemes and solvers stack on all
//! three in later tickets.

pub mod meshers;
pub mod operators;
pub mod utilities;
