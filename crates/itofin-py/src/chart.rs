//! Python chart indicators backed by the shared Rust calculations.

use crate::PyQlError;
use libitofin::math::chart::{self, ChartSeries, VolumeBars};
use numpy::PyArray1;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Values aligned with input bars. Entries before `first_valid` are warmup slots.
#[gen_stub_pyclass]
#[pyclass(
    name = "ChartSeries",
    frozen,
    skip_from_py_object,
    module = "itofin.chart"
)]
#[derive(Clone)]
pub(crate) struct PyChartSeries {
    inner: ChartSeries,
}

impl PyChartSeries {
    pub(crate) fn from_core(inner: ChartSeries) -> Self {
        Self { inner }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyChartSeries {
    /// A float64 copy of all bars; warmup slots contain zero placeholders.
    #[getter]
    fn values<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_slice(py, &self.inner.values)
    }

    /// Index of the first usable result, or the series length if none is usable.
    #[getter]
    fn first_valid(&self) -> usize {
        self.inner.first_valid
    }

    /// Copy values with `None` in warmup slots for JSON or chart libraries.
    fn to_list(&self) -> Vec<Option<f64>> {
        self.inner
            .values
            .iter()
            .enumerate()
            .map(|(index, value)| (index >= self.inner.first_valid).then_some(*value))
            .collect()
    }
}

/// Raw volume and the per-bar direction relative to the open.
#[gen_stub_pyclass]
#[pyclass(name = "VolumeBars", frozen, module = "itofin.chart")]
pub(crate) struct PyVolumeBars {
    volume: PyChartSeries,
    direction: Vec<i8>,
}

impl From<VolumeBars> for PyVolumeBars {
    fn from(bars: VolumeBars) -> Self {
        Self {
            volume: PyChartSeries::from_core(bars.volume),
            direction: bars.direction,
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyVolumeBars {
    /// Raw nonnegative volume, aligned with input bars.
    #[getter]
    fn volume(&self) -> PyChartSeries {
        self.volume.clone()
    }

    /// Per-bar direction: -1 for down, 0 for flat, 1 for up.
    #[getter]
    fn direction<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<i8>> {
        PyArray1::from_slice(py, &self.direction)
    }
}

/// Simple moving average, seeded after `period` closing prices.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn sma(close: Vec<f64>, period: usize) -> PyResult<PyChartSeries> {
    chart::sma(&close, period)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Exponential moving average seeded by the first `period`-bar SMA.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn ema(close: Vec<f64>, period: usize) -> PyResult<PyChartSeries> {
    chart::ema(&close, period)
        .map(PyChartSeries::from_core)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Validate OHLCV bars and return raw volume with close-versus-open direction.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn volume_bars(
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    volume: Vec<f64>,
) -> PyResult<PyVolumeBars> {
    chart::volume_bars(&open, &high, &low, &close, &volume)
        .map(PyVolumeBars::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warmup_converts_to_missing_without_changing_valid_zero() {
        let series = PyChartSeries::from_core(ChartSeries {
            values: vec![0.0, 0.0, 2.0, 0.0],
            first_valid: 2,
        });
        assert_eq!(series.to_list(), vec![None, None, Some(2.0), Some(0.0)]);
    }

    #[test]
    fn invalid_period_maps_to_python_error() {
        assert!(sma(vec![1.0], 0).is_err());
        assert!(ema(vec![1.0], 0).is_err());
    }
}
