//! Longstaff-Schwartz Monte Carlo driver for early-exercise options.
//!
//! Port of `ql/pricingengines/mclongstaffschwartzengine.hpp:178-210`: the
//! two-pass spine every least-squares Monte Carlo engine runs. A first pass
//! draws `n_calibration_samples` paths from a stream independent of the pricing
//! one and buffers them in the [`LongstaffSchwartzPathPricer`]; that pricer is
//! then calibrated; the second pass prices through the SAME pricer, now in its
//! pricing phase (`:179-199`).
//!
//! Divergences from `mclongstaffschwartzengine.hpp`, all deliberate:
//! - **composition over the vanilla base, not a mixin over `McSimulation`**:
//!   C++ derives from both `GenericEngine` and `McSimulation<MC,RNG,S>`
//!   (`:48-49`) and overrides `timeGrid()`/`pathGenerator()`/`pathPricer()`.
//!   Rust has no multiple inheritance, so this holds a [`McVanillaEngineBase`],
//!   whose grid, generator, and result plumbing are exactly the American branch
//!   of those overrides; the `lsmPathPricer()` hook (`:88-89`) is the pricer
//!   argument of [`calculate_with`](McLongstaffSchwartzEngineBase::calculate_with).
//! - **the grid is the vanilla base's**: C++'s `timeGrid()` override
//!   (`:218-231`) has an American branch (the last exercise time) and a Bermudan
//!   one (every positive exercise time). [`McVanillaEngineBase::time_grid`] is
//!   the American branch verbatim, so the Bermudan grid is DEFERRED; the engine
//!   on top of this driver rejects any non-American exercise, closing the gap.
//! - **`Null` sentinels become [`Option`]** (D10): the unset
//!   `nCalibrationSamples`, `antitheticVariateCalibration`, and
//!   `seedCalibration` (`:141,145-147,148-149`) default to the same values.
//! - **the calibration seed offset is added in `u32`**: C++ computes `seed +
//!   1768237423L` in `BigNatural` (`:149`); this stack's RNG surface is seeded
//!   by `u32`, so the addition wraps. Only seeds above 2526729872 differ, and
//!   they select a stream just as arbitrary as the C++ one. The zero seed
//!   (`:148`) is kept as zero, which draws a fresh stream per pass rather than a
//!   fixed one (`mt19937uniformrng.rs:25-31`), exactly as C++ relies on.
//!
//! Deferred, omitted visibly rather than accepted and ignored:
//! - **`brownianBridgeCalibration`** (`:145`): deferred with the Brownian bridge
//!   itself (#453); the calibration generator is always built without one, as
//!   the American engine's `false` (`mcamericanengine.hpp:161`) asks.
//! - **control variate**: still rejected by
//!   [`McSimulation`](crate::methods::montecarlo::McSimulation).

use std::any::Any;

use crate::errors::QlResult;
use crate::instruments::OptionArguments;
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::math::statistics::GeneralStatistics;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{LongstaffSchwartzPathPricer, MonteCarloModel};
use crate::patterns::observable::Observable;
use crate::pricingengine::{Arguments, Results};
use crate::pricingengines::vanilla::McVanillaEngineBase;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Size};

/// The calibration-path count and stream offset of
/// `mclongstaffschwartzengine.hpp:141,149`.
const DEFAULT_CALIBRATION_SAMPLES: Size = 2048;
const CALIBRATION_SEED_OFFSET: u32 = 1_768_237_423;

/// Shared two-pass Longstaff-Schwartz plumbing, generic over the RNG policy
/// `RNG` (`RNG_Calibration` is fixed to it, as `MakeMCAmericanEngine` leaves it).
pub struct McLongstaffSchwartzEngineBase<RNG> {
    base: McVanillaEngineBase<RNG>,
    n_calibration_samples: Size,
    antithetic_variate_calibration: bool,
    seed_calibration: u32,
}

impl<RNG: McRngTraits> McLongstaffSchwartzEngineBase<RNG> {
    /// Builds the driver (`:118-155`), applying the three calibration defaults:
    /// 2048 samples (`:141`), the pricing antithetic flag (`:145-147`), and the
    /// offset pricing seed (`:148-149`).
    ///
    /// # Errors
    ///
    /// Propagates the [`McVanillaEngineBase::new`] time-step validation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: Shared<dyn StochasticProcess1D>,
        time_steps: Option<Size>,
        time_steps_per_year: Option<Size>,
        brownian_bridge: bool,
        antithetic_variate: bool,
        control_variate: bool,
        required_samples: Option<Size>,
        required_tolerance: Option<Real>,
        max_samples: Option<Size>,
        seed: u32,
        n_calibration_samples: Option<Size>,
        antithetic_variate_calibration: Option<bool>,
        seed_calibration: Option<u32>,
    ) -> QlResult<Self> {
        let base = McVanillaEngineBase::new(
            process,
            time_steps,
            time_steps_per_year,
            brownian_bridge,
            antithetic_variate,
            control_variate,
            required_samples,
            required_tolerance,
            max_samples,
            seed,
        )?;

        Ok(McLongstaffSchwartzEngineBase {
            base,
            n_calibration_samples: n_calibration_samples.unwrap_or(DEFAULT_CALIBRATION_SAMPLES),
            antithetic_variate_calibration: antithetic_variate_calibration
                .unwrap_or(antithetic_variate),
            seed_calibration: seed_calibration.unwrap_or_else(|| calibration_seed(seed)),
        })
    }

    /// The typed option arguments, for building the payoff-dependent pricer.
    pub fn arguments(&self) -> &OptionArguments {
        self.base.arguments()
    }

    /// The erased argument bundle the instrument fills in.
    pub fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    /// The last calculation's results.
    pub fn results(&self) -> &dyn Results {
        self.base.results()
    }

    /// Clears the results ahead of a calculation.
    pub fn reset(&mut self) {
        self.base.reset();
    }

    /// The engine observable.
    pub fn observable(&self) -> &Observable {
        self.base.observable()
    }

    /// The number of calibration paths the first pass draws (`:141`).
    pub fn n_calibration_samples(&self) -> Size {
        self.n_calibration_samples
    }

    /// Whether the calibration pass averages antithetic pairs (`:145-147`).
    pub fn antithetic_variate_calibration(&self) -> bool {
        self.antithetic_variate_calibration
    }

    /// The seed of the calibration stream (`:148-149`).
    pub fn seed_calibration(&self) -> u32 {
        self.seed_calibration
    }

    /// The simulation time grid (`:218-231`, American branch).
    ///
    /// # Errors
    ///
    /// Propagates a [`McVanillaEngineBase::time_grid`] failure.
    pub fn time_grid(&self) -> QlResult<TimeGrid> {
        self.base.time_grid()
    }

    /// Runs both passes through `pricer` and fills the results (`:179-210`):
    /// calibration paths, then [`LongstaffSchwartzPathPricer::calibrate`], then
    /// the pricing pass, whose mean and error estimate the base writes; the
    /// exercise probability lands in the additional results under
    /// `exerciseProbability` (`:205-206`). `pricer` is the C++ `lsmPathPricer()`
    /// hook, built fresh by the calling engine on every calculation so no stale
    /// calibration survives a reprice.
    ///
    /// # Errors
    ///
    /// Propagates a grid, generator, sampling, regression, or simulation
    /// failure.
    pub fn calculate_with(&mut self, pricer: Shared<LongstaffSchwartzPathPricer>) -> QlResult<()> {
        let generator = self.base.path_generator_with_seed(self.seed_calibration)?;
        let mut calibration_model = MonteCarloModel::new(
            generator,
            Shared::clone(&pricer),
            GeneralStatistics::default(),
            self.antithetic_variate_calibration,
        )?;
        calibration_model.add_samples(self.n_calibration_samples)?;
        pricer.calibrate()?;

        self.base.run(Shared::clone(&pricer))?;

        let probability = pricer.exercise_probability()?;
        self.base
            .results_mut()
            .instrument
            .additional_results
            .insert(
                "exerciseProbability".to_string(),
                shared(probability) as Shared<dyn Any>,
            );
        Ok(())
    }
}

/// The calibration seed of a pricing `seed` (`mclongstaffschwartzengine.hpp:149`):
/// zero stays zero so both passes draw fresh streams, anything else is offset so
/// the two streams differ.
fn calibration_seed(seed: u32) -> u32 {
    if seed == 0 {
        0
    } else {
        seed.wrapping_add(CALIBRATION_SEED_OFFSET)
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;
    use crate::exercise::{EuropeanExercise, Exercise};
    use crate::instrument::InstrumentResults;
    use crate::instruments::{OneAssetOptionResults, PlainVanillaPayoff, StrikedTypePayoff};
    use crate::math::randomnumbers::rngtraits::PseudoRandom;
    use crate::methods::montecarlo::{
        EarlyExercisePathPricer, LsmBasisSystem, Path, PolynomialType,
    };
    use crate::option::OptionType;
    use crate::pricingengines::vanilla::test_market::{Market, market, time_to_days, today};
    use crate::shared::shared;

    const STRIKE: Real = 100.0;

    fn flat_market() -> Market {
        let market = market();
        market.set(STRIKE, 0.0, 0.05, 0.20);
        market
    }

    /// The American-put shape of `mcamericanengine.cpp:31-72`, reduced to the
    /// unscaled spot so the driver test needs nothing from #762.
    struct AmericanPut;

    impl EarlyExercisePathPricer<Path> for AmericanPut {
        type State = Real;

        fn value(&self, path: &Path, t: Size) -> Real {
            (STRIKE - path[t]).max(0.0)
        }

        fn state(&self, path: &Path, t: Size) -> Real {
            path[t]
        }

        fn basis_system(&self) -> Vec<Box<dyn Fn(Real) -> Real>> {
            LsmBasisSystem::path_basis_system(2, PolynomialType::Monomial)
        }
    }

    fn driver(
        market: &Market,
        antithetic_variate: bool,
        antithetic_variate_calibration: Option<bool>,
        n_calibration_samples: Option<Size>,
        seed_calibration: Option<u32>,
    ) -> McLongstaffSchwartzEngineBase<PseudoRandom> {
        let mut engine = McLongstaffSchwartzEngineBase::new(
            Shared::clone(&market.process) as Shared<dyn StochasticProcess1D>,
            Some(4),
            None,
            false,
            antithetic_variate,
            false,
            Some(512),
            None,
            None,
            42,
            n_calibration_samples,
            antithetic_variate_calibration,
            seed_calibration,
        )
        .unwrap();

        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_mut::<OptionArguments>()
            .unwrap();
        args.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Put, STRIKE))
            as Shared<dyn StrikedTypePayoff>);
        args.exercise = Some(
            shared(EuropeanExercise::new(today() + time_to_days(1.0))) as Shared<dyn Exercise>
        );
        engine
    }

    fn lsm(
        market: &Market,
        engine: &McLongstaffSchwartzEngineBase<PseudoRandom>,
    ) -> Shared<LongstaffSchwartzPathPricer> {
        shared(
            LongstaffSchwartzPathPricer::new(
                &engine.time_grid().unwrap(),
                shared(AmericanPut) as Shared<dyn EarlyExercisePathPricer<Path, State = Real>>,
                &market.process.risk_free_rate(),
            )
            .unwrap(),
        )
    }

    fn instrument_results(
        engine: &McLongstaffSchwartzEngineBase<PseudoRandom>,
    ) -> &InstrumentResults {
        &(engine.results() as &dyn Any)
            .downcast_ref::<OneAssetOptionResults>()
            .unwrap()
            .instrument
    }

    /// The seed derivation of `:148-149`: zero stays zero, anything else is
    /// offset by the C++ literal.
    #[test]
    fn the_calibration_seed_offsets_every_nonzero_pricing_seed() {
        assert_eq!(calibration_seed(0), 0);
        assert_eq!(calibration_seed(42), 42 + 1_768_237_423);
        assert_eq!(calibration_seed(1), 1_768_237_424);
        let market = flat_market();
        assert_eq!(
            driver(&market, false, None, None, None).seed_calibration(),
            1_768_237_465
        );
    }

    /// The unset calibration parameters take the C++ defaults (`:141,145-147`),
    /// and an explicit value wins over both.
    #[test]
    fn the_calibration_defaults_follow_the_cpp_fallbacks() {
        let market = flat_market();
        let defaulted = driver(&market, true, None, None, None);
        assert_eq!(defaulted.n_calibration_samples(), 2048);
        assert!(
            defaulted.antithetic_variate_calibration(),
            "an unset calibration flag follows the pricing one"
        );

        let explicit = driver(&market, true, Some(false), Some(64), Some(7));
        assert_eq!(explicit.n_calibration_samples(), 64);
        assert!(!explicit.antithetic_variate_calibration());
        assert_eq!(explicit.seed_calibration(), 7);
    }

    /// The full two-pass run: a positive value proves the pricer was calibrated
    /// BEFORE the pricing pass, since an uncalibrated pricer is still in its
    /// calibration phase and reports 0.0 for every path.
    #[test]
    fn both_passes_run_and_fill_the_results() {
        let market = flat_market();
        let mut engine = driver(&market, false, None, Some(256), None);
        let pricer = lsm(&market, &engine);
        engine.calculate_with(Shared::clone(&pricer)).unwrap();

        let results = instrument_results(&engine);
        let value = results.value.unwrap();
        assert!(value > 0.0, "a calibrated American put is worth something");
        assert!(results.error_estimate.unwrap() > 0.0);

        let probability = *results.additional_results["exerciseProbability"]
            .downcast_ref::<Real>()
            .unwrap();
        assert!((0.0..=1.0).contains(&probability));
        assert_eq!(probability, pricer.exercise_probability().unwrap());
    }

    /// Antithetic on the calibration pass buffers the negated partner of every
    /// calibration path (`:194`), so the pricer sees twice the requested paths
    /// and the run still produces a sane price.
    #[test]
    fn antithetic_calibration_doubles_the_buffered_paths() {
        let market = flat_market();
        let mut engine = driver(&market, false, Some(true), Some(16), None);
        let pricer = lsm(&market, &engine);
        engine.calculate_with(Shared::clone(&pricer)).unwrap();

        let results = instrument_results(&engine);
        assert!(results.value.unwrap() > 0.0);
        assert!((0.0..=1.0).contains(&pricer.exercise_probability().unwrap()));
    }
}
