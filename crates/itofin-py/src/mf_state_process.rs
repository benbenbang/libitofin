//! Immutable standalone Markov-functional state process bindings.
use crate::PyQlError;
use libitofin::processes::MfStateProcess;
use libitofin::stochasticprocess::StochasticProcess1D;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Standalone Gaussian state process with immutable volatility buckets.
///
/// Not a fitted Markov-functional interest-rate model. Times are process years.
/// Diffusion returns raw volatility; variance integrates sigma(s)^2 exp(2 a s).
/// With no bucket times, variance uses unit volatility irrespective of vols[0].
#[gen_stub_pyclass]
#[pyclass(name = "MfStateProcess", unsendable, module = "itofin.processes")]
pub struct PyMfStateProcess {
    inner: MfStateProcess,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMfStateProcess {
    /// Copy signed reversion, increasing times and nonnegative raw volatilities.
    /// Volatility count must exceed time count by one; all inputs must be finite.
    #[new]
    fn new(reversion: f64, times: Vec<f64>, vols: Vec<f64>) -> PyResult<Self> {
        Ok(Self {
            inner: MfStateProcess::new(reversion, times.into(), vols.into())
                .map_err(PyQlError::from)?,
        })
    }
    /// Return the signed exponential reversion parameter.
    fn reversion(&self) -> f64 {
        self.inner.reversion()
    }
    /// Return a new copy of the right-continuous bucket boundaries.
    fn times(&self) -> Vec<f64> {
        self.inner.times().to_vec()
    }
    /// Return a new copy of the raw bucket volatilities.
    fn volatilities(&self) -> Vec<f64> {
        self.inner.volatilities().to_vec()
    }
    /// Return zero, the initial process state.
    fn x0(&self) -> PyResult<f64> {
        Ok(self.inner.x0().map_err(PyQlError::from)?)
    }
    /// Return zero after validating process time and state.
    fn drift(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.drift(t, x).map_err(PyQlError::from)?)
    }
    /// Return raw sigma(t), not sigma(t) exp(reversion*t).
    fn diffusion(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.diffusion(t, x).map_err(PyQlError::from)?)
    }
    /// Return the unchanged state after validating the interval.
    fn expectation(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.expectation(t, x, dt).map_err(PyQlError::from)?)
    }
    /// Return the exact conditional standard deviation.
    fn std_deviation(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .std_deviation(t, x, dt)
            .map_err(PyQlError::from)?)
    }
    /// Integrate sigma(s)^2 exp(2*reversion*s) over the interval.
    fn variance(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.variance(t, x, dt).map_err(PyQlError::from)?)
    }
    /// Evolve using one finite standard Gaussian draw.
    fn evolve(&self, t: f64, x: f64, dt: f64, dw: f64) -> PyResult<f64> {
        Ok(self.inner.evolve(t, x, dt, dw).map_err(PyQlError::from)?)
    }
}
