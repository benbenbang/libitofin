//! C ABI adapters. The Rust core remains independent of this crate.
//!
//! # C caller contract
//! Every non-null pointer must be aligned, live, and valid for its stated size.
//! Output buffers must not overlap inputs, contexts, or other outputs. A context
//! and all its handles belong to the thread that created it. Calls on a context
//! must be serialized, including destruction. Never reuse a destroyed context.
//! Go enforces these rules through a session worker locked to one OS thread.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod bootstrap_api;
pub mod bootstrap_callbacks;
pub mod bootstrap_variables;
pub mod boundary;
pub mod simulation_api;
pub mod simulation_kernel;
pub use boundary::{Context, ItofinError};
pub mod calendar_api;
pub mod cashflows_api;
pub mod credit_api;
pub mod credit_helpers_api;
pub mod credit_instruments_api;
pub mod curves_api;
pub mod helpers_api;
pub mod indexes_api;
pub mod inflation_api;
pub mod inflation_capfloor_api;
pub mod inflation_cashflows_api;
pub mod inflation_curves_api;
pub mod inflation_helpers_api;
pub mod inflation_products_api;
pub mod inflation_seasonality_api;
pub mod inflation_vol_api;
pub mod inflation_volgrid_api;
pub mod market_api;
pub mod mc_api;
pub mod models_api;
pub mod options_api;
pub mod rates_api;
pub mod rates_engines;
pub mod rates_fra;
pub mod rates_index;
pub mod rates_options;
pub mod ratevol_api;
pub mod results_api;
pub mod rng_api;
pub mod rng_gaussian_sobol;
pub mod rng_low_discrepancy;
pub mod rng_sequence_api;
pub mod settings_api;
pub mod smile_api;
pub mod stripper_api;
pub mod time_api;
pub mod version_api;
pub mod vol_api;
#[cfg(test)]
mod vol_tests;
pub mod volcube_api;
pub mod volgrid_api;

#[cfg(test)]
mod swaption_facades_tests;
