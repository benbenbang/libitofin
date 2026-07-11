//! Financial instruments.
//!
//! Port of `ql/instruments/`: the payoff subset and the vanilla-option
//! instruments needed by the European-option slice.

mod bond;
mod oneassetoption;
mod payoffs;

pub use bond::{Bond, BondArguments, BondEngine, BondResults};
pub use oneassetoption::{
    EuropeanOption, Greeks, MoreGreeks, OneAssetOption, OneAssetOptionEngine,
    OneAssetOptionResults, OptionArguments, VanillaOption,
};
pub use payoffs::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
