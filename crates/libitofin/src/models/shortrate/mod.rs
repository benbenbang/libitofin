//! Short-rate models.
//!
//! Port of `ql/models/shortrate/`. Flat re-exports of the one-factor affine
//! surface and its concrete models.

pub mod onefactormodel;
pub mod vasicek;

pub use onefactormodel::{AffineModel, OneFactorAffineModel};
pub use vasicek::Vasicek;
