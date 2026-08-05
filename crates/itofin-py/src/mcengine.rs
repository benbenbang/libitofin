//! Facades for the Monte Carlo European pricing engines:
//! [`PyMCEuropeanEngine`] and [`PyMCEuropeanHestonEngine`].
//!
//! The core engines are generic over their RNG policy, which does not cross FFI
//! (D7), so the facades pin `PseudoRandom` - the policy that carries an error
//! estimate, and the one the core oracles price against.
//!
//! Deferred (visible): the low-discrepancy `LowDiscrepancy` policy behind
//! `testQmcEngines` is not exposed; it lands with the Sobol RNG policy (#454).

use crate::PyQlError;
use crate::heston::PyHestonProcess;
use crate::market::PyBlackScholesProcess;
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::{
    MCEuropeanEngine, MCEuropeanHestonEngine, MakeMcEuropeanEngine, MakeMcEuropeanHestonEngine,
};
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;

/// Python `MCEuropeanEngine`: the Monte Carlo engine for European payoffs
/// (`pricingengines::vanilla::MCEuropeanEngine`), built through the core's
/// `MakeMcEuropeanEngine` factory.
///
/// Every argument past `process` is optional and left unset when omitted, so
/// the core's own validation reports the illegal combinations as
/// `ItofinError`: exactly one of `steps` / `steps_per_year` must be given, and
/// `samples` and `absolute_tolerance` are mutually exclusive.
///
/// `antithetic` is accepted but not yet supported by the core engine, so
/// passing `True` raises `ItofinError` at construction rather than silently
/// pricing without the variance reduction (#772).
///
/// Pricing is seeded and deterministic: the same `seed` reproduces the NPV
/// bitwise, and the resulting standard error is read back through
/// `VanillaOption.error_estimate()`.
#[pyclass(name = "MCEuropeanEngine", unsendable)]
pub struct PyMCEuropeanEngine {
    inner: SharedMut<MCEuropeanEngine<PseudoRandom>>,
}

#[pymethods]
impl PyMCEuropeanEngine {
    /// An engine over `process`, configured through the core factory.
    #[new]
    #[pyo3(signature = (
        process,
        steps = None,
        steps_per_year = None,
        samples = None,
        absolute_tolerance = None,
        max_samples = None,
        seed = None,
        antithetic = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        process: &PyBlackScholesProcess,
        steps: Option<usize>,
        steps_per_year: Option<usize>,
        samples: Option<usize>,
        absolute_tolerance: Option<f64>,
        max_samples: Option<usize>,
        seed: Option<u32>,
        antithetic: Option<bool>,
    ) -> PyResult<Self> {
        let mut maker = MakeMcEuropeanEngine::<PseudoRandom>::new(process.inner());
        if let Some(steps) = steps {
            maker = maker.with_steps(steps);
        }
        if let Some(steps_per_year) = steps_per_year {
            maker = maker.with_steps_per_year(steps_per_year);
        }
        if let Some(samples) = samples {
            maker = maker.with_samples(samples);
        }
        if let Some(tolerance) = absolute_tolerance {
            maker = maker.with_absolute_tolerance(tolerance);
        }
        if let Some(max_samples) = max_samples {
            maker = maker.with_max_samples(max_samples);
        }
        if let Some(seed) = seed {
            maker = maker.with_seed(seed);
        }
        if let Some(antithetic) = antithetic {
            maker = maker.with_antithetic_variate(antithetic);
        }
        let engine = maker.build().map_err(PyQlError::from)?;
        Ok(PyMCEuropeanEngine {
            inner: shared_mut(engine),
        })
    }
}

impl PyMCEuropeanEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

/// Python `MCEuropeanHestonEngine`: the Monte Carlo engine pricing European
/// payoffs on a Heston process (`pricingengines::vanilla::
/// MCEuropeanHestonEngine`), built through the core's
/// `MakeMcEuropeanHestonEngine` factory.
///
/// Same optional-argument contract as [`PyMCEuropeanEngine`]: everything past
/// `process` is left unset when omitted, so the core reports the illegal
/// combinations as `ItofinError`. Unlike that engine, `antithetic` is supported
/// here - the multi-factor path generator wires the antithetic negation - and
/// the core cached oracle prices with it on.
///
/// Pricing is seeded and deterministic: the same `seed` reproduces the NPV
/// bitwise, and the resulting standard error is read back through
/// `VanillaOption.error_estimate()`.
#[pyclass(name = "MCEuropeanHestonEngine", unsendable)]
pub struct PyMCEuropeanHestonEngine {
    inner: SharedMut<MCEuropeanHestonEngine<PseudoRandom>>,
}

#[pymethods]
impl PyMCEuropeanHestonEngine {
    /// An engine over `process`, configured through the core factory.
    #[new]
    #[pyo3(signature = (
        process,
        steps = None,
        steps_per_year = None,
        samples = None,
        absolute_tolerance = None,
        max_samples = None,
        seed = None,
        antithetic = None,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        process: &PyHestonProcess,
        steps: Option<usize>,
        steps_per_year: Option<usize>,
        samples: Option<usize>,
        absolute_tolerance: Option<f64>,
        max_samples: Option<usize>,
        seed: Option<u32>,
        antithetic: Option<bool>,
    ) -> PyResult<Self> {
        let mut maker = MakeMcEuropeanHestonEngine::<PseudoRandom>::new(process.inner());
        if let Some(steps) = steps {
            maker = maker.with_steps(steps);
        }
        if let Some(steps_per_year) = steps_per_year {
            maker = maker.with_steps_per_year(steps_per_year);
        }
        if let Some(samples) = samples {
            maker = maker.with_samples(samples);
        }
        if let Some(tolerance) = absolute_tolerance {
            maker = maker.with_absolute_tolerance(tolerance);
        }
        if let Some(max_samples) = max_samples {
            maker = maker.with_max_samples(max_samples);
        }
        if let Some(seed) = seed {
            maker = maker.with_seed(seed);
        }
        if let Some(antithetic) = antithetic {
            maker = maker.with_antithetic_variate(antithetic);
        }
        let engine = maker.build().map_err(PyQlError::from)?;
        Ok(PyMCEuropeanHestonEngine {
            inner: shared_mut(engine),
        })
    }
}

impl PyMCEuropeanHestonEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
