//! itofin — a Rust port of QuantLib's quantitative finance core.
//!
//! The core library (`libitofin`). Language bindings (Python via PyO3, a C ABI
//! via cbindgen) live in sibling crates and depend on this one.

pub mod errors;
pub mod handle;
pub mod math;
pub mod patterns;
pub mod settings;
pub mod shared;
pub mod types;
pub mod utilities;
