//! Facades for the random-number generators: the uniform Mersenne-Twister
//! generator, the sequence generator built on it, and the Gaussian generator
//! layered over the former.
//!
//! The classes carry QuantLib's Python (SWIG) names rather than the C++ ones,
//! so `UniformRandomGenerator` stands for `MersenneTwisterUniformRng`,
//! `UniformRandomSequenceGenerator` for
//! `RandomSequenceGenerator<MersenneTwisterUniformRng>` and
//! `GaussianRandomGenerator` for `BoxMullerGaussianRng` over the Mersenne
//! Twister: a QuantLib Python caller swaps the import and keeps the call sites.
//!
//! Every draw is returned as a value, not as QuantLib's weighted `Sample`
//! wrapper; the weight of a pseudo-random draw is always 1.0. Vector draws come
//! back as NumPy arrays, and the batch methods draw many sequences in one call
//! so a Monte Carlo loop over paths does not cross the binding once per path.

use crate::PyQlError;
use libitofin::math::randomnumbers::rngtraits::SequenceGenerator;
use libitofin::math::randomnumbers::{
    BoxMullerGaussianRng, GaussianRng, MersenneTwisterUniformRng, RandomSequenceGenerator,
    UniformRng,
};
use numpy::{PyArray1, PyArray2, PyArrayMethods};
use pyo3::prelude::*;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// A buffer for `len` draws, or an ItofinError when it cannot be allocated.
///
/// The batch methods take their count from Python, so a count past the
/// address space (or past what the allocator will grant) must surface as an
/// error rather than the abort `Vec::with_capacity` would raise.
fn draw_buffer(len: usize) -> PyResult<Vec<f64>> {
    let mut buffer = Vec::new();
    buffer.try_reserve_exact(len).map_err(|_| {
        crate::ItofinError::new_err(format!("cannot allocate a buffer of {len} draws"))
    })?;
    Ok(buffer)
}

/// Draws `count` values from `next` into a NumPy array of shape `(count,)`.
///
/// # Errors
///
/// Returns an error when the buffer cannot be allocated.
pub(crate) fn draw_vector<'py>(
    py: Python<'py>,
    count: usize,
    mut next: impl FnMut() -> f64,
) -> PyResult<Bound<'py, PyArray1<f64>>> {
    let mut draws = draw_buffer(count)?;
    draws.extend((0..count).map(|_| next()));
    Ok(PyArray1::from_vec(py, draws))
}

/// Draws `count` sequences of `dimension` values from `next`, one row per
/// sequence, as a C-ordered `(count, dimension)` NumPy array.
///
/// The rows are drawn in order, so the array holds exactly what `count`
/// successive single draws would have returned.
///
/// # Errors
///
/// Returns an error when `count * dimension` overflows or the buffer cannot
/// be allocated.
pub(crate) fn draw_matrix<'py>(
    py: Python<'py>,
    count: usize,
    dimension: usize,
    mut next: impl FnMut() -> Vec<f64>,
) -> PyResult<Bound<'py, PyArray2<f64>>> {
    let len = count.checked_mul(dimension).ok_or_else(|| {
        crate::ItofinError::new_err(format!(
            "{count} sequences of dimension {dimension} overflow the draw buffer"
        ))
    })?;
    let mut flat = draw_buffer(len)?;
    for _ in 0..count {
        flat.extend(next());
    }
    let array = PyArray1::from_vec(py, flat);
    // The row length is fixed by construction, so the reshape cannot fail.
    Ok(array
        .reshape([count, dimension])
        .expect("every drawn row has the generator's dimension"))
}

/// The uniform pseudo-random number generator: a Mersenne Twister (MT19937)
/// with period 2^19937 - 1, QuantLib's `MersenneTwisterUniformRng`.
///
/// Draws are deterministic for a non-zero seed: the same seed reproduces the
/// same stream bitwise, on every platform. A seed of 0 draws a random seed
/// from the core seed generator, so two zero-seeded generators diverge.
#[gen_stub_pyclass]
#[pyclass(
    name = "UniformRandomGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyUniformRandomGenerator {
    inner: MersenneTwisterUniformRng,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyUniformRandomGenerator {
    /// Build a generator from a seed.
    ///
    /// Args:
    ///     seed (int): The 32-bit seed. 0 (the default) draws a random seed
    ///         from the core seed generator, matching QuantLib.
    #[new]
    #[pyo3(signature = (seed = 0))]
    fn new(seed: u32) -> Self {
        PyUniformRandomGenerator {
            inner: MersenneTwisterUniformRng::new(seed),
        }
    }

    /// Build a generator from an array of seeds, the reference
    /// `init_by_array` initialization.
    ///
    /// Args:
    ///     seeds (list[int]): The 32-bit seed words; at least one.
    ///
    /// Returns:
    ///     UniformRandomGenerator: The generator initialized from the array.
    ///
    /// Raises:
    ///     ItofinError: If seeds is empty.
    #[staticmethod]
    fn from_seeds(seeds: Vec<u32>) -> PyResult<Self> {
        if seeds.is_empty() {
            return Err(crate::ItofinError::new_err("empty seed array"));
        }
        Ok(PyUniformRandomGenerator {
            inner: MersenneTwisterUniformRng::from_seeds(&seeds),
        })
    }

    /// Draw the next uniform deviate.
    ///
    /// Returns:
    ///     float: A deviate strictly inside (0, 1): the raw 32-bit output
    ///     shifted by one half and scaled by 2^-32, so neither endpoint is
    ///     ever returned.
    fn next_real(&mut self) -> f64 {
        self.inner.next_real()
    }

    /// Draw the next raw 32-bit output.
    ///
    /// Returns:
    ///     int: An integer uniform over [0, 2^32 - 1].
    fn next_u32(&mut self) -> u32 {
        self.inner.next_u32()
    }

    /// Draw many uniform deviates in one call.
    ///
    /// Args:
    ///     count (int): The number of deviates to draw.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (count,), holding exactly
    ///     what count successive next_real() calls would have returned.
    ///
    /// Raises:
    ///     ItofinError: If a buffer of count draws cannot be allocated.
    fn next_reals<'py>(
        &mut self,
        py: Python<'py>,
        count: usize,
    ) -> PyResult<Bound<'py, PyArray1<f64>>> {
        draw_vector(py, count, || self.inner.next_real())
    }
}

impl PyUniformRandomGenerator {
    /// A copy of the generator state, for the generators that take the scalar
    /// generator by value as QuantLib does.
    pub(crate) fn inner(&self) -> MersenneTwisterUniformRng {
        self.inner.clone()
    }
}

/// The Gaussian pseudo-random number generator: the polar Box-Muller
/// transform over a Mersenne Twister, QuantLib's
/// `BoxMullerGaussianRng<MersenneTwisterUniformRng>`.
///
/// Each pair of uniform draws yields two standard normal deviates; the second
/// is cached and returned by the next call, as in QuantLib.
#[gen_stub_pyclass]
#[pyclass(
    name = "GaussianRandomGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyGaussianRandomGenerator {
    inner: BoxMullerGaussianRng<MersenneTwisterUniformRng>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGaussianRandomGenerator {
    /// Build a generator over a copy of a uniform generator.
    ///
    /// Args:
    ///     rng (UniformRandomGenerator): The uniform generator to copy the
    ///         state from.
    #[new]
    fn new(rng: &PyUniformRandomGenerator) -> Self {
        PyGaussianRandomGenerator {
            inner: BoxMullerGaussianRng::new(rng.inner()),
        }
    }

    /// Build a generator over a fresh Mersenne Twister.
    ///
    /// Args:
    ///     seed (int): The 32-bit seed; 0 draws a random seed.
    ///
    /// Returns:
    ///     GaussianRandomGenerator: The seeded generator.
    #[staticmethod]
    #[pyo3(signature = (seed = 0))]
    fn with_seed(seed: u32) -> Self {
        PyGaussianRandomGenerator {
            inner: BoxMullerGaussianRng::new(MersenneTwisterUniformRng::new(seed)),
        }
    }

    /// Draw the next standard normal deviate.
    ///
    /// Returns:
    ///     float: A deviate with mean 0 and standard deviation 1.
    fn next_gaussian(&mut self) -> f64 {
        self.inner.next_gaussian()
    }

    /// Draw many standard normal deviates in one call.
    ///
    /// Args:
    ///     count (int): The number of deviates to draw.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (count,), holding exactly
    ///     what count successive next_gaussian() calls would have returned.
    ///
    /// Raises:
    ///     ItofinError: If a buffer of count draws cannot be allocated.
    fn next_gaussians<'py>(
        &mut self,
        py: Python<'py>,
        count: usize,
    ) -> PyResult<Bound<'py, PyArray1<f64>>> {
        draw_vector(py, count, || self.inner.next_gaussian())
    }
}

/// The uniform random sequence generator: `dimension` Mersenne-Twister draws
/// per sequence, QuantLib's `RandomSequenceGenerator<MersenneTwisterUniformRng>`.
///
/// The generator copies the scalar generator it is built from, as QuantLib
/// does, so later draws on the original do not affect the sequence.
#[gen_stub_pyclass]
#[pyclass(
    name = "UniformRandomSequenceGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyUniformRandomSequenceGenerator {
    inner: RandomSequenceGenerator<MersenneTwisterUniformRng>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyUniformRandomSequenceGenerator {
    /// Build a sequence generator over a copy of a scalar generator.
    ///
    /// Args:
    ///     dimension (int): The number of draws per sequence, at least 1.
    ///     rng (UniformRandomGenerator): The scalar generator to copy the
    ///         state from.
    ///
    /// Raises:
    ///     ItofinError: If dimension is 0.
    #[new]
    fn new(dimension: usize, rng: &PyUniformRandomGenerator) -> PyResult<Self> {
        let inner =
            RandomSequenceGenerator::new(dimension, rng.inner()).map_err(PyQlError::from)?;
        Ok(PyUniformRandomSequenceGenerator { inner })
    }

    /// Build a sequence generator over a fresh Mersenne Twister.
    ///
    /// Args:
    ///     dimension (int): The number of draws per sequence, at least 1.
    ///     seed (int): The 32-bit seed; 0 draws a random seed.
    ///
    /// Returns:
    ///     UniformRandomSequenceGenerator: The seeded sequence generator.
    ///
    /// Raises:
    ///     ItofinError: If dimension is 0.
    #[staticmethod]
    #[pyo3(signature = (dimension, seed = 0))]
    fn with_seed(dimension: usize, seed: u32) -> PyResult<Self> {
        let inner = RandomSequenceGenerator::with_seed(dimension, seed).map_err(PyQlError::from)?;
        Ok(PyUniformRandomSequenceGenerator { inner })
    }

    /// The number of draws per sequence.
    ///
    /// Returns:
    ///     int: The dimension the generator was built with.
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    /// Draw the next sequence.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (dimension,), every entry
    ///     strictly inside (0, 1).
    fn next_sequence<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_vec(py, self.inner.next_sequence().value.clone())
    }

    /// The most recently drawn sequence, without advancing.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (dimension,); all zeros
    ///     before the first draw.
    fn last_sequence<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_vec(py, self.inner.last_sequence().value.clone())
    }

    /// Draw many sequences in one call.
    ///
    /// Args:
    ///     count (int): The number of sequences to draw.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (count, dimension), row i
    ///     being what the (i + 1)-th next_sequence() call would have returned.
    ///
    /// Raises:
    ///     ItofinError: If a buffer of count sequences cannot be allocated.
    fn next_sequences<'py>(
        &mut self,
        py: Python<'py>,
        count: usize,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let dimension = self.inner.dimension();
        draw_matrix(py, count, dimension, || {
            self.inner.next_sequence().value.clone()
        })
    }
}
