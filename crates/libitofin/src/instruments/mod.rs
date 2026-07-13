//! Financial instruments.
//!
//! Port of `ql/instruments/`: the payoff subset and the vanilla-option
//! instruments needed by the European-option slice.

mod bond;
mod bonds;
mod fixedvsfloatingswap;
mod oneassetoption;
mod payoffs;
mod swap;

pub use bond::{Bond, BondArguments, BondEngine, BondPrice, BondResults};
pub use bonds::FixedRateBond;
pub use fixedvsfloatingswap::{
    FixedVsFloatingSwap, FixedVsFloatingSwapArguments, FixedVsFloatingSwapEngine,
    FixedVsFloatingSwapResults, FloatingArgumentsFn,
};
pub use oneassetoption::{
    EuropeanOption, Greeks, MoreGreeks, OneAssetOption, OneAssetOptionEngine,
    OneAssetOptionResults, OptionArguments, VanillaOption,
};
pub use payoffs::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
pub use swap::{Swap, SwapArguments, SwapEngine, SwapResults, SwapType};
