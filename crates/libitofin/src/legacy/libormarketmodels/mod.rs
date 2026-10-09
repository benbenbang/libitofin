//! Instantaneous volatility, correlation and covariance for forward-rate rows.
//!
//! Inputs and arithmetic are checked, unlike the unchecked fork queries. Models
//! are immutable and preserve input row order; no mutable calibration arguments
//! or stochastic-process integration are provided.

pub mod lmexpcorrmodel;
pub mod lmlinexpvolmodel;

pub use lmexpcorrmodel::LmExponentialCorrelationModel;
pub use lmlinexpvolmodel::LmLinearExponentialVolatilityModel;
