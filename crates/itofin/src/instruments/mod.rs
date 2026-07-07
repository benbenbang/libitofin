//! Financial instruments.
//!
//! Port of `ql/instruments/`: currently the payoff subset needed by the
//! European-option slice. The instrument classes (`OneAssetOption`,
//! `VanillaOption`, `EuropeanOption`) follow with the instrument and
//! pricing-engine framework.

mod payoffs;

pub use payoffs::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
