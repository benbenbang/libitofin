//! Scalar geometric Brownian motion using the native additive Euler scheme.

use crate::PyQlError;
use libitofin::processes::GeometricBrownianMotionProcess;
use libitofin::stochasticprocess::StochasticProcess1D;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Constant-coefficient `dX = mu * X * dt + volatility * X * dW` process.
///
/// Finite signed states are supported. Discrete transitions use additive Euler,
/// not the exact lognormal scheme of `itofin.simulate_gbm`. A sufficiently large
/// draw can cross zero. Standard deviation retains the sign of the state,
/// matching native diffusion-based Euler transitions.
#[gen_stub_pyclass]
#[pyclass(
    name = "GeometricBrownianMotionProcess",
    unsendable,
    module = "itofin.processes"
)]
pub struct PyGeometricBrownianMotionProcess {
    inner: GeometricBrownianMotionProcess,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGeometricBrownianMotionProcess {
    /// Copy finite initial state and drift, and finite nonnegative volatility.
    #[new]
    fn new(initial: f64, mu: f64, volatility: f64) -> PyResult<Self> {
        Ok(Self {
            inner: GeometricBrownianMotionProcess::new(initial, mu, volatility)
                .map_err(PyQlError::from)?,
        })
    }

    /// Return the copied initial state.
    fn x0(&self) -> PyResult<f64> {
        Ok(self.inner.x0().map_err(PyQlError::from)?)
    }

    /// Return the constant proportional drift coefficient.
    fn mu(&self) -> f64 {
        self.inner.mu()
    }

    /// Return the constant nonnegative volatility coefficient.
    fn volatility(&self) -> f64 {
        self.inner.volatility()
    }

    /// Return `mu * x` for a finite state and nonnegative finite time.
    fn drift(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.drift(t, x).map_err(PyQlError::from)?)
    }

    /// Return signed diffusion `volatility * x`.
    fn diffusion(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.diffusion(t, x).map_err(PyQlError::from)?)
    }

    /// Return Euler expectation `x + mu * x * dt`, not the exact GBM mean.
    fn expectation(&self, t0: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.expectation(t0, x, dt).map_err(PyQlError::from)?)
    }

    /// Return Euler variance `volatility**2 * x**2 * dt`.
    fn variance(&self, t0: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.variance(t0, x, dt).map_err(PyQlError::from)?)
    }

    /// Return signed Euler deviation `volatility * x * sqrt(dt)`.
    fn std_deviation(&self, t0: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .std_deviation(t0, x, dt)
            .map_err(PyQlError::from)?)
    }

    /// Return Euler expectation plus signed deviation times a standard draw.
    fn evolve(&self, t0: f64, x: f64, dt: f64, dw: f64) -> PyResult<f64> {
        Ok(self.inner.evolve(t0, x, dt, dw).map_err(PyQlError::from)?)
    }
}
