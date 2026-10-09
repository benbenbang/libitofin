//! Ordered NAV maximum drawdown snapshots backed by the shared Rust utility.

use crate::PyQlError;
use libitofin::math::statistics::{self, DrawdownResult};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Immutable fractional maximum loss and zero-based ordered NAV indices.
#[gen_stub_pyclass]
#[pyclass(name = "DrawdownResult", frozen, module = "itofin.statistics")]
pub(crate) struct PyDrawdownResult {
    inner: DrawdownResult,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyDrawdownResult {
    /// Nonnegative fractional loss, not a signed return or percentage.
    #[getter]
    fn drawdown(&self) -> f64 {
        self.inner.drawdown
    }

    /// Earliest equal running peak before the winning trough.
    #[getter]
    fn peak_index(&self) -> usize {
        self.inner.peak_index
    }

    /// First trough attaining the greatest computed fractional loss.
    #[getter]
    fn trough_index(&self) -> usize {
        self.inner.trough_index
    }
}

/// Maximum drawdown of ordered finite strictly positive equity/NAV values.
///
/// Empty input errors. One NAV or no decline returns zero with indices (0, 0).
/// Equal peaks retain their earliest index; equal computed losses retain the
/// first trough. Extreme positive ratios may round the fractional loss to one.
/// This is not meaningful for unordered return samples. Results own no handles.
#[gen_stub_pyfunction(module = "itofin.statistics")]
#[pyfunction]
pub(crate) fn maximum_drawdown(values: Vec<f64>) -> PyResult<PyDrawdownResult> {
    Ok(PyDrawdownResult {
        inner: statistics::maximum_drawdown(&values).map_err(PyQlError::from)?,
    })
}
