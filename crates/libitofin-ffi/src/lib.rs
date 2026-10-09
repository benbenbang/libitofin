//! C ABI adapters. The Rust core remains independent of this crate.
//!
//! # C caller contract
//! Every non-null pointer must be aligned, live, and valid for its stated size.
//! Output buffers must not overlap inputs, contexts, or other outputs. A context
//! and all its handles belong to the thread that created it. Calls on a context
//! must be serialized, including destruction. Never reuse a destroyed context.
//! Go enforces these rules through a session worker locked to one OS thread.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod bates_api;
pub mod bates_calibration_api;
pub mod bma_api;
pub mod bma_swap_api;
pub mod bootstrap_api;
pub mod bootstrap_callbacks;
pub mod bootstrap_variables;
pub mod boundary;
pub mod chart_api;
pub mod merton_paths_api;
pub mod ou_paths_api;
pub mod simulation_api;
pub use boundary::{Context, ItofinError};
pub mod benchmark_beta_api;
pub mod calendar_api;
pub mod cap_calibration_api;
pub mod cashflows_api;
pub mod constraint_api;
pub mod convergence_statistics_api;
pub mod credit_api;
pub mod credit_helpers_api;
pub mod credit_instruments_api;
pub mod curves_api;
pub mod discrepancy_statistics_api;
pub mod drawdown_api;
pub mod fd_engine_api;
pub mod garch_api;
pub mod general_statistics_api;
pub mod geometric_brownian_api;
pub mod gjr_api;
pub mod gjr_model_api;
pub mod global_calibration_api;
pub mod helpers_api;
pub mod heston_engines_api;
pub mod incremental_statistics_api;
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
pub mod iterative_bootstrap_api;
pub mod joint_curves_api;
pub mod market_api;
pub mod mc_api;
pub mod merton_api;
pub mod models_api;
pub mod optimize_api;
pub mod options_api;
pub mod overnight_futures_api;
pub mod performance_ratios_api;
pub mod poisson_rng_api;
pub mod prices_api;
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
pub mod sequence_statistics_api;
pub mod settings_api;
pub mod smile_api;
pub mod statistics_api;
pub mod stripper_api;
pub mod stripper_completion_api;
pub mod time_api;
pub mod tree_swaption_api;
pub mod version_api;
pub mod vol_api;
#[cfg(test)]
mod vol_tests;
pub mod volatility_estimators_api;
pub mod volatility_ohlc_api;
pub mod volatility_overnight_api;
pub mod volcube_api;
pub mod volgrid_api;

#[cfg(test)]
mod swaption_facades_tests;

#[cfg(test)]
mod cap_calibration_tests;

pub mod variance_swap_api;
