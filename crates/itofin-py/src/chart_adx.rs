//! Wilder directional indicators backed by shared Rust calculations.

use crate::{PyQlError, chart::PyChartSeries};
use libitofin::math::chart::{self, Adx};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Aligned +DI, -DI, DX and ADX, each with its own warmup metadata.
#[gen_stub_pyclass]
#[pyclass(name = "Adx", frozen, module = "itofin.chart")]
pub(crate) struct PyAdx {
    plus_di: PyChartSeries,
    minus_di: PyChartSeries,
    dx: PyChartSeries,
    adx: PyChartSeries,
}

impl From<Adx> for PyAdx {
    fn from(result: Adx) -> Self {
        Self {
            plus_di: PyChartSeries::from_core(result.plus_di),
            minus_di: PyChartSeries::from_core(result.minus_di),
            dx: PyChartSeries::from_core(result.dx),
            adx: PyChartSeries::from_core(result.adx),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyAdx {
    /// Copy of positive directional indicator, valid from index period.
    #[getter]
    fn plus_di(&self) -> PyChartSeries {
        self.plus_di.clone()
    }
    /// Copy of negative directional indicator, valid from index period.
    #[getter]
    fn minus_di(&self) -> PyChartSeries {
        self.minus_di.clone()
    }
    /// Copy of directional index, valid from index period.
    #[getter]
    fn dx(&self) -> PyChartSeries {
        self.dx.clone()
    }
    /// Copy of Wilder-smoothed DX, valid from index 2*period-1.
    #[getter]
    fn adx(&self) -> PyChartSeries {
        self.adx.clone()
    }
}

/// Wilder +DI/-DI/DX/ADX, seeded from transitions excluding bar zero.
/// Strictly larger positive movements win; ties select neither. TR/DM seed
/// from bars1..=period; ADX seeds the first period valid DX values. Zero TR
/// sets DI to zero; zero DI sum sets DX to zero. Period one is supported.
/// Warmup zeroes are missing through each series' to_list(). Invalid bars,
/// zero periods and nonfinite differences fail even during warmup.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
#[pyo3(signature = (high, low, close, period=14))]
pub(crate) fn adx(
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    period: usize,
) -> PyResult<PyAdx> {
    Ok(chart::adx(&high, &low, &close, period)
        .map_err(PyQlError::from)?
        .into())
}
