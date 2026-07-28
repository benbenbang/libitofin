//! Time-stepping schemes for the finite-difference solver.
//!
//! Port of `ql/methods/finitedifferences/schemes/`. The boundary-condition
//! helper the schemes all hold landed first, then the [`Scheme`] contract they
//! meet; the schemes themselves follow in #657.

mod boundaryconditionschemehelper;
mod scheme;

pub use boundaryconditionschemehelper::BoundaryConditionSchemeHelper;
pub use scheme::Scheme;
