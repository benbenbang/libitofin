//! Python facade for fixed-parameter GARCH(1,1) filtering and forecasting.

use crate::PyQlError;
use crate::chart::PyChartSeries;
use libitofin::math::garch::{Garch11, Garch11Filter, Garch11Fit};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyfunction, gen_stub_pymethods};

/// Conditional volatility aligned with returns and one-step variance forecast.
#[gen_stub_pyclass]
#[pyclass(name = "Garch11Result", frozen, module = "itofin.chart")]
pub(crate) struct PyGarch11Result {
    conditional_volatility: PyChartSeries,
    next_variance: f64,
}

impl From<Garch11Filter> for PyGarch11Result {
    fn from(result: Garch11Filter) -> Self {
        Self {
            conditional_volatility: PyChartSeries::from_core(result.conditional_volatility),
            next_variance: result.next_variance,
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGarch11Result {
    /// Per-return conditional standard deviation in the return input's units.
    #[getter]
    fn conditional_volatility(&self) -> PyChartSeries {
        self.conditional_volatility.clone()
    }

    /// Conditional variance forecast following the final return.
    #[getter]
    fn next_variance(&self) -> f64 {
        self.next_variance
    }
}

/// Fitted stationary GARCH(1,1) parameters and one-step variance forecast.
#[gen_stub_pyclass]
#[pyclass(name = "Garch11FitResult", frozen, module = "itofin.chart")]
pub(crate) struct PyGarch11FitResult {
    alpha: f64,
    beta: f64,
    omega: f64,
    log_likelihood: f64,
    next_variance: f64,
}

impl From<Garch11Fit> for PyGarch11FitResult {
    fn from(result: Garch11Fit) -> Self {
        Self {
            alpha: result.alpha,
            beta: result.beta,
            omega: result.omega,
            log_likelihood: result.log_likelihood,
            next_variance: result.next_variance,
        }
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGarch11FitResult {
    /// Fitted response to the preceding squared return.
    #[getter]
    fn alpha(&self) -> f64 {
        self.alpha
    }

    /// Fitted persistence of conditional variance.
    #[getter]
    fn beta(&self) -> f64 {
        self.beta
    }

    /// Fitted variance intercept.
    #[getter]
    fn omega(&self) -> f64 {
        self.omega
    }

    /// Gaussian log likelihood per return, excluding the constant term.
    #[getter]
    fn log_likelihood(&self) -> f64 {
        self.log_likelihood
    }

    /// Conditional variance forecast following the final return.
    #[getter]
    fn next_variance(&self) -> f64 {
        self.next_variance
    }
}

/// Filter returns with fixed GARCH(1,1) parameters and long-run variance.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn garch11_filter(
    returns: Vec<f64>,
    alpha: f64,
    beta: f64,
    long_run_variance: f64,
) -> PyResult<PyGarch11Result> {
    Garch11::new(alpha, beta, long_run_variance)
        .and_then(|model| model.filter(&returns))
        .map(PyGarch11Result::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Forecast the next variance from the last return and current variance.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn garch11_forecast(
    last_return: f64,
    current_variance: f64,
    alpha: f64,
    beta: f64,
    long_run_variance: f64,
) -> PyResult<f64> {
    Garch11::new(alpha, beta, long_run_variance)
        .and_then(|model| model.forecast(last_return, current_variance))
        .map_err(PyQlError::from)
        .map_err(Into::into)
}

/// Fit stationary GARCH(1,1) parameters to observed returns.
#[gen_stub_pyfunction(module = "itofin.chart")]
#[pyfunction]
pub(crate) fn garch11_fit(returns: Vec<f64>) -> PyResult<PyGarch11FitResult> {
    Garch11::fit(&returns)
        .map(PyGarch11FitResult::from)
        .map_err(PyQlError::from)
        .map_err(Into::into)
}
