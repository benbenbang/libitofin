//! Monte Carlo building blocks.
//!
//! Port of `ql/methods/montecarlo/`. This foundation ticket lands the weighted
//! [`Sample`]; the path generators, path pricers, and Monte Carlo model stack
//! stack on top in later tickets.

mod path;
mod sample;

pub use path::Path;
pub use sample::Sample;
