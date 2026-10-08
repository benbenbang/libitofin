//! Bounded global methods accepted by the existing calibration dispatch.

use crate::ItofinError;
use crate::optimize::PyOptimizeResult;
use itofin_optimize::{
    Bounds, Common, DifferentialEvolutionOptions, GlobalOptions, HybridSimulatedAnnealingOptions,
    ParticleSwarmOptions,
};
use libitofin::math::optimization::global::{
    DifferentialEvolution, HybridSimulatedAnnealing, ParticleSwarm,
};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Bounded global calibration in projected free-parameter order.
/// The complete search box must satisfy the model's constraint.
#[gen_stub_pyclass]
#[pyclass(
    name = "DifferentialEvolution",
    unsendable,
    module = "itofin.optimization"
)]
pub struct PyDifferentialEvolution {
    inner: DifferentialEvolution,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyDifferentialEvolution {
    /// Construct a bounded, deterministic DE/rand/1/bin calibration method.
    /// Bounds and population coordinates use the free parameter order.
    /// Invalid candidates abort before pricing rather than receiving a penalty.
    #[new]
    #[pyo3(signature = (bounds, *, seed=0, population_size=None, initial_population=None, xatol=None, fatol=None, mutation=0.8, recombination=0.9, maxiter=None, maxfev=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        bounds: Vec<(f64, f64)>,
        #[pyo3(from_py_with = crate::optimize::strict_global_seed)] seed: u64,
        #[pyo3(from_py_with = extract_population_size)] population_size: Option<usize>,
        initial_population: Option<Vec<Vec<f64>>>,
        xatol: Option<f64>,
        fatol: Option<f64>,
        mutation: f64,
        recombination: f64,
        #[pyo3(from_py_with = extract_maxiter)] maxiter: Option<usize>,
        #[pyo3(from_py_with = extract_maxfev)] maxfev: Option<usize>,
    ) -> PyResult<Self> {
        let bounds = Bounds {
            lower: bounds.iter().map(|pair| pair.0).collect(),
            upper: bounds.iter().map(|pair| pair.1).collect(),
        };
        let options = DifferentialEvolutionOptions {
            global: GlobalOptions {
                seed,
                population_size,
                initial_population,
                xatol,
                fatol,
            },
            mutation,
            recombination,
        };
        let inner = DifferentialEvolution::new(
            bounds,
            options,
            Common {
                maxiter,
                maxfev,
                tol: None,
            },
        )
        .map_err(|error| ItofinError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Copy the exact last global result, or None before a completed run.
    /// Exhausted runs are retained without being labelled successful.
    fn last_result(&self) -> PyResult<Option<PyOptimizeResult>> {
        self.inner
            .last_result()
            .cloned()
            .map(PyOptimizeResult::from_core)
            .transpose()
    }
}

impl PyDifferentialEvolution {
    pub(crate) fn inner_mut(&mut self) -> &mut DifferentialEvolution {
        &mut self.inner
    }
}

/// Bounded global calibration in projected free-parameter order.
/// The complete search box must satisfy the model's constraint.
#[gen_stub_pyclass]
#[pyclass(name = "ParticleSwarm", unsendable, module = "itofin.optimization")]
pub struct PyParticleSwarm {
    inner: ParticleSwarm,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyParticleSwarm {
    /// Construct a bounded, deterministic global-best particle swarm calibration method.
    /// Bounds and population coordinates use the free parameter order.
    /// Invalid candidates abort before pricing rather than receiving a penalty.
    #[new]
    #[pyo3(signature = (bounds, *, seed=0, population_size=None, initial_population=None, xatol=None, fatol=None, inertia=0.7, cognitive=1.4, social=1.4, velocity_clamp=0.2, maxiter=None, maxfev=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        bounds: Vec<(f64, f64)>,
        #[pyo3(from_py_with = crate::optimize::strict_global_seed)] seed: u64,
        #[pyo3(from_py_with = extract_population_size)] population_size: Option<usize>,
        initial_population: Option<Vec<Vec<f64>>>,
        xatol: Option<f64>,
        fatol: Option<f64>,
        inertia: f64,
        cognitive: f64,
        social: f64,
        velocity_clamp: f64,
        #[pyo3(from_py_with = extract_maxiter)] maxiter: Option<usize>,
        #[pyo3(from_py_with = extract_maxfev)] maxfev: Option<usize>,
    ) -> PyResult<Self> {
        let bounds = Bounds {
            lower: bounds.iter().map(|pair| pair.0).collect(),
            upper: bounds.iter().map(|pair| pair.1).collect(),
        };
        let options = ParticleSwarmOptions {
            global: GlobalOptions {
                seed,
                population_size,
                initial_population,
                xatol,
                fatol,
            },
            inertia,
            cognitive,
            social,
            velocity_clamp,
        };
        let inner = ParticleSwarm::new(
            bounds,
            options,
            Common {
                maxiter,
                maxfev,
                tol: None,
            },
        )
        .map_err(|error| ItofinError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Copy the exact last global result, or None before a completed run.
    /// Exhausted runs are retained without being labelled successful.
    fn last_result(&self) -> PyResult<Option<PyOptimizeResult>> {
        self.inner
            .last_result()
            .cloned()
            .map(PyOptimizeResult::from_core)
            .transpose()
    }
}

impl PyParticleSwarm {
    pub(crate) fn inner_mut(&mut self) -> &mut ParticleSwarm {
        &mut self.inner
    }
}

/// Bounded hybrid annealing calibration in projected free-parameter order.
/// The complete search box must satisfy the model's constraint.
#[gen_stub_pyclass]
#[pyclass(
    name = "HybridSimulatedAnnealing",
    unsendable,
    module = "itofin.optimization"
)]
pub struct PyHybridSimulatedAnnealing {
    inner: HybridSimulatedAnnealing,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyHybridSimulatedAnnealing {
    /// Construct a seeded single-chain annealing method with coordinate local searches.
    /// Bounds use the free parameter order; no population is constructed.
    /// Invalid candidates abort before pricing rather than receiving a penalty.
    #[new]
    #[pyo3(signature = (bounds, *, seed=0, initial_temperature=1.0, cooling_rate=0.95, step_size=0.25, local_search_interval=10, local_search_steps=4, reanneal_interval=100, xatol=None, fatol=None, maxiter=None, maxfev=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        bounds: Vec<(f64, f64)>,
        #[pyo3(from_py_with = crate::optimize::strict_global_seed)] seed: u64,
        initial_temperature: f64,
        cooling_rate: f64,
        step_size: f64,
        #[pyo3(from_py_with = extract_local_search_interval)] local_search_interval: usize,
        #[pyo3(from_py_with = extract_local_search_steps)] local_search_steps: usize,
        #[pyo3(from_py_with = extract_reanneal_interval)] reanneal_interval: usize,
        xatol: Option<f64>,
        fatol: Option<f64>,
        #[pyo3(from_py_with = extract_maxiter)] maxiter: Option<usize>,
        #[pyo3(from_py_with = extract_maxfev)] maxfev: Option<usize>,
    ) -> PyResult<Self> {
        let bounds = Bounds {
            lower: bounds.iter().map(|pair| pair.0).collect(),
            upper: bounds.iter().map(|pair| pair.1).collect(),
        };
        let options = HybridSimulatedAnnealingOptions {
            seed,
            initial_temperature,
            cooling_rate,
            step_size,
            local_search_interval,
            local_search_steps,
            reanneal_interval,
            xatol,
            fatol,
        };
        let inner = HybridSimulatedAnnealing::new(
            bounds,
            options,
            Common {
                maxiter,
                maxfev,
                tol: None,
            },
        )
        .map_err(|error| ItofinError::new_err(error.to_string()))?;
        Ok(Self { inner })
    }

    /// Copy the exact last result, or None before a completed run.
    /// Exhausted runs retain their actual counters without being labelled successful.
    fn last_result(&self) -> PyResult<Option<PyOptimizeResult>> {
        self.inner
            .last_result()
            .cloned()
            .map(PyOptimizeResult::from_core)
            .transpose()
    }
}

impl PyHybridSimulatedAnnealing {
    pub(crate) fn inner_mut(&mut self) -> &mut HybridSimulatedAnnealing {
        &mut self.inner
    }
}

fn extract_local_search_interval(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    crate::optimize::strict_global_usize(value, "local_search_interval")
}
fn extract_local_search_steps(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    crate::optimize::strict_global_usize(value, "local_search_steps")
}
fn extract_reanneal_interval(value: &Bound<'_, PyAny>) -> PyResult<usize> {
    crate::optimize::strict_global_usize(value, "reanneal_interval")
}

fn optional_integer(value: &Bound<'_, PyAny>, name: &str) -> PyResult<Option<usize>> {
    if value.is_none() {
        Ok(None)
    } else {
        crate::optimize::strict_global_usize(value, name).map(Some)
    }
}

fn extract_population_size(value: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
    optional_integer(value, "population_size")
}
fn extract_maxiter(value: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
    optional_integer(value, "maxiter")
}
fn extract_maxfev(value: &Bound<'_, PyAny>) -> PyResult<Option<usize>> {
    optional_integer(value, "maxfev")
}
