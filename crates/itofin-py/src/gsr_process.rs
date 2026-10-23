//! Standalone constant-parameter GSR process bindings.
use crate::PyQlError;
use libitofin::processes::forwardmeasureprocess::ForwardMeasureProcess1D;
use libitofin::processes::gsrprocess::GsrProcess;
use libitofin::stochasticprocess::StochasticProcess1D;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Constant-volatility, signed constant-reversion Gaussian forward-measure process.
///
/// This is not a fitted or calibrated GSR model. Times are process years within
/// [0,T]; date conversion and varying parameter grids are unsupported.
#[gen_stub_pyclass]
#[pyclass(name = "GsrProcess", unsendable, module = "itofin.processes")]
pub struct PyGsrProcess {
    inner: GsrProcess,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGsrProcess {
    /// Copy a validated constant grid and finite nonnegative forward horizon.
    ///
    /// Volatility count is time count plus one. Reversions contain one value or
    /// match volatility count. All repeated values must be identical. Grid times
    /// are finite, nonnegative and strictly increasing. Zero horizon is valid.
    #[new]
    fn new(
        times: Vec<f64>,
        vols: Vec<f64>,
        reversions: Vec<f64>,
        t_measure: f64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: GsrProcess::new(times, vols, reversions, t_measure).map_err(PyQlError::from)?,
        })
    }
    /// Return the current forward-measure horizon.
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
    /// Return the zero initial state.
    fn x0(&self) -> PyResult<f64> {
        Ok(self.inner.x0().map_err(PyQlError::from)?)
    }
    /// Return constant volatility at a valid process time.
    fn sigma(&self, t: f64) -> PyResult<f64> {
        Ok(self.inner.sigma(t).map_err(PyQlError::from)?)
    }
    /// Return signed reversion at a valid process time.
    fn reversion(&self, t: f64) -> PyResult<f64> {
        Ok(self.inner.reversion(t).map_err(PyQlError::from)?)
    }
    /// Return the auxiliary variance y(t).
    fn y(&self, t: f64) -> PyResult<f64> {
        Ok(self.inner.y(t).map_err(PyQlError::from)?)
    }
    /// Return G(t,end) for ordered endpoints inside the horizon.
    fn g(&self, t: f64, end: f64) -> PyResult<f64> {
        Ok(self.inner.g(t, end).map_err(PyQlError::from)?)
    }
    /// Return the forward-measure drift.
    fn drift(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.drift(t, x).map_err(PyQlError::from)?)
    }
    /// Return instantaneous diffusion.
    fn diffusion(&self, t: f64, x: f64) -> PyResult<f64> {
        Ok(self.inner.diffusion(t, x).map_err(PyQlError::from)?)
    }
    /// Return the exact conditional mean.
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
    /// Return the exact conditional variance.
    fn variance(&self, t: f64, x: f64, dt: f64) -> PyResult<f64> {
        Ok(self.inner.variance(t, x, dt).map_err(PyQlError::from)?)
    }
    /// Evolve using one finite standard Gaussian draw.
    fn evolve(&self, t: f64, x: f64, dt: f64, dw: f64) -> PyResult<f64> {
        Ok(self.inner.evolve(t, x, dt, dw).map_err(PyQlError::from)?)
    }
}
