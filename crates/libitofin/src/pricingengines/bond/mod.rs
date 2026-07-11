//! Bond pricing helpers.
//!
//! Port of `ql/pricingengines/bond/`: the free-function analytics a [`Bond`]
//! is priced through.
//!
//! [`Bond`]: crate::instruments::Bond

mod bondfunctions;

pub use bondfunctions::BondFunctions;
