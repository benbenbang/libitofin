//! Python Williams percent R facade over the shared chart core.

use crate::{PyQlError, chart::PyChartSeries};
use libitofin::math::chart;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::gen_stub_pyfunction;

/// Inclusive trailing Williams percent R in [-100, 0]; flat windows return -50.
/// Default period is 14. First-valid is period-1 capped at length, with zero
/// warmup placeholders. Finite differences are required even during warmup.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
#[pyo3(signature = (high, low, close, period = 14))]
pub(crate) fn williams_r(
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
) -> PyResult<PyChartSeries> {
    chart::williams_r(&high, &low, &close, period)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}
