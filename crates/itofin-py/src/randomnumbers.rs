//! Facades for the random-number generators: the uniform Mersenne-Twister
//! generator, the sequence generator built on it, the Gaussian generators
//! layered over both, and the Sobol and Halton low-discrepancy sequences.
//!
//! The classes carry QuantLib's Python (SWIG) names rather than the C++ ones,
//! so `UniformRandomGenerator` stands for `MersenneTwisterUniformRng`,
//! `UniformRandomSequenceGenerator` for
//! `RandomSequenceGenerator<MersenneTwisterUniformRng>`,
//! `GaussianRandomGenerator` for `BoxMullerGaussianRng` over the Mersenne
//! Twister and `GaussianRandomSequenceGenerator` for `InverseCumulativeRsg`
//! over the uniform sequence generator and the inverse cumulative normal (the
//! `PseudoRandom` policy the Monte Carlo engines draw from): a QuantLib Python
//! caller swaps the import and keeps the call sites. `SobolRsg` and
//! `HaltonRsg` keep the C++ names, as QuantLib's Python API does, and
//! `GaussianLowDiscrepancySequenceGenerator` is the inverse cumulative normal
//! over a Sobol sequence.
//!
//! Deferred (visible): the Monte Carlo engines still pin the pseudo-random
//! policy; the low-discrepancy policy behind `testQmcEngines` lands with the
//! core's `LowDiscrepancy` traits (#454). The randomized Halton starts and
//! shifts are deferred in the core, so `HaltonRsg` is the deterministic
//! sequence.
//!
//! Every draw is returned as a value, not as QuantLib's weighted `Sample`
//! wrapper; the weight of a pseudo-random draw is always 1.0. Vector draws come
//! back as NumPy arrays, and the batch methods draw many sequences in one call
//! so a Monte Carlo loop over paths does not cross the binding once per path.

use crate::PyQlError;
use libitofin::math::distributions::normal::InverseCumulativeNormal;
use libitofin::math::randomnumbers::rngtraits::SequenceGenerator;
use libitofin::math::randomnumbers::sobol::{DirectionIntegers, PPMT_MAX_DIM, SobolRsg};
use libitofin::math::randomnumbers::{
    BoxMullerGaussianRng, GaussianRng, HaltonRsg, InverseCumulativeRsg, MersenneTwisterUniformRng,
    RandomSequenceGenerator, UniformRng,
};
use libitofin::methods::montecarlo::Sample;
use libitofin::types::Real;
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

impl PyUniformRandomSequenceGenerator {
    /// A copy of the sequence generator state, for the Gaussian sequence
    /// generator that takes it by value as QuantLib does.
    pub(crate) fn inner(&self) -> RandomSequenceGenerator<MersenneTwisterUniformRng> {
        self.inner.clone()
    }
}

/// The Gaussian random sequence generator: uniform Mersenne-Twister sequences
/// mapped through the inverse cumulative normal, QuantLib's
/// `InverseCumulativeRsg<RandomSequenceGenerator<MersenneTwisterUniformRng>,
/// InverseCumulativeNormal>`.
///
/// This is the `PseudoRandom` policy the Monte Carlo engines draw their paths
/// from: with_seed(dimension, seed) reproduces the engines' generator for the
/// same dimension and seed. The generator copies the uniform sequence generator
/// it is built from, as QuantLib does.
#[gen_stub_pyclass]
#[pyclass(
    name = "GaussianRandomSequenceGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyGaussianRandomSequenceGenerator {
    inner: InverseCumulativeRsg<
        RandomSequenceGenerator<MersenneTwisterUniformRng>,
        InverseCumulativeNormal,
    >,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGaussianRandomSequenceGenerator {
    /// Build a Gaussian sequence generator over a copy of a uniform one.
    ///
    /// Args:
    ///     usg (UniformRandomSequenceGenerator): The uniform sequence generator
    ///         to copy the state from; its dimension is the dimension here.
    #[new]
    fn new(usg: &PyUniformRandomSequenceGenerator) -> Self {
        PyGaussianRandomSequenceGenerator {
            inner: InverseCumulativeRsg::new(usg.inner(), InverseCumulativeNormal::standard()),
        }
    }

    /// Build a Gaussian sequence generator over a fresh Mersenne Twister.
    ///
    /// Args:
    ///     dimension (int): The number of draws per sequence, at least 1.
    ///     seed (int): The 32-bit seed; 0 draws a random seed.
    ///
    /// Returns:
    ///     GaussianRandomSequenceGenerator: The seeded sequence generator.
    ///
    /// Raises:
    ///     ItofinError: If dimension is 0.
    #[staticmethod]
    #[pyo3(signature = (dimension, seed = 0))]
    fn with_seed(dimension: usize, seed: u32) -> PyResult<Self> {
        let usg = RandomSequenceGenerator::with_seed(dimension, seed).map_err(PyQlError::from)?;
        Ok(PyGaussianRandomSequenceGenerator {
            inner: InverseCumulativeRsg::new(usg, InverseCumulativeNormal::standard()),
        })
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
    ///     numpy.ndarray: A float64 array of shape (dimension,) of standard
    ///     normal deviates.
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

/// The choice of free direction integers for the Sobol dimensions beyond the
/// first, QuantLib's `SobolRsg::DirectionIntegers`.
///
/// Jaeckel is QuantLib's default. Unit uses the unit initialization for every
/// dimension; the others are the tabulated initializers shipped with QuantLib,
/// with a seeded Mersenne Twister drawing the free integers past each table.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "DirectionIntegers",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.randomnumbers"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyDirectionIntegers {
    Unit,
    Jaeckel,
    SobolLevitan,
    SobolLevitanLemieux,
    JoeKuoD5,
    JoeKuoD6,
    JoeKuoD7,
    Kuo,
    Kuo2,
    Kuo3,
}

impl PyDirectionIntegers {
    /// The core direction-integer choice this variant stands for.
    pub(crate) fn inner(self) -> DirectionIntegers {
        match self {
            PyDirectionIntegers::Unit => DirectionIntegers::Unit,
            PyDirectionIntegers::Jaeckel => DirectionIntegers::Jaeckel,
            PyDirectionIntegers::SobolLevitan => DirectionIntegers::SobolLevitan,
            PyDirectionIntegers::SobolLevitanLemieux => DirectionIntegers::SobolLevitanLemieux,
            PyDirectionIntegers::JoeKuoD5 => DirectionIntegers::JoeKuoD5,
            PyDirectionIntegers::JoeKuoD6 => DirectionIntegers::JoeKuoD6,
            PyDirectionIntegers::JoeKuoD7 => DirectionIntegers::JoeKuoD7,
            PyDirectionIntegers::Kuo => DirectionIntegers::Kuo,
            PyDirectionIntegers::Kuo2 => DirectionIntegers::Kuo2,
            PyDirectionIntegers::Kuo3 => DirectionIntegers::Kuo3,
        }
    }
}

/// The Sobol low-discrepancy sequence generator, QuantLib's `SobolRsg`.
///
/// Successive draws fill the unit hypercube evenly rather than randomly, so a
/// Monte Carlo estimate over them converges faster than over pseudo-random
/// draws. The first draw is 0.5 in every dimension, and every draw lies
/// strictly inside (0, 1). The generator is deterministic for a given seed:
/// the seed only matters for dimensions beyond the tabulated initializers.
#[gen_stub_pyclass]
#[pyclass(name = "SobolRsg", unsendable, module = "itofin.randomnumbers")]
pub struct PySobolRsg {
    inner: SobolRsg,
}

#[gen_stub_pymethods]
#[pymethods]
impl PySobolRsg {
    /// Build a Sobol generator.
    ///
    /// Args:
    ///     dimension (int): The number of draws per sequence, from 1 to the
    ///         number of primitive polynomials shipped (21200).
    ///     seed (int): The seed for the free direction integers past the
    ///         tabulated dimensions; used literally, so 0 is a fixed seed.
    ///     direction_integers (DirectionIntegers): The direction-integer
    ///         table, Jaeckel by default as in QuantLib.
    ///     use_gray_code (bool): Generate through the Gray-code counter (the
    ///         QuantLib default) rather than the plain counter.
    ///
    /// Raises:
    ///     ItofinError: If dimension is 0 or exceeds 21200.
    #[new]
    #[pyo3(signature = (
        dimension,
        seed = 0,
        direction_integers = PyDirectionIntegers::Jaeckel,
        use_gray_code = true,
    ))]
    fn new(
        dimension: usize,
        seed: u64,
        direction_integers: PyDirectionIntegers,
        use_gray_code: bool,
    ) -> PyResult<Self> {
        if dimension == 0 || dimension > PPMT_MAX_DIM {
            return Err(crate::ItofinError::new_err(format!(
                "dimension {dimension} outside [1, {PPMT_MAX_DIM}]"
            )));
        }
        Ok(PySobolRsg {
            inner: SobolRsg::with_gray_code(
                dimension,
                seed,
                direction_integers.inner(),
                use_gray_code,
            ),
        })
    }

    /// The number of draws per sequence.
    ///
    /// Returns:
    ///     int: The dimension the generator was built with.
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    /// Draw the next Sobol point.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (dimension,), every entry
    ///     strictly inside (0, 1).
    fn next_sequence<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_vec(py, self.inner.next_sequence().to_vec())
    }

    /// The most recently drawn point, without advancing.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (dimension,); all zeros
    ///     before the first draw.
    fn last_sequence<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_vec(py, self.inner.last_sequence().to_vec())
    }

    /// Draw the next point as raw 32-bit Sobol integers.
    ///
    /// Returns:
    ///     numpy.ndarray: A uint32 array of shape (dimension,); the float
    ///     point is this array scaled by 2^-32.
    fn next_int32_sequence<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray1<u32>> {
        PyArray1::from_vec(py, self.inner.next_int32_sequence().to_vec())
    }

    /// Draw many points in one call.
    ///
    /// Args:
    ///     count (int): The number of points to draw.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (count, dimension), row i
    ///     being what the (i + 1)-th next_sequence() call would have returned.
    ///
    /// Raises:
    ///     ItofinError: If a buffer of count points cannot be allocated.
    fn next_sequences<'py>(
        &mut self,
        py: Python<'py>,
        count: usize,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let dimension = self.inner.dimension();
        draw_matrix(py, count, dimension, || self.inner.next_sequence().to_vec())
    }

    /// Skip to the n-th point of the sequence and return it as raw integers.
    ///
    /// QuantLib's skipTo, whose counter semantics are kept: with the Gray-code
    /// counter the following draw returns point n + 1, unless it is the very
    /// first draw made on the generator, which returns point n itself; with
    /// the plain counter the following draw returns point n. The float point
    /// is the returned array scaled by 2^-32.
    ///
    /// Args:
    ///     n (int): The 0-based index of the point to skip to.
    ///
    /// Returns:
    ///     numpy.ndarray: A uint32 array of shape (dimension,), point n as
    ///     raw Sobol integers.
    ///
    /// Raises:
    ///     ItofinError: If n is 2^32 - 1, past the sequence period.
    fn skip_to<'py>(&mut self, py: Python<'py>, n: u32) -> PyResult<Bound<'py, PyArray1<u32>>> {
        if n == u32::MAX {
            return Err(crate::ItofinError::new_err(
                "skip exceeds the Sobol sequence period",
            ));
        }
        Ok(PyArray1::from_vec(py, self.inner.skip_to(n).to_vec()))
    }
}

impl PySobolRsg {
    /// A copy of the generator state, for the Gaussian sequence generator that
    /// takes it by value as QuantLib does.
    pub(crate) fn inner(&self) -> SobolRsg {
        self.inner.clone()
    }
}

/// The Halton low-discrepancy sequence generator, QuantLib's `HaltonRsg`
/// with randomStart and randomShift both off.
///
/// Draw k (1-based) is the radical inverse of k in a distinct prime base per
/// dimension: base 2 for the first dimension, 3 for the second, 5 for the
/// third, and so on. The sequence is deterministic; the randomized start and
/// shift of QuantLib's default constructor are not exposed, being deferred in
/// the core.
#[gen_stub_pyclass]
#[pyclass(name = "HaltonRsg", unsendable, module = "itofin.randomnumbers")]
pub struct PyHaltonRsg {
    inner: HaltonRsg,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyHaltonRsg {
    /// Build a Halton generator.
    ///
    /// Args:
    ///     dimension (int): The number of draws per point, at least 1.
    ///
    /// Raises:
    ///     ItofinError: If dimension is 0.
    #[new]
    fn new(dimension: usize) -> PyResult<Self> {
        Ok(PyHaltonRsg {
            inner: HaltonRsg::new(dimension).map_err(PyQlError::from)?,
        })
    }

    /// The number of draws per point.
    ///
    /// Returns:
    ///     int: The dimension the generator was built with.
    fn dimension(&self) -> usize {
        self.inner.dimension()
    }

    /// Draw the next Halton point.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (dimension,), every entry
    ///     inside [0, 1).
    fn next_sequence<'py>(&mut self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_vec(py, self.inner.next_sequence().to_vec())
    }

    /// The most recently drawn point, without advancing.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (dimension,); all zeros
    ///     before the first draw.
    fn last_sequence<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<f64>> {
        PyArray1::from_vec(py, self.inner.last_sequence().to_vec())
    }

    /// Draw many points in one call.
    ///
    /// Args:
    ///     count (int): The number of points to draw.
    ///
    /// Returns:
    ///     numpy.ndarray: A float64 array of shape (count, dimension), row i
    ///     being what the (i + 1)-th next_sequence() call would have returned.
    ///
    /// Raises:
    ///     ItofinError: If a buffer of count points cannot be allocated.
    fn next_sequences<'py>(
        &mut self,
        py: Python<'py>,
        count: usize,
    ) -> PyResult<Bound<'py, PyArray2<f64>>> {
        let dimension = self.inner.dimension();
        draw_matrix(py, count, dimension, || self.inner.next_sequence().to_vec())
    }
}

/// A Sobol generator presented as a weighted sequence generator, so the core
/// `InverseCumulativeRsg` can map it as it maps the pseudo-random sequences.
///
/// The core `SobolRsg` returns bare slices because a low-discrepancy draw has
/// unit weight; this adapter restores the `Sample` wrapper with that weight.
/// It stands in for the core's deferred `LowDiscrepancy` policy (#454).
#[derive(Clone)]
pub(crate) struct SobolSequence {
    rsg: SobolRsg,
    sample: Sample<Vec<Real>>,
}

impl SobolSequence {
    fn new(rsg: SobolRsg) -> Self {
        let dimension = rsg.dimension();
        SobolSequence {
            rsg,
            sample: Sample::new(vec![0.0; dimension], 1.0),
        }
    }
}

impl SequenceGenerator for SobolSequence {
    fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
        self.sample.value.copy_from_slice(self.rsg.next_sequence());
        &self.sample
    }

    fn last_sequence(&self) -> &Sample<Vec<Real>> {
        &self.sample
    }

    fn dimension(&self) -> usize {
        self.rsg.dimension()
    }
}

/// The Gaussian low-discrepancy sequence generator: Sobol points mapped
/// through the inverse cumulative normal, QuantLib's
/// `InverseCumulativeRsg<SobolRsg, InverseCumulativeNormal>`.
///
/// The first draw is 0.0 in every dimension, the inverse normal of the first
/// Sobol point 0.5. The generator copies the Sobol generator it is built from,
/// as QuantLib does.
#[gen_stub_pyclass]
#[pyclass(
    name = "GaussianLowDiscrepancySequenceGenerator",
    unsendable,
    module = "itofin.randomnumbers"
)]
pub struct PyGaussianLowDiscrepancySequenceGenerator {
    inner: InverseCumulativeRsg<SobolSequence, InverseCumulativeNormal>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyGaussianLowDiscrepancySequenceGenerator {
    /// Build a Gaussian sequence generator over a copy of a Sobol generator.
    ///
    /// Args:
    ///     rsg (SobolRsg): The Sobol generator to copy the state from; its
    ///         dimension is the dimension here.
    #[new]
    fn new(rsg: &PySobolRsg) -> Self {
        PyGaussianLowDiscrepancySequenceGenerator {
            inner: InverseCumulativeRsg::new(
                SobolSequence::new(rsg.inner()),
                InverseCumulativeNormal::standard(),
            ),
        }
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
    ///     numpy.ndarray: A float64 array of shape (dimension,) of standard
    ///     normal deviates.
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
