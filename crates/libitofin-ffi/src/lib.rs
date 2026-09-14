//! C ABI adapters. The Rust core remains independent of this crate.
//!
//! # C caller contract
//! Every non-null pointer must be aligned, live, and valid for its stated size.
//! Output buffers must not overlap inputs, contexts, or other outputs. A context
//! and all its handles belong to the thread that created it. Calls on a context
//! must be serialized, including destruction. Never reuse a destroyed context.
//! Go enforces these rules through a session worker locked to one OS thread.
#![deny(unsafe_op_in_unsafe_fn)]

pub mod boundary;
pub mod simulation_api;
pub mod simulation_kernel;
pub use boundary::{Context, ItofinError};
pub mod time_api;
pub mod settings_api;
pub mod results_api;
pub mod market_api;
pub mod curves_api;
pub mod indexes_api;
pub mod vol_api;
pub mod smile_api;
pub mod ratevol_api;
pub mod options_api;
pub mod models_api;
pub mod mc_api;
pub mod credit_api;
pub mod credit_instruments_api;
pub mod helpers_api;
pub mod volgrid_api;
pub mod rates_api;
pub mod rates_fra;
pub mod stripper_api;
