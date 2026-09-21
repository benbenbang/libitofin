//! Concrete fallible Poisson generators with independent copied state.

use crate::PyQlError;
use libitofin::math::randomnumbers::{PoissonPseudoRandom, PoissonRng, PoissonRsg};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Scalar Poisson variates from MT19937. Seed zero selects a random seed.
#[gen_stub_pyclass]
#[pyclass(
    name = "PoissonRandomGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyPoissonRandomGenerator {
    inner: PoissonRng,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPoissonRandomGenerator {
    /// Construct with a finite positive rate; the default rate is one.
    #[new]
    #[pyo3(signature = (seed = 0, lambda_ = 1.0))]
    fn new(seed: u32, lambda_: f64) -> PyResult<Self> {
        Ok(Self {
            inner: PoissonPseudoRandom::make_scalar_generator(seed, lambda_)
                .map_err(PyQlError::from)?,
        })
    }

    /// Draw the next count. Unresolvable quantiles raise ItofinError.
    fn next_real(&mut self) -> PyResult<f64> {
        Ok(self.inner.next_sample().map_err(PyQlError::from)?.value)
    }

    /// Copy the current state without advancing either generator.
    fn copy(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

/// Weighted Poisson sequences from MT19937, with an explicit per-generator rate.
#[gen_stub_pyclass]
#[pyclass(
    name = "PoissonRandomSequenceGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyPoissonRandomSequenceGenerator {
    inner: PoissonRsg,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPoissonRandomSequenceGenerator {
    /// Construct a positive-dimensional sequence; the default rate is one.
    #[new]
    #[pyo3(signature = (dimension, seed = 0, lambda_ = 1.0))]
    fn new(dimension: usize, seed: u32, lambda_: f64) -> PyResult<Self> {
        if !(1..=16 * 1024 * 1024).contains(&dimension) {
            return Err(crate::ItofinError::new_err(
                "dimension outside [1, 16777216]",
            ));
        }
        Ok(Self {
            inner: PoissonPseudoRandom::with_lambda(dimension, seed, lambda_)
                .map_err(PyQlError::from)?,
        })
    }

    /// Number of components per draw.
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    /// Draw one sequence; an error preserves the last successful sequence.
    fn next_sequence(&mut self) -> PyResult<Vec<f64>> {
        Ok(self
            .inner
            .next_sequence()
            .map_err(PyQlError::from)?
            .value
            .clone())
    }

    /// Last successful sequence, initially zeros.
    fn last_sequence(&self) -> Vec<f64> {
        self.inner.last_sequence().value.clone()
    }

    /// Copy the current state without advancing either generator.
    fn copy(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}
