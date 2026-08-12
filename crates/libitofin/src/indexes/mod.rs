//! Indexes.
//!
//! Port of `ql/index.hpp` and `ql/indexes/`. The abstract [`Index`] base and
//! its interest-rate refinement [`InterestRateIndex`] land here; concrete
//! indexes (an `IborIndex`) follow. Items are re-exported flat, so the base is
//! `indexes::Index` rather than `indexes::index::Index`.

pub mod ibor;
pub mod iborindex;
pub mod index;
pub mod inflation;
pub mod inflationindex;
pub mod interestrateindex;
pub mod region;
pub mod swapindex;

pub use ibor::{Estr, Euribor, Sofr};
pub use iborindex::{IborIndex, OvernightIndex};
pub use index::Index;
pub use inflation::{EuHicp, UkHicp, UkRpi, YyEuHicp, YyEuHicpXt, YyUkRpi};
pub use inflationindex::{
    Cpi, CpiInterpolationType, InflationIndex, InflationIndexBase, YoYInflationIndex,
    ZeroInflationIndex, inflation_period,
};
pub use interestrateindex::{InterestRateIndex, InterestRateIndexBase};
pub use region::Region;
pub use swapindex::SwapIndex;
