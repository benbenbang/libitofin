//! Python facade for unweighted explicit-frequency performance ratios.

use crate::PyQlError;
use libitofin::math::statistics;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// All-observation RMS shortfall below the scalar per-period target, without annualization.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
pub(crate) fn target_downside_deviation(returns: Vec<f64>, target: f64) -> PyResult<f64> {
    statistics::target_downside_deviation(&returns, target).map_err(|e| PyQlError::from(e).into())
}

/// Arithmetic excess mean/sample std (N-1), scaled by sqrt(periods_per_year).
/// Risk-free return is per period. Frequency is required; zero dispersion is an error.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (returns, risk_free_return, *, periods_per_year))]
pub(crate) fn sharpe_ratio(
    returns: Vec<f64>,
    risk_free_return: f64,
    periods_per_year: f64,
) -> PyResult<f64> {
    statistics::sharpe_ratio(&returns, risk_free_return, periods_per_year)
        .map_err(|e| PyQlError::from(e).into())
}

/// Arithmetic mean minus per-period MAR/all-N downside, scaled by sqrt(periods_per_year).
/// Frequency is required; no downside is an error, not infinity.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (returns, minimum_acceptable_return, *, periods_per_year))]
pub(crate) fn sortino_ratio(
    returns: Vec<f64>,
    minimum_acceptable_return: f64,
    periods_per_year: f64,
) -> PyResult<f64> {
    statistics::sortino_ratio(&returns, minimum_acceptable_return, periods_per_year)
        .map_err(|e| PyQlError::from(e).into())
}
