//! Pseudo-random spot-start annualized variance estimation.

use crate::PyQlError;
use crate::market::PyBlackScholesProcess;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::MCVarianceSwapEngine;
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Estimate integrated log-price diffusion variance, not squared returns.
///
/// Exactly one grid selector and one stopping selector are required. Tolerance
/// is an absolute annualized-variance standard-error target, not a cash target.
/// Sampling error excludes grid/integration bias. Nonzero seeds reproduce each
/// recalculation; zero selects the existing randomized convention. No advanced
/// Monte Carlo features or historical observations are supported.
#[gen_stub_pyclass]
#[pyclass(
    name = "MCVarianceSwapEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyMCVarianceSwapEngine {
    inner: SharedMut<MCVarianceSwapEngine>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyMCVarianceSwapEngine {
    /// Retain a Black-Scholes process with bounded sampling and grid work.
    ///
    /// Fixed samples require at least two. Tolerance sampling starts with 1023
    /// observations and defaults to a 50000-sample maximum. Failure to reach
    /// tolerance within the permitted budget raises ItofinError.
    #[new]
    #[pyo3(signature = (process, *, steps=None, steps_per_year=None, samples=None, tolerance=None, max_samples=None, seed=0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        process: &PyBlackScholesProcess,
        steps: Option<usize>,
        steps_per_year: Option<usize>,
        samples: Option<usize>,
        tolerance: Option<f64>,
        max_samples: Option<usize>,
        seed: u64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared_mut(
                MCVarianceSwapEngine::new(
                    process.inner(),
                    steps,
                    steps_per_year,
                    samples,
                    tolerance,
                    max_samples,
                    seed,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }
}

impl PyMCVarianceSwapEngine {
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
