//! Python facade for overnight OHLC volatility estimators.

use crate::chart::PyChartSeries;
use crate::{ItofinError, PyQlError};
use libitofin::math::volatility::{self, OhlcOvernightEstimates};
use libitofin::prices::IntervalPrice;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Annualized overnight estimates aligned with the input OHLC bars.
#[gen_stub_pyclass]
#[pyclass(name = "OhlcOvernightEstimates", frozen, module = "itofin.chart")]
pub(crate) struct PyOhlcOvernightEstimates {
    garman_klass_sigma1: PyChartSeries,
    garman_klass_sigma3: PyChartSeries,
    garman_klass_sigma6: PyChartSeries,
}

impl From<OhlcOvernightEstimates> for PyOhlcOvernightEstimates {
    fn from(estimates: OhlcOvernightEstimates) -> Self {
        Self {
            garman_klass_sigma1: PyChartSeries::from_core(estimates.garman_klass_sigma1),
            garman_klass_sigma3: PyChartSeries::from_core(estimates.garman_klass_sigma3),
            garman_klass_sigma6: PyChartSeries::from_core(estimates.garman_klass_sigma6),
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyOhlcOvernightEstimates {
    /// Garman-Klass Sigma1 estimate using the preceding close.
    #[getter]
    fn garman_klass_sigma1(&self) -> PyChartSeries {
        self.garman_klass_sigma1.clone()
    }

    /// Garman-Klass Sigma3 estimate using the preceding close.
    #[getter]
    fn garman_klass_sigma3(&self) -> PyChartSeries {
        self.garman_klass_sigma3.clone()
    }

    /// Garman-Klass Sigma6 estimate using the preceding close.
    #[getter]
    fn garman_klass_sigma6(&self) -> PyChartSeries {
        self.garman_klass_sigma6.clone()
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

/// Estimate three annualized overnight series using each bar's year fraction.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn ohlc_overnight_volatility(
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    year_fractions: Vec<f64>,
    overnight_fraction: f64,
) -> PyResult<PyOhlcOvernightEstimates> {
    let prices = interval_prices(&open, &high, &low, &close)?;
    volatility::ohlc_overnight_volatility(&prices, &year_fractions, overnight_fraction)
        .map(PyOhlcOvernightEstimates::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Estimate three annualized overnight series with one year fraction per interval.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn ohlc_overnight_volatility_constant_fraction(
    open: Vec<f64>,
    high: Vec<f64>,
    low: Vec<f64>,
    close: Vec<f64>,
    year_fraction: f64,
    overnight_fraction: f64,
) -> PyResult<PyOhlcOvernightEstimates> {
    let prices = interval_prices(&open, &high, &low, &close)?;
    volatility::ohlc_overnight_volatility_constant_fraction(
        &prices,
        year_fraction,
        overnight_fraction,
    )
    .map(PyOhlcOvernightEstimates::from)
    .map_err(PyQlError::from)
    .map_err(Into::into)
}
