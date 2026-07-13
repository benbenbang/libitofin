//! Swap pricing engines.
//!
//! Port of `ql/pricingengines/swap/`: the engines a [`Swap`] is priced through.
//!
//! [`Swap`]: crate::instruments::Swap

mod discountingswapengine;

pub use discountingswapengine::DiscountingSwapEngine;
