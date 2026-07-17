//! Concrete short-rate calibration helpers.
//!
//! Port of `ql/models/shortrate/calibrationhelpers/`. Each helper is a
//! [`BlackCalibrationHelper`](crate::models::calibrationhelper::BlackCalibrationHelper)
//! that compares a market price (implied by a quoted volatility) against the
//! price a short-rate model produces, so a calibration cost function can drive
//! the model onto the market.

pub mod swaptionhelper;

pub use swaptionhelper::SwaptionHelper;
