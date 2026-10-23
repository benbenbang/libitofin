//! Hull-White forward-measure process retaining its supplied live curve.
use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use libitofin::processes::{ForwardMeasureProcess1D, HullWhiteForwardProcess};
use libitofin::stochasticprocess::StochasticProcess1D;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Standalone forward-measure short-rate process, not a calibrated pricing model.
///
/// The initial short rate is captured once. Fitting shifts and drift read the
/// retained live curve. Horizon starts at zero; times are process years, not dates.
#[gen_stub_pyclass]
#[pyclass(
    name = "HullWhiteForwardProcess",
    unsendable,
    module = "itofin.processes"
)]
pub struct PyHullWhiteForwardProcess {
    inner: HullWhiteForwardProcess,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyHullWhiteForwardProcess {
    /// Retain the exact curve handle, with finite nonnegative reversion and volatility.
    #[new]
    fn new(curve: &PyYieldTermStructure, a: f64, sigma: f64) -> PyResult<Self> {
        Ok(Self {
            inner: HullWhiteForwardProcess::new(curve.handle(), a, sigma)
                .map_err(PyQlError::from)?,
        })
    }
    /// Return the short rate captured at construction, unchanged by curve updates.
    fn x0(&self) -> PyResult<f64> {
        Ok(self.inner.x0().map_err(PyQlError::from)?)
    }
    /// Return constant nonnegative mean reversion.
    fn a(&self) -> f64 {
        self.inner.a()
    }
    /// Return constant nonnegative volatility.
    fn sigma(&self) -> f64 {
        self.inner.sigma()
    }
    /// Return the current finite nonnegative horizon.
    fn forward_measure_time(&self) -> f64 {
        self.inner.forward_measure_time()
    }
    /// Replace the horizon and notify observers. Invalid values preserve state.
    fn set_forward_measure_time(&mut self, horizon: f64) -> PyResult<()> {
        Ok(self
            .inner
            .set_forward_measure_time(horizon)
            .map_err(PyQlError::from)?)
    }
    /// Return the fitting shift using the retained live curve.
    fn alpha(&self, t: f64) -> PyResult<f64> {
        Ok(self.inner.alpha(t).map_err(PyQlError::from)?)
    }
    /// Return B(t,maturity) for absolute endpoints; maturity before t is valid.
    fn b(&self, t: f64, maturity: f64) -> PyResult<f64> {
        Ok(self.inner.b(t, maturity).map_err(PyQlError::from)?)
    }
    /// Return M_T(s,t,maturity) for s <= t and any nonnegative absolute maturity.
    fn m_t(&self, s: f64, t: f64, maturity: f64) -> PyResult<f64> {
        Ok(self.inner.m_t(s, t, maturity).map_err(PyQlError::from)?)
    }
    /// Return forward-measure drift using the live curve and current horizon.
    fn drift(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.drift(t, x).map_err(PyQlError::from)?)
    }
    /// Return instantaneous volatility at valid t and x.
    fn diffusion(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.diffusion(t, x).map_err(PyQlError::from)?)
    }
    /// Return the exact conditional mean using the retained live curve.
    fn expectation(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.expectation(t, x, dt).map_err(PyQlError::from)?)
    }
    /// Return conditional standard deviation.
    fn std_deviation(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .std_deviation(t, x, dt)
            .map_err(PyQlError::from)?)
    }
    /// Return variance, retaining the native Ornstein-Uhlenbeck small-speed branch.
    fn variance(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.variance(t, x, dt).map_err(PyQlError::from)?)
    }
    /// Evolve with one finite standard Gaussian draw.
    fn evolve(&self, t: f64, x: f64, dt: f64, dw: f64) -> PyResult<f64> {
        Ok(self.inner.evolve(t, x, dt, dw).map_err(PyQlError::from)?)
    }
}
