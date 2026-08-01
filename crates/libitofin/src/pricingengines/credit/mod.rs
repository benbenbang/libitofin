//! Credit pricing engines.
//!
//! Port of `ql/pricingengines/credit/`: the engines that price the credit
//! instruments of EPIC Credit (#676) over a default-probability curve.

pub mod midpointcdsengine;

pub use midpointcdsengine::MidPointCdsEngine;
