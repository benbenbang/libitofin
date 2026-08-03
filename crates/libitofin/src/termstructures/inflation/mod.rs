//! Inflation term structures.
//!
//! Port of `ql/termstructures/inflationtermstructure.{hpp,cpp}` and the curves
//! under `ql/termstructures/inflation/`. The
//! [`ZeroInflationTermStructure`](inflationtermstructure::ZeroInflationTermStructure)
//! trait is the contract every zero-coupon inflation curve plugs into; the
//! interpolated curves, the year-on-year structures and the seasonality
//! classes that build on it follow within EPIC Inflation (#705).

pub mod inflationtermstructure;
pub mod interpolatedzeroinflationcurve;
