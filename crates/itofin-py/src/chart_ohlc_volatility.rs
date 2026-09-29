//! Python facade for pointwise OHLC volatility estimators.

use crate::chart::PyChartSeries;
use crate::{ItofinError, PyQlError};
use libitofin::math::volatility::{self, OhlcPointEstimates};
use libitofin::prices::IntervalPrice;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Annualized volatility estimates aligned with the input OHLC bars.
#[gen_stub_pyclass]
#[pyclass(name = "OhlcPointEstimates", frozen, module = "itofin.chart")]
pub(crate) struct PyOhlcPointEstimates {
    simple_sigma: PyChartSeries,
    parkinson_sigma: PyChartSeries,
    garman_klass_sigma4: PyChartSeries,
    garman_klass_sigma5: PyChartSeries,
}

impl From<OhlcPointEstimates> for PyOhlcPointEstimates {
    fn from(estimates: OhlcPointEstimates) -> Self {
        Self {
            simple_sigma: PyChartSeries::from_core(estimates.simple_sigma),
            parkinson_sigma: PyChartSeries::from_core(estimates.parkinson_sigma),
            garman_klass_sigma4: PyChartSeries::from_core(estimates.garman_klass_sigma4),
            garman_klass_sigma5: PyChartSeries::from_core(estimates.garman_klass_sigma5),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyOhlcPointEstimates {
    /// Simple open-to-close estimate.
    #[getter]
    fn simple_sigma(&self) -> PyChartSeries {
        self.simple_sigma.clone()
    }

    /// Parkinson high-low estimate.
    #[getter]
    fn parkinson_sigma(&self) -> PyChartSeries {
        self.parkinson_sigma.clone()
    }

    /// Four-price Garman-Klass estimate.
    #[getter]
    fn garman_klass_sigma4(&self) -> PyChartSeries {
        self.garman_klass_sigma4.clone()
    }

    /// Garman-Klass Sigma5 estimate.
    #[getter]
    fn garman_klass_sigma5(&self) -> PyChartSeries {
        self.garman_klass_sigma5.clone()
    }
}

fn interval_prices(
    open: &[f64],
    high: &[f64],
    low: &[f64],
    close: &[f64],
) -> PyResult<Vec<IntervalPrice>> {
    if [high.len(), low.len(), close.len()]
        .iter()
        .any(|len| *len != open.len())
    {
        return Err(ItofinError::new_err(
            "OHLC slices must have the same length",
        ));
    }
    open.iter()
        .zip(high)
        .zip(low)
        .zip(close)
        .map(|(((&open, &high), &low), &close)| {
            IntervalPrice::new(open, high, low, close)
                .map_err(PyQlError::from)
                .map_err(Into::into)
        })
        .collect()
}

/// Estimate four annualized volatility series using each bar's year fraction.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn ohlc_point_volatility(
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    year_fractions: Vec<f64>,
) -> PyResult<PyOhlcPointEstimates> {
    let prices = interval_prices(&open, &high, &low, &close)?;
    volatility::ohlc_point_volatility(&prices, &year_fractions)
        .map(PyOhlcPointEstimates::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Estimate four annualized volatility series with one year fraction per bar.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn ohlc_point_volatility_constant_fraction(
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    year_fraction: f64,
) -> PyResult<PyOhlcPointEstimates> {
    let prices = interval_prices(&open, &high, &low, &close)?;
    volatility::ohlc_point_volatility_constant_fraction(&prices, year_fraction)
        .map(PyOhlcPointEstimates::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}
