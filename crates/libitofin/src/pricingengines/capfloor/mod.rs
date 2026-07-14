//! Cap/floor pricing engines.
//!
//! Port of `ql/pricingengines/capfloor/`: the Black-formula engine that prices
//! a [`CapFloor`](crate::instruments::CapFloor) optionlet by optionlet.

mod blackcapfloorengine;

pub use blackcapfloorengine::BlackCapFloorEngine;
