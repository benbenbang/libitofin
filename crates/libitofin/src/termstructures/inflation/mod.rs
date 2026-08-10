//! Inflation term structures.
//!
//! Port of `ql/termstructures/inflationtermstructure.{hpp,cpp}` and the curves
//! under `ql/termstructures/inflation/`. The
//! [`ZeroInflationTermStructure`](inflationtermstructure::ZeroInflationTermStructure)
//! trait is the contract every zero-coupon inflation curve plugs into and
//! [`YoYInflationTermStructure`](inflationtermstructure::YoYInflationTermStructure)
//! the contract every year-on-year one plugs into; the interpolated and
//! piecewise curves and the seasonality corrections build on them.

pub mod inflationhelpers;
pub mod inflationtermstructure;
pub mod inflationtraits;
pub mod interpolatedyoyinflationcurve;
pub mod interpolatedzeroinflationcurve;
pub mod piecewiseyoyinflationcurve;
pub mod piecewisezeroinflationcurve;
pub mod seasonality;

pub use seasonality::{MultiplicativePriceSeasonality, Seasonality};
