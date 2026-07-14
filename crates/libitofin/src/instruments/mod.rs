//! Financial instruments.
//!
//! Port of `ql/instruments/`: the payoff subset and the vanilla-option
//! instruments needed by the European-option slice.

mod bond;
mod bonds;
mod capfloor;
mod fixedvsfloatingswap;
mod makeois;
mod makevanillaswap;
mod oneassetoption;
mod overnightindexedswap;
mod payoffs;
mod swap;
mod swaption;
mod vanillaswap;

pub use bond::{Bond, BondArguments, BondEngine, BondPrice, BondResults};
pub use bonds::FixedRateBond;
pub use capfloor::{CapFloor, CapFloorArguments, CapFloorType};
pub use fixedvsfloatingswap::{
    FixedVsFloatingSwap, FixedVsFloatingSwapArguments, FixedVsFloatingSwapEngine,
    FixedVsFloatingSwapResults, FloatingArgumentsFn,
};
pub use makeois::MakeOis;
pub use makevanillaswap::MakeVanillaSwap;
pub use oneassetoption::{
    EuropeanOption, Greeks, MoreGreeks, OneAssetOption, OneAssetOptionEngine,
    OneAssetOptionResults, OptionArguments, VanillaOption,
};
pub use overnightindexedswap::OvernightIndexedSwap;
pub use payoffs::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
pub use swap::{Swap, SwapArguments, SwapEngine, SwapResults, SwapType};
pub use swaption::{SettlementMethod, SettlementType, check_type_and_method_consistency};
pub use vanillaswap::VanillaSwap;
