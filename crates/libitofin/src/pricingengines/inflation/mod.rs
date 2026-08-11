//! Inflation pricing engines.
//!
//! Port of `ql/pricingengines/inflation/`: the year-on-year cap/floor engines
//! and the optionlet formula they and the coupon pricers share.

pub mod inflationcapfloorengines;

pub use inflationcapfloorengines::{YoYInflationCapFloorEngine, yoy_optionlet_price};
