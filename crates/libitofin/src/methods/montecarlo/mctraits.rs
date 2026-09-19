//! Monte Carlo policy traits: single- versus multi-variate simulation.
//!
//! Port of `ql/methods/montecarlo/mctraits.hpp`: the `MC` template argument of
//! `McSimulation` / `MCVanillaEngine` that fixes, in one place, the process
//! trait the engine stores, the path generator it builds, and the path type its
//! pricer sees. [`SingleVariate`] is `mctraits.hpp:39-46` (`Path` over a
//! `PathGenerator`), [`MultiVariate`] is `mctraits.hpp:50-57` (`MultiPath` over
//! a `MultiPathGenerator`).
//!
//! Divergence from `mctraits.hpp`, deliberate: C++ stores the process as the
//! multi-factor `StochasticProcess` base in both policies because its
//! `StochasticProcess1D` IS-A `StochasticProcess`; this crate keeps the two
//! process traits as siblings (`stochasticprocess.rs`), so the policy also
//! names the process handle and the four things the engine base reads off it
//! (observable, `time`, `factors`, generator construction). The `rng_traits`
//! member is not carried: the RNG policy stays a separate generic on the engine
//! (`RNG` in `McVanillaEngineBase<RNG, MC>`), and the generator is a generic
//! associated type over the RNG's sequence generator.

use crate::errors::QlResult;
use crate::math::randomnumbers::rngtraits::SequenceGenerator;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{MultiPath, MultiPathGenerator, Path, PathGen, PathGenerator};
use crate::patterns::observable::Observable;
use crate::shared::Shared;
use crate::stochasticprocess::{StochasticProcess, StochasticProcess1D};
use crate::time::date::Date;
use crate::types::{Size, Time};

/// The Monte Carlo policy (`mctraits.hpp:39,50`): which process an engine
/// holds and which generator / path type it simulates with.
pub trait McTraits {
    /// The shared process handle the engine stores.
    type Process: Clone;

    /// The realized path type (`mctraits.hpp:41,52`).
    type PathType;

    /// The path generator over the RNG policy's sequence generator
    /// (`mctraits.hpp:44,55`).
    type Generator<RSG: SequenceGenerator>: PathGen<PathType = Self::PathType>;

    /// The process observable, for the engine to register with.
    fn observable(process: &Self::Process) -> &Observable;

    /// The process time of `date` (`mcvanillaengine.hpp:155`).
    ///
    /// # Errors
    ///
    /// Propagates the process's date/time conversion failure.
    fn time(process: &Self::Process, date: &Date) -> QlResult<Time>;

    /// The number of independent factors the generator draws per step
    /// (`mcvanillaengine.hpp:74`).
    fn factors(process: &Self::Process) -> Size;

    /// Builds the policy's path generator over `grid`
    /// (`mcvanillaengine.hpp:78-80`).
    ///
    /// # Errors
    ///
    /// Propagates the generator constructor's validation failure.
    fn path_generator<RSG: SequenceGenerator>(
        process: Self::Process,
        grid: TimeGrid,
        generator: RSG,
        brownian_bridge: bool,
    ) -> QlResult<Self::Generator<RSG>>;
}

/// Single-factor policy (`mctraits.hpp:39-46`): a [`StochasticProcess1D`]
/// simulated by a [`PathGenerator`] into a [`Path`].
pub struct SingleVariate;

impl McTraits for SingleVariate {
    type Process = Shared<dyn StochasticProcess1D>;
    type PathType = Path;
    type Generator<RSG: SequenceGenerator> = PathGenerator<RSG>;

    fn observable(process: &Self::Process) -> &Observable {
        process.observable()
    }

    fn time(process: &Self::Process, date: &Date) -> QlResult<Time> {
        process.time(date)
    }

    fn factors(_process: &Self::Process) -> Size {
        1
    }

    fn path_generator<RSG: SequenceGenerator>(
        process: Self::Process,
        grid: TimeGrid,
        generator: RSG,
        brownian_bridge: bool,
    ) -> QlResult<Self::Generator<RSG>> {
        PathGenerator::from_time_grid(process, grid, generator, brownian_bridge)
    }
}

/// Multi-factor policy (`mctraits.hpp:50-57`): a [`StochasticProcess`]
/// simulated by a [`MultiPathGenerator`] into a [`MultiPath`].
pub struct MultiVariate;

impl McTraits for MultiVariate {
    type Process = Shared<dyn StochasticProcess>;
    type PathType = MultiPath;
    type Generator<RSG: SequenceGenerator> = MultiPathGenerator<RSG>;

    fn observable(process: &Self::Process) -> &Observable {
        process.observable()
    }

    fn time(process: &Self::Process, date: &Date) -> QlResult<Time> {
        process.time(date)
    }

    fn factors(process: &Self::Process) -> Size {
        process.factors()
    }

    fn path_generator<RSG: SequenceGenerator>(
        process: Self::Process,
        grid: TimeGrid,
        generator: RSG,
        brownian_bridge: bool,
    ) -> QlResult<Self::Generator<RSG>> {
        MultiPathGenerator::new(process, grid, generator, brownian_bridge)
    }
}
