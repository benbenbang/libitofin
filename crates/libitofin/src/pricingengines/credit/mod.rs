//! Credit pricing engines.
//!
//! Port of `ql/pricingengines/credit/`: the engines that price the credit
//! instruments of EPIC Credit (#676) over a default-probability curve.

pub mod integralcdsengine;
pub mod midpointcdsengine;

pub use integralcdsengine::IntegralCdsEngine;
pub use midpointcdsengine::MidPointCdsEngine;
