//! Named concrete Ibor indexes.
//!
//! Port of `ql/indexes/ibor/`. The concrete named [`IborIndex`] family lands
//! here, starting with [`Euribor`], plus the named [`OvernightIndex`]
//! concretes [`Eonia`], [`Sofr`], and [`Estr`]. Items are re-exported flat, so
//! the index is `indexes::ibor::Euribor` rather than
//! `indexes::ibor::euribor::Euribor`.
//!
//! [`IborIndex`]: crate::indexes::iborindex::IborIndex
//! [`OvernightIndex`]: crate::indexes::iborindex::OvernightIndex

pub mod eonia;
pub mod estr;
pub mod euribor;
pub mod sofr;

pub use eonia::Eonia;
pub use estr::Estr;
pub use euribor::Euribor;
pub use sofr::Sofr;
