//! Backward solvers over a finite-difference grid.
//!
//! Port of `ql/methods/finitedifferences/solvers/`. The descriptor a solver
//! picks its scheme from lands first; the solver itself follows in #658.

mod fdmschemedesc;

pub use fdmschemedesc::{FdmSchemeDesc, FdmSchemeType};
