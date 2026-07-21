//! Cap/floor pricing engines.
//!
//! Port of `ql/pricingengines/capfloor/`: the Black-formula engine that prices
//! a [`CapFloor`](crate::instruments::CapFloor) optionlet by optionlet, and the
//! analytic Hull-White engine that prices it as a portfolio of discount-bond
//! options.

mod analyticcapfloorengine;
mod blackcapfloorengine;

pub use analyticcapfloorengine::AnalyticCapFloorEngine;
pub use blackcapfloorengine::BlackCapFloorEngine;
