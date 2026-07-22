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
use crate::methods::montecarlo::{Path, PathGen, Sample};
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Size, Time};
use crate::{fail, require};

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

impl<GSG: SequenceGenerator> PathGen for PathGenerator<GSG> {
    type PathType = Path;

    fn next(&mut self) -> QlResult<Sample<Path>> {
        PathGenerator::next(self)
    }

    /// Antithetic sampling is deferred for the single-factor generator
    /// (`pathgenerator.hpp:117,127`): only [`next`](PathGenerator::next) is
    /// ported, so this fails loudly rather than returning an unnegated draw.
    fn antithetic(&mut self) -> QlResult<Sample<Path>> {
        fail!(
            "antithetic single-factor path generation is not yet ported; only \
             the forward draw is available"
        )
    }

    fn dimension(&self) -> Size {
        self.dimension
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

    #[test]
    fn pathgen_antithetic_is_rejected_as_deferred() {
        // pathgenerator.hpp:117,127: single-factor antithetic is not ported;
        // the PathGen impl must fail loudly rather than reuse the forward draw.
        let mut pg = PathGenerator::new(gbs_process(), 1.0, 12, generator(12, 42), false).unwrap();
        match PathGen::antithetic(&mut pg) {
            Err(e) => assert!(e.message().contains("antithetic")),
            Ok(_) => panic!("single-factor antithetic must be rejected as deferred"),
        }
    }

    /// Terminal-moment pin (issue #450, gate #1): the batch's load-bearing
    /// oracle. For a `GeneralizedBlackScholesProcess` with flat continuous
    /// r = 5%, q = 2%, sigma = 20% over `[0, T = 1]`, the exact log scheme makes
    /// `ln(S_T / S_0)` normal with mean `(r - q - 0.5 sigma^2) T` and variance
    /// `sigma^2 T`. Both are step-count-invariant because `sum(dt_i) = T`, so
    /// running 12 steps (not one) exercises the accumulation loop for free. Over
    /// N fixed-seed paths the sample mean has standard error
    /// `se = sigma sqrt(T / N)` and the sample variance has
    /// `se_var = sqrt(2) sigma^2 T / sqrt(N - 1)` (the variance of a normal
    /// sample variance). The mean bound is 4 se, the variance bound 5 se_var;
    /// the confirm-by-stubbing edits below move the moments by tens of se, far
    /// outside either bound.
    #[test]
    fn terminal_moments_match_the_gbm_law() {
        const N: usize = 50_000;
        const STEPS: usize = 12;
        const T: Time = 1.0;

        let mut pg =
            PathGenerator::new(gbs_process(), T, STEPS, generator(STEPS, 42), false).unwrap();
        let mut logs = Vec::with_capacity(N);
        for _ in 0..N {
            let path = pg.next().unwrap().value;
            logs.push((path.back() / path.front()).ln());
        }

        let mean = logs.iter().sum::<Real>() / N as Real;
        let variance = logs.iter().map(|x| (x - mean).powi(2)).sum::<Real>() / (N as Real - 1.0);

        let mean_target = (R - Q - 0.5 * VOL * VOL) * T;
        let var_target = VOL * VOL * T;
        let se = VOL * (T / N as Real).sqrt();
        let se_var = 2.0_f64.sqrt() * var_target / (N as Real - 1.0).sqrt();

        assert!(
            (mean - mean_target).abs() < 4.0 * se,
            "mean {mean} vs target {mean_target}: {:.2} se (bound 4.0)",
            (mean - mean_target).abs() / se
        );
        assert!(
            (variance - var_target).abs() < 5.0 * se_var,
            "variance {variance} vs target {var_target}: {:.2} se_var (bound 5.0)",
            (variance - var_target).abs() / se_var
        );
    }
}
