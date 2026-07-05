//! Pseudo-random number generators ported from `ql/math/randomnumbers/`.
//!
//! QuantLib couples its generators through class templates whose `next()`
//! returns a weighted `Sample<Real>`; for the pseudo-random generators the
//! weight is always `1.0`, so we drop the wrapper and express the coupling
//! with traits instead: [`UniformRng`] for uniform deviates in `(0, 1)`,
//! [`Uint64Rng`] for the generators that also expose their raw 64-bit output
//! (what the Ziggurat transform consumes), and [`GaussianRng`] for normal
//! deviates. Generators take `&mut self` where QuantLib mutates through
//! `const`; the sequences are identical.
//!
//! Low-discrepancy (Sobol and friends) generation is a separate port.

use crate::types::Real;

pub mod knuthuniformrng;
pub mod mt19937uniformrng;
pub mod ranluxuniformrng;
pub mod seedgenerator;

pub use knuthuniformrng::KnuthUniformRng;
pub use mt19937uniformrng::MersenneTwisterUniformRng;
pub use ranluxuniformrng::{Ranlux3UniformRng, Ranlux4UniformRng, Ranlux64UniformRng};

/// A generator of uniform pseudo-random deviates in the open `(0, 1)`
/// interval.
pub trait UniformRng {
    /// The next uniform deviate in `(0, 1)`.
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
