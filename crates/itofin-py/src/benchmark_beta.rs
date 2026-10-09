//! Stateless Python benchmark beta over aligned returns.

use crate::PyQlError;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Weighted covariance / benchmark variance for 2-100000 aligned return pairs.
///
/// No annualization, data feed or risk-free adjustment. Weights are optional,
/// finite and nonnegative with finite positive total. Both moments apply the
/// same observation-count correction, counting zero-weight rows. All samples
/// must be finite; zero benchmark variance or nonfinite moments/beta raise
/// ItofinError. Caller owns date/frequency alignment. Inputs are unchanged;
/// no native handle or context needs closing.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
#[pyo3(signature = (asset_returns, benchmark_returns, *, weights = None))]
pub(crate) fn benchmark_beta(
    asset_returns: Vec<f64>,
    benchmark_returns: Vec<f64>,
    weights: Option<Vec<f64>>,
) -> PyResult<f64> {
    libitofin::math::statistics::benchmark_beta(
        &asset_returns,
        &benchmark_returns,
        weights.as_deref(),
    )
    .map_err(PyQlError::from)
    .map_err(Into::into)
}
