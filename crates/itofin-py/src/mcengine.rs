//! Concrete pseudo-random and low-discrepancy Monte Carlo engine facades.

use crate::PyQlError;
use crate::heston::PyHestonProcess;
use crate::market::PyBlackScholesProcess;
use libitofin::math::randomnumbers::rngtraits::{LowDiscrepancy, PseudoRandom};
use libitofin::methods::montecarlo::PolynomialType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::{
    MCAmericanEngine, MCEuropeanEngine, MCEuropeanHestonEngine, MakeMcAmericanEngine,
    MakeMcEuropeanEngine, MakeMcEuropeanHestonEngine,
};
use libitofin::shared::{SharedMut, shared_mut};
use pyo3::prelude::*;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// The Monte Carlo engine for European payoffs, over the pseudo-random RNG
/// policy.
///
/// Pricing is seeded and deterministic: the same seed reproduces the NPV
/// bitwise, and the standard error is read back through
/// VanillaOption.error_estimate().
#[gen_stub_pyclass]
#[pyclass(
    name = "MCEuropeanEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyMCEuropeanEngine {
    inner: SharedMut<MCEuropeanEngine<PseudoRandom>>,
}

#[gen_stub_pymethods]
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
#[gen_stub_pyclass]
#[pyclass(
    name = "MCEuropeanHestonEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyMCEuropeanHestonEngine {
    inner: SharedMut<MCEuropeanHestonEngine<PseudoRandom>>,
}

#[gen_stub_pymethods]
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
/// and Bermudan payoffs, over the pseudo-random RNG policy. Sobol, Brownian
/// bridge, control variates and multi-asset paths are not exposed.
///
/// The option must have American or Bermudan exercise: a
/// European-exercise option raises ItofinError ("wrong exercise given") when
/// priced here.
///
/// Pricing is seeded and deterministic: the same seed reproduces the NPV
/// bitwise, the standard error is read back through
/// VanillaOption.error_estimate() and the early-exercise fraction through
/// VanillaOption.exercise_probability().
#[gen_stub_pyclass]
#[pyclass(
    name = "MCAmericanEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyMCAmericanEngine {
    inner: SharedMut<MCAmericanEngine<PseudoRandom>>,
}

#[gen_stub_pymethods]
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
    ///     polynomial_order (int | None): The order of the selected regression
    ///         basis. The core default is 2.
    ///     calibration_samples (int | None): The paths the regression is fitted
    ///         on. The core default is 2048.
    ///     basis_system (int): 0 Monomial (default), 1 Laguerre, 2 Hermite,
    ///         3 Hyperbolic, or 6 Chebyshev2nd. Legendre (4) and Chebyshev (5)
    ///         are rejected, matching QuantLib's American path pricer.
    ///         Chebyshev2nd supports put payoffs only; call pricing returns an error.
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
        basis_system = 0,
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
        basis_system: i32,
    ) -> PyResult<Self> {
        let basis = match basis_system {
            0 => PolynomialType::Monomial,
            1 => PolynomialType::Laguerre,
            2 => PolynomialType::Hermite,
            3 => PolynomialType::Hyperbolic,
            4 => PolynomialType::Legendre,
            5 => PolynomialType::Chebyshev,
            6 => PolynomialType::Chebyshev2nd,
            _ => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "unknown polynomial basis",
                ));
            }
        };
        let mut maker =
            MakeMcAmericanEngine::<PseudoRandom>::new(process.inner()).with_basis_system(basis);
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

/// Fixed-sample Sobol European engine, without an error estimate.
#[gen_stub_pyclass]
#[pyclass(
    name = "QMCEuropeanEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyQMCEuropeanEngine {
    inner: SharedMut<MCEuropeanEngine<LowDiscrepancy>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyQMCEuropeanEngine {
    /// Build a Sobol engine with a positive fixed sample count.
    ///
    /// No statistical error estimate is available. Absolute tolerance and
    /// max_samples are rejected. Seed selects Sobol direction initialization;
    /// it is deterministic even when omitted or zero.
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
        let mut maker = MakeMcEuropeanEngine::<LowDiscrepancy>::new(process.inner());
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
        Ok(PyQMCEuropeanEngine {
            inner: shared_mut(engine),
        })
    }
}

impl PyQMCEuropeanEngine {
    /// The erased engine the instrument facades install via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}
