//! Path-generator abstraction over the sampled path type.
//!
//! Ports the `path_generator_type` role of `ql/methods/montecarlo/mctraits.hpp`:
//! `SingleVariate` binds it to `PathGenerator<rsg_type>` over [`Path`]
//! (`mctraits.hpp:44`), `MultiVariate` to `MultiPathGenerator<rsg_type>` over
//! [`MultiPath`] (`mctraits.hpp:55`). This trait is the Rust seam that lets
//! [`MonteCarloModel`](super::MonteCarloModel) drive either without duplicating
//! the accumulation loop.
//!
//! [`Path`]: super::Path
//! [`MultiPath`]: super::MultiPath

use crate::errors::QlResult;
use crate::methods::montecarlo::Sample;
use crate::types::Size;

/// A generator of weighted sample paths (the `path_generator_type` policy,
/// `mctraits.hpp:44,55`).
pub trait PathGen {
    /// The realized path type: `Path` for single-factor, `MultiPath` for
    /// multi-factor (`mctraits.hpp:41,52`).
    type PathType;

    /// Draws the next forward path (`pathgenerator.hpp:142`,
    /// `multipathgenerator.hpp:92`).
    ///
    /// # Errors
    ///
    /// Propagates a process or sequence-generator failure.
    fn next(&mut self) -> QlResult<Sample<Self::PathType>>;

    /// Draws the antithetic partner of the last forward path: the same draws
    /// negated (`multipathgenerator.hpp:97`).
    ///
    /// # Errors
    ///
    /// Errors when the generator does not support antithetic sampling (the
    /// single-factor [`PathGenerator`](super::PathGenerator) deferral), or
    /// propagates a process or sequence-generator failure.
    fn antithetic(&mut self) -> QlResult<Sample<Self::PathType>>;

    /// The sequence-generator dimensionality (`pathgenerator.hpp:62`).
    fn dimension(&self) -> Size;
}
