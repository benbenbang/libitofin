//! Short-rate models.
//!
//! Port of `ql/models/shortrate/`. Flat re-exports of the one-factor affine
//! surface; concrete models (Vasicek, ...) land in later tickets.

pub mod onefactormodel;

pub use onefactormodel::{AffineModel, OneFactorAffineModel};
