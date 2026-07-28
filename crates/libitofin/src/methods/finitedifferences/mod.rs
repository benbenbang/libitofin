//! Finite-difference methods (L9).
//!
//! Port of `ql/methods/finitedifferences/`. The operator layer lands first,
//! then the meshers that give its index space a geometry; the schemes and
//! solvers stack on both in later tickets.

pub mod meshers;
pub mod operators;
