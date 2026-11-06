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
//! - **`temp_` is a per-call local**: C++ stages the draws in a `temp_`
//!   member (`pathgenerator.hpp:73,130-138`); here they are staged in a local
//!   buffer, bridged or copied, so `next` keeps no mutable scratch state.
//! - **the bridge is built only when requested**: C++ always constructs `bb_`
//!   from the grid (`pathgenerator.hpp:85,99`); here it is built only when
//!   `brownian_bridge` is `true`, since the copy branch never reads it.
//! - **`next` is fallible**: `evolve` returns `QlResult` on main
//!   (`stochasticprocess.rs:81`); a mid-path `Err` is a setup/data error, not a
//!   per-sample outcome, so it aborts the whole call.
//! - **`antithetic()` before any `next()` is an error**: C++ reads whatever
//!   `generator_.lastSequence()` holds (`pathgenerator.hpp:127`), which on an
//!   undrawn `InverseCumulativeRsg` is a zero vector
//!   (`inversecumulativersg.rs:34`) and so a silent zero-noise path. Per D4 the
//!   undrawn case returns `Err` instead.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::randomnumbers::rngtraits::SequenceGenerator;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{BrownianBridge, Path, PathGen, Sample};
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
    bridge: Option<BrownianBridge>,
    drawn: bool,
}

impl<GSG: SequenceGenerator> PathGenerator<GSG> {
    /// A generator over a regular grid of `time_steps` steps on `[0, length]`
    /// (`pathgenerator.hpp:80`).
    ///
    /// # Errors
    ///
    /// Returns `Err` if the grid is degenerate (see [`TimeGrid::new`]), if the
    /// generator's dimensionality does not equal `time_steps`
    /// (`pathgenerator.hpp:90`), or if `brownian_bridge` is `true` on a grid the
    /// bridge rejects (see [`BrownianBridge::from_time_grid`]).
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
    /// Returns `Err` if the generator's dimensionality does not equal
    /// `time_grid.size() - 1` (`pathgenerator.hpp:104`), or if
    /// `brownian_bridge` is `true` on a grid the bridge rejects (see
    /// [`BrownianBridge::from_time_grid`]).
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
        let dimension = generator.dimension();
        let time_steps = time_grid.size() - 1;
        require!(
            dimension == time_steps,
            "sequence generator dimensionality ({dimension}) != timeSteps ({time_steps})"
        );
        let bridge = if brownian_bridge {
            Some(BrownianBridge::from_time_grid(&time_grid)?)
        } else {
            None
        };
        Ok(PathGenerator {
            generator,
            dimension,
            time_grid,
            process,
            bridge,
            drawn: false,
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

    /// Draws the next forward path (`pathgenerator.hpp:112`).
    ///
    /// # Errors
    ///
    /// Propagates any `evolve`/`x0` error from the process, aborting the path.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> QlResult<Sample<Path>> {
        self.generate(false)
    }

    /// Draws the antithetic partner of the last forward path: the SAME draws,
    /// negated, under the same weight (`pathgenerator.hpp:118,127,140,149`).
    ///
    /// # Errors
    ///
    /// Returns `Err` when no forward path has been drawn yet, since there is no
    /// last sequence to negate; propagates any `evolve`/`x0` error from the
    /// process.
    pub fn antithetic(&mut self) -> QlResult<Sample<Path>> {
        self.generate(true)
    }

    /// The `next(bool)` core (`pathgenerator.hpp:121-154`): the drawn (or, for
    /// the antithetic partner, the cached last) sequence is bridged or copied
    /// into the increments (`:130-138`), which the antithetic path negates
    /// step by step (`:146-148`).
    fn generate(&mut self, antithetic: bool) -> QlResult<Sample<Path>> {
        require!(
            !antithetic || self.drawn,
            "no path drawn yet: the antithetic partner negates the last sequence"
        );
        let (weight, draws) = {
            let sequence = if antithetic {
                self.generator.last_sequence()
            } else {
                self.generator.next_sequence()
            };
            let mut draws = sequence.value.clone();
            if let Some(bridge) = &self.bridge {
                bridge.transform(&sequence.value, &mut draws)?;
            }
            (sequence.weight, draws)
        };
        self.drawn = true;

        let mut path = Path::new(self.time_grid.clone(), Array::new())?;
        *path.front_mut() = self.process.x0()?;

        for i in 1..path.length() {
            let t = self.time_grid[i - 1];
            let dt = self.time_grid.dt(i - 1);
            let dw = if antithetic {
                -draws[i - 1]
            } else {
                draws[i - 1]
            };
            path[i] = self.process.evolve(t, path[i - 1], dt, dw)?;
        }

        Ok(Sample::new(path, weight))
    }
}

impl<GSG: SequenceGenerator> PathGen for PathGenerator<GSG> {
    type PathType = Path;

    fn next(&mut self) -> QlResult<Sample<Path>> {
        self.generate(false)
    }

    fn antithetic(&mut self) -> QlResult<Sample<Path>> {
        self.generate(true)
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
    use crate::math::distributions::normal::InverseCumulativeNormal;
    use crate::math::randomnumbers::rngtraits::{McRngTraits, PseudoRandom};
    use crate::math::randomnumbers::sobol::{DirectionIntegers, SobolRsg};
    use crate::math::randomnumbers::{InverseCumulativeRsg, SobolSequence};
    use crate::methods::montecarlo::brownianbridge::tests::Moments;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::processes::{BlackScholesMertonProcess, GeometricBrownianMotionProcess};
    use crate::quotes::make_quote_handle;
    use crate::shared::shared;
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
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

    /// `test-suite/pathgenerator.cpp:39-85` `testSingle`: seed 42, 12 steps
    /// over 10 years, the terminal value of the 101st path and of its
    /// antithetic partner. The grid times are explicit, so the fixture's
    /// reference date does not enter.
    fn test_single(
        process: Shared<dyn StochasticProcess1D>,
        brownian_bridge: bool,
        expected: Real,
        antithetic: Real,
    ) {
        let mut pg =
            PathGenerator::new(process, 10.0, 12, generator(12, 42), brownian_bridge).unwrap();
        for _ in 0..100 {
            pg.next().unwrap();
        }
        let calculated = pg.next().unwrap().value.back();
        assert!(
            (calculated - expected).abs() <= 2.0e-8,
            "bridge={brownian_bridge}: {calculated} vs {expected}"
        );
        let calculated = pg.antithetic().unwrap().value.back();
        assert!(
            (calculated - antithetic).abs() <= 2.0e-7,
            "bridge={brownian_bridge} antithetic: {calculated} vs {antithetic}"
        );
    }

    /// `pathgenerator.cpp:155-157`.
    #[test]
    fn black_scholes_cached_values_without_bridge() {
        test_single(gbs_process(), false, 26.13784357783, 467.2928561411);
    }

    /// `pathgenerator.cpp:158-160`.
    #[test]
    fn black_scholes_cached_values_with_bridge() {
        test_single(gbs_process(), true, 60.28215549393, 202.6143139999);
    }

    /// `pathgenerator.cpp:164-166`.
    #[test]
    fn geometric_brownian_cached_values_without_bridge() {
        let process = shared(GeometricBrownianMotionProcess::new(100.0, 0.03, 0.20).unwrap())
            as Shared<dyn StochasticProcess1D>;
        test_single(process, false, 27.62223714065, 483.6026514084);
    }

    /// `test-suite/brownianbridge.cpp:166` `testPathGeneration`: bridged and
    /// unbridged Sobol paths share their first two moments. The two generators
    /// are built identically, as C++ copies one `gsg` into both, and the
    /// moments are accumulated locally for the reason given on
    /// `brownianbridge.rs`'s `variates_match_brownian_moments`. Equal moments
    /// alone would also hold if the bridge were skipped, so the second path
    /// pair (the first Sobol point is all-zero noise) is required to differ.
    #[test]
    fn bridged_paths_match_unbridged_moments() {
        let times = [
            0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0, 2.0, 5.0, 7.0, 9.0, 10.0,
        ];
        let n = times.len();
        let samples = 131_071;
        let today = reference();
        let flat = |rate: Rate| {
            Handle::new(shared(FlatForward::with_rate(
                today,
                rate,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let vol = RelinkableHandle::new(shared(BlackConstantVol::new(
            today,
            Some(NullCalendar::new()),
            0.20,
            Actual365Fixed::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        let process = shared(BlackScholesMertonProcess::new(
            make_quote_handle(100.0).handle(),
            flat(0.03),
            flat(0.06),
            vol.handle(),
        )) as Shared<dyn StochasticProcess1D>;
        let grid = TimeGrid::from_mandatory_times(&times).unwrap();
        let sobol = || {
            InverseCumulativeRsg::new(
                SobolSequence::new(SobolRsg::new(n, 42, DirectionIntegers::Jaeckel)),
                InverseCumulativeNormal::standard(),
            )
        };
        let mut generator1 =
            PathGenerator::from_time_grid(Shared::clone(&process), grid.clone(), sobol(), false)
                .unwrap();
        let mut generator2 = PathGenerator::from_time_grid(process, grid, sobol(), true).unwrap();

        let mut stats1 = Moments::new(n);
        let mut stats2 = Moments::new(n);
        for i in 0..samples {
            let path1 = generator1.next().unwrap().value;
            let path2 = generator2.next().unwrap().value;
            if i == 1 {
                assert_ne!(path1.values(), path2.values(), "the bridge must act");
            }
            stats1.add(&path1.values()[1..]);
            stats2.add(&path2.values()[1..]);
        }

        let max_rel_diff = |a: &[Real], b: &[Real]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| ((x - y) / y).abs())
                .fold(0.0, Real::max)
        };
        let mean_error = max_rel_diff(&stats2.mean(), &stats1.mean());
        let cov_error = max_rel_diff(&stats2.covariance(), &stats1.covariance());
        assert!(mean_error <= 3.0e-5, "mean error {mean_error}");
        assert!(cov_error <= 3.0e-3, "covariance error {cov_error}");
    }

    /// A generator whose draws ADVANCE on every `next_sequence`, so reusing the
    /// cached sequence and drawing a fresh one give different numbers. A stub
    /// returning one constant vector cannot tell the two apart.
    struct AdvancingSequence {
        sample: Sample<Vec<Real>>,
        drawn: usize,
    }

    impl AdvancingSequence {
        fn new(dimension: usize) -> Self {
            AdvancingSequence {
                sample: Sample::new(vec![0.0; dimension], 0.0),
                drawn: 0,
            }
        }
    }

    impl SequenceGenerator for AdvancingSequence {
        fn next_sequence(&mut self) -> &Sample<Vec<Real>> {
            self.drawn += 1;
            let base = 100.0 * self.drawn as Real;
            for (i, v) in self.sample.value.iter_mut().enumerate() {
                *v = base + (i + 1) as Real;
            }
            self.sample.weight = self.drawn as Real;
            &self.sample
        }

        fn last_sequence(&self) -> &Sample<Vec<Real>> {
            &self.sample
        }

        fn dimension(&self) -> usize {
            self.sample.value.len()
        }
    }

    /// `evolve` is the running sum of the draws, so a path reads back the exact
    /// increments it was built from.
    struct DrawSumProcess {
        observable: Observable,
    }

    impl AsObservable for DrawSumProcess {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl StochasticProcess1D for DrawSumProcess {
        fn x0(&self) -> QlResult<Real> {
            Ok(0.0)
        }

        fn drift(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(0.0)
        }

        fn diffusion(&self, _t: Time, _x: Real) -> QlResult<Real> {
            Ok(1.0)
        }

        fn evolve(&self, _t0: Time, x0: Real, _dt: Time, dw: Real) -> QlResult<Real> {
            Ok(x0 + dw)
        }
    }

    fn draw_sum_generator(steps: usize) -> PathGenerator<AdvancingSequence> {
        let process = shared(DrawSumProcess {
            observable: Observable::new(),
        }) as Shared<dyn StochasticProcess1D>;
        PathGenerator::new(process, 1.0, steps, AdvancingSequence::new(steps), false).unwrap()
    }

    /// `pathgenerator.hpp:127,140,149`: the antithetic draw reuses the CACHED
    /// last sequence negated, under that sequence's weight, and does NOT consume
    /// a fresh draw. With draws advancing by 100 per call, a fresh-draw
    /// implementation would put the second block's numbers in the first
    /// antithetic path, so the increments separate the two.
    #[test]
    fn antithetic_negates_the_last_forward_draws() {
        let mut pg = draw_sum_generator(3);

        let forward = pg.next().unwrap();
        assert_eq!(
            forward.value.values().to_vec(),
            vec![0.0, 101.0, 203.0, 306.0]
        );
        assert_eq!(forward.weight, 1.0);

        let anti = PathGen::antithetic(&mut pg).unwrap();
        assert_eq!(
            anti.value.values().to_vec(),
            vec![0.0, -101.0, -203.0, -306.0]
        );
        assert_eq!(anti.weight, 1.0, "the cached sequence carries the weight");

        let second = pg.next().unwrap();
        assert_eq!(
            second.value.values().to_vec(),
            vec![0.0, 201.0, 403.0, 606.0]
        );
        let second_anti = PathGen::antithetic(&mut pg).unwrap();
        assert_eq!(
            second_anti.value.values().to_vec(),
            vec![0.0, -201.0, -403.0, -606.0]
        );
        assert_eq!(second_anti.weight, 2.0);
    }

    /// The D4 divergence: with no cached sequence to negate, the undrawn
    /// generator errors rather than evolving the zero vector C++ would read.
    #[test]
    fn antithetic_before_any_forward_draw_is_rejected() {
        let mut pg = draw_sum_generator(3);
        match PathGen::antithetic(&mut pg) {
            Err(e) => assert!(e.message().contains("no path drawn yet")),
            Ok(_) => panic!("antithetic before the first draw must be rejected"),
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
