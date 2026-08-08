//! Inflation term structures.
//!
//! Port of `ql/termstructures/inflationtermstructure.{hpp,cpp}` and the curves
//! under `ql/termstructures/inflation/`. The
//! [`ZeroInflationTermStructure`](inflationtermstructure::ZeroInflationTermStructure)
//! trait is the contract every zero-coupon inflation curve plugs into; the
//! interpolated curves and the seasonality corrections build on it; the
//! year-on-year structures follow within EPIC Inflation (#705).

pub mod inflationhelpers;
pub mod inflationtermstructure;
pub mod inflationtraits;
pub mod interpolatedzeroinflationcurve;
pub mod piecewisezeroinflationcurve;
pub mod seasonality;

pub use seasonality::{MultiplicativePriceSeasonality, Seasonality};
