//! Indexes.
//!
//! Port of `ql/index.hpp` and `ql/indexes/`. The abstract [`Index`] base and
//! its interest-rate refinement [`InterestRateIndex`] land here; concrete
//! indexes (an `IborIndex`) follow. Items are re-exported flat, so the base is
//! `indexes::Index` rather than `indexes::index::Index`.

pub mod index;
pub mod interestrateindex;

pub use index::Index;
pub use interestrateindex::{InterestRateIndex, InterestRateIndexBase};
