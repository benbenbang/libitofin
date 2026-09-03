//! Facades for the Monte Carlo pricing engines: MCEuropeanEngine,
//! MCEuropeanHestonEngine and MCAmericanEngine.
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
    MCAmericanEngine, MCEuropeanEngine, MCEuropeanHestonEngine, MakeMcAmericanEngine,
    MakeMcEuropeanEngine, MakeMcEuropeanHestonEngine,
};
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;

/// The Monte Carlo engine for European payoffs, over the pseudo-random RNG
/// policy. The low-discrepancy policy is not exposed (#454).
///
/// Pricing is seeded and deterministic: the same seed reproduces the NPV
/// bitwise, and the standard error is read back through
/// VanillaOption.error_estimate().
#[pyclass(name = "MCEuropeanEngine", unsendable)]
pub struct PyMCEuropeanEngine {
    inner: SharedMut<MCEuropeanEngine<PseudoRandom>>,
}

#[pymethods]
impl PyMCEuropeanEngine {
    /// Build an engine over process, configured through the core factory.
    ///
    /// Every argument past process is left unset when omitted, so the core's
    /// own validation reports the illegal combinations.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The process paths are drawn from.
    ///     steps (int | None): The fixed number of time steps per path.
    ///     steps_per_year (int | None): The time steps per year, the
    ///         alternative to steps.
    ///     samples (int | None): The fixed number of paths to draw.
    ///     absolute_tolerance (float | None): The target standard error, the
    ///         alternative to samples.
    ///     max_samples (int | None): The cap on paths drawn when running to a
    ///         tolerance.
    ///     seed (int | None): The RNG seed; the same seed reproduces the NPV
    ///         bitwise.
    ///     antithetic (bool | None): The antithetic variate, supported since
    ///         #772 lifted the former construction-time rejection.
    ///
    /// Raises:
    ///     ItofinError: If neither or both of steps and steps_per_year are
    ///         given, or if both samples and absolute_tolerance are given.
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

/// The Monte Carlo engine for European payoffs on a Heston process, over the
/// pseudo-random RNG policy. The low-discrepancy policy is not exposed (#454).
///
/// Pricing is seeded and deterministic: the same seed reproduces the NPV
/// bitwise, and the standard error is read back through
/// VanillaOption.error_estimate().
#[pyclass(name = "MCEuropeanHestonEngine", unsendable)]
pub struct PyMCEuropeanHestonEngine {
    inner: SharedMut<MCEuropeanHestonEngine<PseudoRandom>>,
}

#[pymethods]
impl PyMCEuropeanHestonEngine {
    /// Build an engine over process, configured through the core factory.
    ///
    /// Every argument past process is left unset when omitted, so the core's
    /// own validation reports the illegal combinations.
    ///
    /// Args:
    ///     process (HestonProcess): The Heston process paths are drawn from.
    ///     steps (int | None): The fixed number of time steps per path.
    ///     steps_per_year (int | None): The time steps per year, the
    ///         alternative to steps.
    ///     samples (int | None): The fixed number of paths to draw.
    ///     absolute_tolerance (float | None): The target standard error, the
    ///         alternative to samples.
    ///     max_samples (int | None): The cap on paths drawn when running to a
    ///         tolerance.
    ///     seed (int | None): The RNG seed; the same seed reproduces the NPV
    ///         bitwise.
    ///     antithetic (bool | None): The antithetic variate, supported here;
    ///         the core cached oracle prices with it on.
    ///
    /// Raises:
    ///     ItofinError: If neither or both of steps and steps_per_year are
    ///         given, or if both samples and absolute_tolerance are given.
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

/// The Longstaff-Schwartz least-squares Monte Carlo engine for American
/// payoffs, over the pseudo-random RNG policy. The low-discrepancy policy is
/// not exposed (#454), and the Monomial regression basis is not selectable
/// (#453).
///
/// The option priced must come from VanillaOption.american(...): a
/// European-exercise option raises ItofinError ("wrong exercise given") when
/// priced here.
///
/// Pricing is seeded and deterministic: the same seed reproduces the NPV
/// bitwise, the standard error is read back through
/// VanillaOption.error_estimate() and the early-exercise fraction through
/// VanillaOption.exercise_probability().
#[pyclass(name = "MCAmericanEngine", unsendable)]
pub struct PyMCAmericanEngine {
    inner: SharedMut<MCAmericanEngine<PseudoRandom>>,
}

#[pymethods]
impl PyMCAmericanEngine {
    /// Build an engine over process, configured through the core factory.
    ///
    /// Every argument past process is left unset when omitted, so the core's
    /// own validation reports the illegal combinations.
    ///
    /// Args:
    ///     process (BlackScholesProcess): The process paths are drawn from.
    ///     steps (int | None): The fixed number of time steps per path.
    ///     steps_per_year (int | None): The time steps per year, the
    ///         alternative to steps.
    ///     samples (int | None): The fixed number of paths to draw.
    ///     absolute_tolerance (float | None): The target standard error, the
    ///         alternative to samples.
    ///     max_samples (int | None): The cap on paths drawn when running to a
    ///         tolerance.
    ///     seed (int | None): The RNG seed; the same seed reproduces the NPV
    ///         bitwise.
    ///     antithetic (bool | None): The antithetic variate, supported here;
    ///         the core oracle prices with it on.
    ///     polynomial_order (int | None): The order of the Monomial regression
    ///         basis. The core default is 2.
    ///     calibration_samples (int | None): The paths the regression is fitted
    ///         on. The core default is 2048.
    ///
    /// Raises:
    ///     ItofinError: If neither or both of steps and steps_per_year are
    ///         given, or if both samples and absolute_tolerance are given.
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
        polynomial_order = None,
        calibration_samples = None,
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
        polynomial_order: Option<usize>,
        calibration_samples: Option<usize>,
    ) -> PyResult<Self> {
        let mut maker = MakeMcAmericanEngine::<PseudoRandom>::new(process.inner());
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
        if let Some(order) = polynomial_order {
            maker = maker.with_polynomial_order(order);
        }
        if let Some(samples) = calibration_samples {
            maker = maker.with_calibration_samples(samples);
        }
        let engine = maker.build().map_err(PyQlError::from)?;
        Ok(PyMCAmericanEngine {
            inner: shared_mut(engine),
        })
    }
}

impl PyMCAmericanEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
