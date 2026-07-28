//! Time-stepping schemes for the finite-difference solver.
//!
//! Port of `ql/methods/finitedifferences/schemes/`. The boundary-condition
//! helper the schemes all hold landed first; #657 adds the [`Scheme`] contract
//! they meet and the two schemes the backward solver of #658 drives.

mod boundaryconditionschemehelper;
mod douglasscheme;
mod scheme;
#[cfg(test)]
mod testops;

pub use boundaryconditionschemehelper::BoundaryConditionSchemeHelper;
pub use douglasscheme::DouglasScheme;
pub use scheme::Scheme;
