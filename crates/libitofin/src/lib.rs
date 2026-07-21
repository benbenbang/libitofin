//! itofin - a Rust port of QuantLib's quantitative finance core.
//!
//! The core library (`libitofin`). Language bindings (Python via PyO3, a C ABI
//! via cbindgen) live in sibling crates and depend on this one.

pub mod cashflow;
pub mod cashflows;
pub mod currency;
pub mod errors;
pub mod event;
pub mod exercise;
pub mod handle;
pub mod indexes;
pub mod instrument;
pub mod instruments;
pub mod interestrate;
pub mod math;
pub mod methods;
pub mod models;
pub mod option;
pub mod patterns;
pub mod payoff;
pub mod pricingengine;
pub mod pricingengines;
pub mod processes;
pub mod quotes;
pub mod settings;
pub mod shared;
pub mod stochasticprocess;
pub mod termstructures;
#[cfg(test)]
pub(crate) mod test_support;
pub mod time;
pub mod types;
pub mod utilities;
