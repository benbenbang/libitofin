//! Single-factor path generator.
//!
//! Port of `ql/methods/montecarlo/pathgenerator.hpp`: evolves a
//! [`StochasticProcess1D`] over a [`TimeGrid`], drawing the Gaussian increments
//! from a [`SequenceGenerator`]. The initial value sits at `path[0]`; each
//! later point is `path[i] = process.evolve(t_{i-1}, path[i-1], dt_{i-1},
//! dw_{i-1})` (`pathgenerator.hpp:145-151`).
//!
//! Divergences from `pathgenerator.hpp`, all deliberate:
//! - **process type**: C++ takes `shared_ptr<StochasticProcess>` and
//!   `dynamic_pointer_cast`s down to `StochasticProcess1D`, silently storing
//!   null on failure (`pathgenerator.hpp:88`). Here the constructor takes a
//!   `Shared<dyn StochasticProcess1D>` directly, so the 1-D requirement is a
//!   compile-time guarantee rather than a runtime null.
//! - **`next` takes `&mut self` and returns by value**: C++ mutates a cached
//!   `next_` member through a `const` method and returns a reference
//!   (`pathgenerator.hpp:60,142`). Rust's [`SequenceGenerator::next_sequence`]
//!   needs `&mut self`, and returning an owned `Sample<Path>` per call keeps
//!   the borrow simple; the consumer (`#451`'s model loop) takes it by value.
//! - **`temp_` dropped**: C++ stages the draws in a `temp_` member so the
//!   Brownian bridge can transform them in place (`pathgenerator.hpp:73,131`).
//!   With the bridge deferred, the direct-copy branch reads the draws straight
//!   from the sequence into a local, so no staging buffer is kept.
//! - **`next` is fallible**: `evolve` returns `QlResult` on main
//!   (`stochasticprocess.rs:81`); a mid-path `Err` is a setup/data error, not a
//!   per-sample outcome, so it aborts the whole call.
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **Brownian bridge** (`pathgenerator.hpp:130-133`): the constructor accepts
//!   the `brownian_bridge` flag but returns `Err` when it is `true`; only the
//!   `std::copy` branch (`pathgenerator.hpp:134-138`) is ported.
//! - **antithetic sampling** (`pathgenerator.hpp:117,127`): `antithetic()` and
//!   the negated-draw path are not ported; only `next()` is.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::randomnumbers::rngtraits::SequenceGenerator;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{Path, Sample};
use crate::require;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Size, Time};

/// Generates random single-factor paths from a Gaussian sequence generator.
pub struct PathGenerator<GSG> {
    generator: GSG,
    dimension: Size,
    time_grid: TimeGrid,
    process: Shared<dyn StochasticProcess1D>,
}

impl<GSG: SequenceGenerator> PathGenerator<GSG> {
    /// A generator over a regular grid of `time_steps` steps on `[0, length]`
    /// (`pathgenerator.hpp:80`).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `brownian_bridge` is `true` (deferred), if the grid is
    /// degenerate (see [`TimeGrid::new`]), or if the generator's dimensionality
    /// does not equal `time_steps` (`pathgenerator.hpp:90`).
    pub fn new(
        process: Shared<dyn StochasticProcess1D>,
        length: Time,
        time_steps: Size,
        generator: GSG,
        brownian_bridge: bool,
    ) -> QlResult<Self> {
        let time_grid = TimeGrid::new(length, time_steps)?;
        Self::assemble(process, time_grid, generator, brownian_bridge)
    }

    /// A generator over an explicit `time_grid` (`pathgenerator.hpp:95`).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `brownian_bridge` is `true` (deferred) or if the
    /// generator's dimensionality does not equal `time_grid.size() - 1`
    /// (`pathgenerator.hpp:104`).
    pub fn from_time_grid(
        process: Shared<dyn StochasticProcess1D>,
        time_grid: TimeGrid,
        generator: GSG,
        brownian_bridge: bool,
    ) -> QlResult<Self> {
        Self::assemble(process, time_grid, generator, brownian_bridge)
    }

    fn assemble(
        process: Shared<dyn StochasticProcess1D>,
        time_grid: TimeGrid,
        generator: GSG,
        brownian_bridge: bool,
    ) -> QlResult<Self> {
        require!(
            !brownian_bridge,
            "brownian bridge path generation is not yet ported; only the \
             direct-copy variant is available"
        );
        let dimension = generator.dimension();
        let time_steps = time_grid.size() - 1;
        require!(
            dimension == time_steps,
            "sequence generator dimensionality ({dimension}) != timeSteps ({time_steps})"
        );
        Ok(PathGenerator {
            generator,
            dimension,
            time_grid,
            process,
        })
    }

    /// The sequence-generator dimensionality (`pathgenerator.hpp:62`).
    pub fn size(&self) -> Size {
        self.dimension
    }

    /// The time grid the paths are sampled on (`pathgenerator.hpp:63`).
    pub fn time_grid(&self) -> &TimeGrid {
        &self.time_grid
    }

    /// Draws the next path (`pathgenerator.hpp:121-154`, the `brownian_bridge
    /// == false` branch).
    ///
    /// # Errors
    ///
    /// Propagates any `evolve`/`x0` error from the process, aborting the path.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> QlResult<Sample<Path>> {
        let (weight, draws) = {
            let sequence = self.generator.next_sequence();
            (sequence.weight, sequence.value.clone())
        };

        let mut path = Path::new(self.time_grid.clone(), Array::new())?;
        *path.front_mut() = self.process.x0()?;

        for i in 1..path.length() {
            let t = self.time_grid[i - 1];
            let dt = self.time_grid.dt(i - 1);
            path[i] = self.process.evolve(t, path[i - 1], dt, draws[i - 1])?;
        }

        Ok(Sample::new(path, weight))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::{Handle, RelinkableHandle};
    use crate::interestrate::Compounding;
    use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom};
    use crate::processes::BlackScholesMertonProcess;
    use crate::quotes::make_quote_handle;
    use crate::shared::shared;
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Real, Volatility};

    const SPOT: Real = 100.0;
    const R: Rate = 0.05;
    const Q: Rate = 0.02;
    const VOL: Volatility = 0.20;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn gbs_process() -> Shared<dyn StochasticProcess1D> {
        let spot = make_quote_handle(SPOT);
        let vol = RelinkableHandle::new(shared(BlackConstantVol::new(
            reference(),
            Some(Target::new()),
            VOL,
            Actual360::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        shared(BlackScholesMertonProcess::new(
            spot.handle(),
            flat_yield(Q),
            flat_yield(R),
            vol.handle(),
        )) as Shared<dyn StochasticProcess1D>
    }

    fn generator(dimension: usize, seed: u32) -> <PseudoRandom as McRngTraits>::RsgType {
        PseudoRandom::make_sequence_generator(dimension, seed).unwrap()
    }

    #[test]
    fn path_invariants_hold() {
        let mut pg = PathGenerator::new(gbs_process(), 1.0, 12, generator(12, 42), false).unwrap();
        assert_eq!(pg.size(), 12);
        let grid = pg.time_grid().clone();
        let sample = pg.next().unwrap();
        let path = &sample.value;

        assert_eq!(path.length(), grid.size());
        assert_eq!(path[0], SPOT);
        assert_eq!(path.front(), SPOT);
        assert_eq!(path[0], path.front());
        for i in 0..path.length() {
            assert_eq!(path.time(i), grid[i]);
        }
    }

    #[test]
    fn same_seed_produces_identical_paths() {
        let mut a = PathGenerator::new(gbs_process(), 1.0, 12, generator(12, 42), false).unwrap();
        let mut b = PathGenerator::new(gbs_process(), 1.0, 12, generator(12, 42), false).unwrap();
        let pa = a.next().unwrap();
        let pb = b.next().unwrap();
        assert_eq!(pa.value.values(), pb.value.values());
    }

    #[test]
    fn dimension_mismatch_is_rejected() {
        // pathgenerator.hpp:90: dimensionality must equal timeSteps.
        match PathGenerator::new(gbs_process(), 1.0, 12, generator(10, 42), false) {
            Err(e) => assert!(e.message().contains("!= timeSteps")),
            Ok(_) => panic!("dimensionality 10 != 12 timeSteps must be rejected"),
        }

        let grid = TimeGrid::new(1.0, 5).unwrap();
        assert!(
            PathGenerator::from_time_grid(gbs_process(), grid, generator(10, 42), false).is_err()
        );
    }

    #[test]
    fn brownian_bridge_is_rejected_as_deferred() {
        match PathGenerator::new(gbs_process(), 1.0, 12, generator(12, 42), true) {
            Err(e) => assert!(e.message().contains("brownian bridge")),
            Ok(_) => panic!("brownian_bridge = true must be rejected as deferred"),
        }
    }
}
