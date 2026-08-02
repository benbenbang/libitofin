//! Financial instruments.
//!
//! Port of `ql/instruments/`: the payoff subset and the vanilla-option
//! instruments needed by the European-option slice.

mod bond;
mod bonds;
mod capfloor;
mod claim;
mod fixedvsfloatingswap;
mod futures;
mod makecapfloor;
mod makeois;
mod makeswaption;
mod makevanillaswap;
mod oneassetoption;
mod overnightindexedswap;
mod payoffs;
mod protection;
mod swap;
mod swaption;
mod vanillaswap;

pub use bond::{Bond, BondArguments, BondEngine, BondPrice, BondPriceType, BondResults};
pub use bonds::FixedRateBond;
pub use capfloor::{CapFloor, CapFloorArguments, CapFloorType};
pub use claim::{Claim, FaceValueClaim};
pub use fixedvsfloatingswap::{
    FixedVsFloatingSwap, FixedVsFloatingSwapArguments, FixedVsFloatingSwapEngine,
    FixedVsFloatingSwapResults, FloatingArgumentsFn,
};
pub use futures::FuturesType;
pub use makecapfloor::MakeCapFloor;
pub use makeois::MakeOis;
pub use makeswaption::MakeSwaption;
pub use makevanillaswap::MakeVanillaSwap;
pub use oneassetoption::{
    EuropeanOption, Greeks, MoreGreeks, OneAssetOption, OneAssetOptionEngine,
    OneAssetOptionResults, OptionArguments, VanillaOption,
};
pub use overnightindexedswap::OvernightIndexedSwap;
pub use payoffs::{CashOrNothingPayoff, PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
pub use protection::ProtectionSide;
pub use swap::{Swap, SwapArguments, SwapEngine, SwapResults, SwapType};
pub use swaption::{
    SettlementMethod, SettlementType, Swaption, SwaptionArguments, SwaptionEngine,
    check_type_and_method_consistency,
};
pub use vanillaswap::VanillaSwap;
