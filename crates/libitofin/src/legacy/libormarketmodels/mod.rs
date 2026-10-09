//! Instantaneous volatility, correlation and covariance for forward-rate rows.
//!
//! Inputs and arithmetic are checked, unlike the unchecked fork queries. Models
//! are immutable and preserve input row order; no mutable calibration arguments
//! or stochastic-process integration are provided.

pub mod lfmcovarproxy;
pub mod lmexpcorrmodel;
pub mod lmlinexpvolmodel;

pub use lfmcovarproxy::LfmCovarianceProxy;
pub use lmexpcorrmodel::LmExponentialCorrelationModel;
pub use lmlinexpvolmodel::LmLinearExponentialVolatilityModel;
