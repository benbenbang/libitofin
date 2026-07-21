//! Random-number generation policies.
//!
//! Port of `ql/math/randomnumbers/rngtraits.hpp`. QuantLib expresses the
//! sequence-generator and inverse-cumulative concepts through C++ template
//! duck-typing; Rust needs explicit bounds, so this module defines the two
//! concept traits ([`SequenceGenerator`], [`InverseCumulative`]) that the RSG
//! layer is generic over, plus the [`PseudoRandom`] policy the Monte Carlo
//! engines consume through [`McRngTraits`].
//!
//! Divergences from `rngtraits.hpp`:
//! - the `static ext::shared_ptr<IC> icInstance` global override hook
//!   (`rngtraits.hpp:58`) is dropped (design decision D5, no global
//!   singletons); [`PseudoRandom::make_sequence_generator`] always
//!   default-constructs the inverse cumulative.
//! - `GenericLowDiscrepancy`/`LowDiscrepancy` (Sobol, `rngtraits.hpp:81`) is
//!   deferred: the pseudo-random path is all the European-option oracle needs.
//! - the scalar `rng_type` (`InverseCumulativeRng`, `rngtraits.hpp:46`) is
//!   deferred: the path generators consume only the sequence `rsg_type`.

use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::methods::montecarlo::Sample;
use crate::types::Real;

/// A generator of weighted uniform-or-transformed sequences.
///
/// The Rust bound behind QuantLib's implicit "USG"/"rsg" template concepts
/// (`randomsequencegenerator.hpp:37`, `inversecumulativersg.hpp:42`): a value
/// yielding `dimension` draws as a weighted [`Sample`]. Both
/// [`RandomSequenceGenerator`](super::randomsequencegenerator::RandomSequenceGenerator)
/// and [`InverseCumulativeRsg`](super::inversecumulativersg::InverseCumulativeRsg)
/// implement it.
///
/// `next_sequence` takes `&mut self`, where QuantLib mutates a cached sample
/// through a `const` method; the draw order and values are identical.
pub trait SequenceGenerator {
    /// Advances the generator and returns the freshly drawn sample.
    fn next_sequence(&mut self) -> &Sample<Vec<Real>>;

    /// Returns the most recently drawn sample without advancing.
    fn last_sequence(&self) -> &Sample<Vec<Real>>;

    /// The number of draws per sequence.
    fn dimension(&self) -> usize;
}

/// An inverse cumulative distribution used as a stateless deviate transform.
///
/// The Rust bound behind QuantLib's implicit "IC" template concept
/// (`inversecumulativersg.hpp:50`, `Real IC::operator()(Real)`): map a uniform
/// deviate in `(0, 1)` to the distribution's deviate.
pub trait InverseCumulative {
    /// The distribution deviate for the uniform `x` in `(0, 1)`.
    fn evaluate(&self, x: Real) -> Real;
}

impl InverseCumulative for InverseCumulativeNormal {
    /// # Panics
    ///
    /// The infallible transform boundary: callers of this trait
    /// ([`InverseCumulativeRsg`](super::inversecumulativersg::InverseCumulativeRsg))
    /// feed it uniform deviates that the sequence generator guarantees lie
    /// strictly in `(0, 1)`, where [`InverseCumulativeNormal::value`] is always
    /// finite, so the `expect` never fires. The public [`InverseCumulativeNormal`]
    /// API stays fallible; only this local precondition is asserted here.
    fn evaluate(&self, x: Real) -> Real {
        self.value(x)
            .expect("inverse cumulative normal is finite for a uniform deviate in (0, 1)")
    }
}
