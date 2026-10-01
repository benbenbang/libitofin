//! Observable Merton jump-diffusion process and European pricing engine.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::time::PyDate;
use crate::vol::PyBlackVolTermStructure;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::JumpDiffusionEngine;
use libitofin::processes::Merton76Process;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Merton's lognormal-jump model retaining observable market and jump inputs.
///
/// This process supports European pricing. Generic drift, diffusion and path
/// generation are unavailable because a Gaussian step cannot represent jumps.
#[gen_stub_pyclass]
#[pyclass(name = "Merton76Process", unsendable, module = "itofin.processes")]
pub struct PyMerton76Process {
    inner: Shared<Merton76Process>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMerton76Process {
    /// Retain the diffusion market and the three live lognormal-jump quotes.
    ///
    /// Jump intensity and log-jump volatility must be finite and nonnegative;
    /// the log-jump mean must be finite. Invalid inputs raise ItofinError.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        spot: &PySimpleQuote,
        risk_free: &PyYieldTermStructure,
        dividend: &PyYieldTermStructure,
        volatility: &PyBlackVolTermStructure,
        jump_intensity: &PySimpleQuote,
        log_mean_jump: &PySimpleQuote,
        log_jump_volatility: &PySimpleQuote,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared(
                Merton76Process::new(
                    spot.handle(),
                    dividend.handle(),
                    risk_free.handle(),
                    volatility.handle(),
                    jump_intensity.handle(),
                    log_mean_jump.handle(),
                    log_jump_volatility.handle(),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Return the current spot held by the diffusion market.
    fn spot(&self) -> PyResult<f64> {
        Ok(self.inner.x0().map_err(PyQlError::from)?)
    }

    /// Return the current jump intensity.
    fn jump_intensity(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .jump_intensity()
            .current_link()
            .map_err(PyQlError::from)?
            .value()
            .map_err(PyQlError::from)?)
    }

    /// Return the current mean of the logarithmic jump size.
    fn log_mean_jump(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .log_mean_jump()
            .current_link()
            .map_err(PyQlError::from)?
            .value()
            .map_err(PyQlError::from)?)
    }

    /// Return the current standard deviation of the logarithmic jump size.
    fn log_jump_volatility(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .log_jump_volatility()
            .current_link()
            .map_err(PyQlError::from)?
            .value()
            .map_err(PyQlError::from)?)
    }

    /// Convert a date with the risk-free curve's reference date and day count.
    fn time(&self, date: &PyDate) -> PyResult<f64> {
        Ok(self.inner.time(&date.inner()).map_err(PyQlError::from)?)
    }
}

/// European Merton pricing by a convergent Poisson mixture of Black prices.
#[gen_stub_pyclass]
#[pyclass(
    name = "JumpDiffusionEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyJumpDiffusionEngine {
    inner: SharedMut<dyn PricingEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyJumpDiffusionEngine {
    /// Retain the process and configure relative series accuracy and term limit.
    ///
    /// Accuracy must be finite and positive, and the iteration limit must be
    /// between 1 and 100000. Failure to converge raises ItofinError when pricing.
    #[new]
    #[pyo3(signature = (process, relative_accuracy=1e-4, max_iterations=100))]
    fn new(
        process: &PyMerton76Process,
        relative_accuracy: f64,
        max_iterations: usize,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                JumpDiffusionEngine::new(
                    Shared::clone(&process.inner),
                    relative_accuracy,
                    max_iterations,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }
}

impl PyJumpDiffusionEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner)
    }
}
