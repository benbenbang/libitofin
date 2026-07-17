//! Interest-rate models.
//!
//! Port of `ql/models/`. This foundation covers the closed-form short-rate
//! affine spine: [`Parameter`] arguments, the [`CalibratedModel`] holder that
//! stores them and broadcasts changes, and the affine `discountBond` payoff (in
//! [`shortrate`]). Calibration, the numerical tree/lattice and the short-rate
//! dynamics are deferred to later tickets; each deferral is noted at the type it
//! belongs to.

pub mod model;
pub mod parameter;
pub mod shortrate;

pub use model::{
    CalibratedModel, CalibratedModelHolder, PrivateConstraint, TermStructureConsistentModel,
    register_with_term_structure,
};
pub use parameter::{
    ConstantParameter, NullParameter, Parameter, ParameterValue, TermStructureFittingParameter,
};
pub use shortrate::{
    AffineModel, CoxIngersollRoss, ExtendedCoxIngersollRoss, HullWhite, OneFactorAffineModel,
    Vasicek, VolatilityConstraint, convexity_bias,
};
