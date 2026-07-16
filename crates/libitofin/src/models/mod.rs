//! Interest-rate models.
//!
//! Port of `ql/models/`. This foundation covers the closed-form short-rate
//! affine spine: [`Parameter`] arguments and the [`CalibratedModel`] holder
//! that stores them and broadcasts changes. Calibration, the numerical
//! tree/lattice and the short-rate dynamics are deferred to later tickets; each
//! deferral is noted at the type it belongs to.

pub mod model;
pub mod parameter;

pub use model::{CalibratedModel, PrivateConstraint};
pub use parameter::{ConstantParameter, NullParameter, Parameter};
