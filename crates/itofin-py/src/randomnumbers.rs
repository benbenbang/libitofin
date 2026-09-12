//! Facades for the random-number generators: the uniform Mersenne-Twister
//! generator.
//!
//! The classes carry QuantLib's Python (SWIG) names rather than the C++ ones,
//! so `UniformRandomGenerator` stands for `MersenneTwisterUniformRng`: a
//! QuantLib Python caller swaps the import and keeps the call sites.
//!
//! Every draw is returned as a value, not as QuantLib's weighted `Sample`
//! wrapper; the weight of a pseudo-random draw is always 1.0. Vector draws come
//! back as NumPy arrays, and the batch methods draw many sequences in one call
//! so a Monte Carlo loop over paths does not cross the binding once per path.

use libitofin::math::randomnumbers::{MersenneTwisterUniformRng, UniformRng};
use numpy::PyArray1;
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
