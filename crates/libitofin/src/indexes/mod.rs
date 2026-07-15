//! Indexes.
//!
//! Port of `ql/index.hpp` and `ql/indexes/`. The abstract [`Index`] base and
//! its interest-rate refinement [`InterestRateIndex`] land here; concrete
//! indexes (an `IborIndex`) follow. Items are re-exported flat, so the base is
//! `indexes::Index` rather than `indexes::index::Index`.

pub mod ibor;
pub mod iborindex;
pub mod index;
pub mod interestrateindex;
pub mod swapindex;

pub use ibor::{Estr, Euribor, Sofr};
pub use iborindex::{IborIndex, OvernightIndex};
pub use index::Index;
pub use interestrateindex::{InterestRateIndex, InterestRateIndexBase};
pub use swapindex::SwapIndex;
