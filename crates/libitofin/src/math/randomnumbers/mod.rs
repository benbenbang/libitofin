//! Pseudo-random number generators ported from `ql/math/randomnumbers/`.
//!
//! QuantLib couples its generators through class templates whose `next()`
//! returns a weighted `Sample<Real>`; for the pseudo-random generators the
//! weight is always `1.0`, so we drop the wrapper and express the coupling
//! with traits instead: [`UniformRng`] for uniform deviates in the unit
//! interval,
//! [`Uint64Rng`] for the generators that also expose their raw 64-bit output
//! (what the Ziggurat transform consumes), and [`GaussianRng`] for normal
//! deviates. Generators take `&mut self` where QuantLib mutates through
//! `const`; the sequences are identical.
//!
//! Low-discrepancy Sobol generation lives in [`sobol`].

use crate::types::Real;

pub mod boxmullergaussianrng;
pub mod inversecumulativersg;
pub mod knuthuniformrng;
pub mod lattice;
mod lattice_tables;
pub mod mt19937uniformrng;
pub mod randomsequencegenerator;
pub mod ranluxuniformrng;
pub mod rngtraits;
pub mod seedgenerator;
pub mod sobol;
pub mod xoshiro256starstaruniformrng;
pub mod zigguratgaussianrng;

pub use boxmullergaussianrng::BoxMullerGaussianRng;
pub use inversecumulativersg::InverseCumulativeRsg;
pub use knuthuniformrng::KnuthUniformRng;
pub use mt19937uniformrng::MersenneTwisterUniformRng;
pub use randomsequencegenerator::RandomSequenceGenerator;
pub use ranluxuniformrng::{Ranlux3UniformRng, Ranlux4UniformRng, Ranlux64UniformRng};
pub use rngtraits::{InverseCumulative, McRngTraits, PseudoRandom, SequenceGenerator};
pub use xoshiro256starstaruniformrng::Xoshiro256StarStarUniformRng;
pub use zigguratgaussianrng::ZigguratGaussianRng;

/// A generator of uniform pseudo-random deviates in the unit interval.
///
/// Endpoint behavior is generator-specific (e.g. Ranlux can return exactly
/// `0.0`); see each implementation's documentation.
pub trait UniformRng {
    /// The next uniform deviate in the unit interval.
    fn next_real(&mut self) -> Real;
}

/// A uniform generator that natively produces 64-bit integers, the interface
/// required by the Ziggurat Gaussian transform.
pub trait Uint64Rng: UniformRng {
    /// The next raw output, uniform over `[0, u64::MAX]`.
    fn next_u64(&mut self) -> u64;
}

/// A generator of standard normal (mean `0`, standard deviation `1`)
/// pseudo-random deviates.
pub trait GaussianRng {
    /// The next standard normal deviate.
    fn next_gaussian(&mut self) -> Real;
}
