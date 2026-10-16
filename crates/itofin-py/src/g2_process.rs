//! Immutable two-dimensional Gaussian bare-factor transitions.
use crate::{ItofinError, PyQlError};
use libitofin::math::{array::Array, matrix::Matrix};
use libitofin::processes::G2Process;
use libitofin::stochasticprocess::StochasticProcess;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

fn state(values: Vec<f64>) -> PyResult<Array> {
    if values.len() != 2 || values.iter().any(|v| !v.is_finite()) {
        return Err(ItofinError::new_err(
            "G2 input must contain exactly two finite values",
        ));
    }
    Ok(Array::from(values))
}

fn rows(m: Matrix) -> Vec<Vec<f64>> {
    vec![vec![m[(0, 0)], m[(0, 1)]], vec![m[(1, 0)], m[(1, 1)]]]
}

/// Two correlated zero-level Gaussian factors, initially x=y=0.
///
/// Signed reversions and zero volatilities are supported. This is not a fitted
/// short-rate model, curve, tree or pricing engine. Matrices are owned row lists.
/// Diffusion uses rho*eta, correcting QuantLib's unequal-volatility defect.
#[gen_stub_pyclass]
#[pyclass(name = "G2Process", unsendable, module = "itofin.processes")]
pub struct PyG2Process {
    inner: G2Process,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyG2Process {
    /// Copy finite parameters; require nonnegative sigma/eta and rho in [-1,1].
    #[new]
    fn new(a: f64, sigma: f64, b: f64, eta: f64, rho: f64) -> PyResult<Self> {
        Ok(Self {
            inner: G2Process::new(a, sigma, b, eta, rho).map_err(PyQlError::from)?,
        })
    }
    /// Return the two state dimensions.
    fn size(&self) -> usize {
        self.inner.size()
    }
    /// Return the two independent Gaussian factors.
    fn factors(&self) -> usize {
        self.inner.factors()
    }
    /// Return initial x, always zero.
    fn x0(&self) -> f64 {
        self.inner.x0()
    }
    /// Return initial y, always zero.
    fn y0(&self) -> f64 {
        self.inner.y0()
    }
    /// Return signed x reversion.
    fn a(&self) -> f64 {
        self.inner.a()
    }
    /// Return x volatility.
    fn sigma(&self) -> f64 {
        self.inner.sigma()
    }
    /// Return signed y reversion.
    fn b(&self) -> f64 {
        self.inner.b()
    }
    /// Return y volatility.
    fn eta(&self) -> f64 {
        self.inner.eta()
    }
    /// Return instantaneous correlation.
    fn rho(&self) -> f64 {
        self.inner.rho()
    }
    /// Return a fresh [0,0] state.
    fn initial_values(&self) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .initial_values()
            .map_err(PyQlError::from)?
            .to_vec())
    }
    /// Return the native drift at finite nonnegative time and two finite states.
    fn drift(&self, t: f64, x: Vec<f64>) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .drift(t, &state(x)?)
            .map_err(PyQlError::from)?
            .to_vec())
    }
    /// Return the instantaneous two-by-two diffusion, one row per component.
    fn diffusion(&self, t: f64, x: Vec<f64>) -> PyResult<Vec<Vec<f64>>> {
        Ok(rows(
            self.inner
                .diffusion(t, &state(x)?)
                .map_err(PyQlError::from)?,
        ))
    }
    /// Return exact OU conditional means for finite nonnegative t and dt.
    fn expectation(&self, t: f64, x: Vec<f64>, dt: f64) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .expectation(t, &state(x)?, dt)
            .map_err(PyQlError::from)?
            .to_vec())
    }
    /// Return the exact transition root, not entrywise standard deviations.
    fn std_deviation(&self, t: f64, x: Vec<f64>, dt: f64) -> PyResult<Vec<Vec<f64>>> {
        Ok(rows(
            self.inner
                .std_deviation(t, &state(x)?, dt)
                .map_err(PyQlError::from)?,
        ))
    }
    /// Return exact conditional covariance, one row per state component.
    fn covariance(&self, t: f64, x: Vec<f64>, dt: f64) -> PyResult<Vec<Vec<f64>>> {
        Ok(rows(
            self.inner
                .covariance(t, &state(x)?, dt)
                .map_err(PyQlError::from)?,
        ))
    }
    /// Add two finite component changes; reject nonfinite sums.
    fn apply(&self, x: Vec<f64>, change: Vec<f64>) -> PyResult<Vec<f64>> {
        let result = self.inner.apply(&state(x)?, &state(change)?);
        state(result.to_vec()).map(|v| v.to_vec())
    }
    /// Evolve exactly using two independent standard Gaussian draws.
    fn evolve(&self, t: f64, x: Vec<f64>, dt: f64, draws: Vec<f64>) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .evolve(t, &state(x)?, dt, &state(draws)?)
            .map_err(PyQlError::from)?
            .to_vec())
    }
}
