//! Monte Carlo vanilla-option engine base.
//!
//! Port of `ql/pricingengines/vanilla/mcvanillaengine.hpp`: the shared plumbing
//! every Monte Carlo vanilla engine builds on. It selects the simulation
//! [`TimeGrid`] from the option's exercise date
//! (`mcvanillaengine.hpp:153`), builds the [`PathGenerator`] from the RNG policy
//! (`mcvanillaengine.hpp:72`), runs the [`McSimulation`], and writes the mean
//! (and, when the policy supports it, the error estimate) into the option
//! results (`mcvanillaengine.hpp:40`).
//!
//! Divergences from `mcvanillaengine.hpp`, all deliberate:
//! - **composition, not multiple inheritance**: C++ derives from both
//!   `Inst::engine` and `McSimulation` (`mcvanillaengine.hpp:37`). Rust has no
//!   MI, so [`McVanillaEngineBase`] *holds* an [`OneAssetOptionEngine`] and
//!   builds a fresh [`McSimulation`] per [`run`](McVanillaEngineBase::run). The
//!   payoff-dependent path pricer, C++'s pure-virtual `pathPricer()`, is passed
//!   into [`run`](McVanillaEngineBase::run) by the concrete engine (`#452`).
//! - **single-factor process**: C++ holds a multi-factor `StochasticProcess`
//!   and reads `process_->factors()` (`mcvanillaengine.hpp:74`). This stack's
//!   [`PathGenerator`] is single-factor, so the process is a
//!   `StochasticProcess1D` and `factors()` is fixed to 1; the path
//!   dimensionality is `grid.size() - 1`.
//! - **`Null` sentinels become [`Option`]**: the `timeSteps`,
//!   `timeStepsPerYear`, `requiredSamples`, `requiredTolerance`, and
//!   `maxSamples` sentinels (`mcvanillaengine.hpp:60-69`) are `Option` (D10).
//! - **statistics fixed to [`GeneralStatistics`]**: C++ is generic over `S`,
//!   defaulting to `Statistics` (`mcvanillaengine.hpp:36`); the one consumer
//!   (`#452`) uses that default, so `S` is fixed rather than a third generic.
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **antithetic / control variate**: the flags thread to [`McSimulation`],
//!   which rejects them as deferred; `controlVariateValue` and the control
//!   pricing engine (`mcvanillaengine.hpp:82,126`) are not ported.

use std::marker::PhantomData;

use crate::errors::QlResult;
use crate::instruments::{OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::math::statistics::MeanStdDev;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{McSimulation, Path, PathGen, PathGenerator, PathPricer};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, Results};
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Size};
use crate::{fail, require};

/// Shared Monte Carlo plumbing for vanilla-option engines, generic over the
/// RNG policy `RNG` (the C++ `RNG` template argument).
///
/// A concrete engine embeds one, delegates its
/// [`PricingEngine`](crate::pricingengine::PricingEngine) accessors to it, and
/// drives a calculation by building a path pricer and calling
/// [`run`](McVanillaEngineBase::run).
pub struct McVanillaEngineBase<RNG> {
    base: OneAssetOptionEngine,
    process: Shared<dyn StochasticProcess1D>,
    time_steps: Option<Size>,
    time_steps_per_year: Option<Size>,
    required_samples: Option<Size>,
    max_samples: Option<Size>,
    required_tolerance: Option<Real>,
    brownian_bridge: bool,
    antithetic_variate: bool,
    control_variate: bool,
    seed: u32,
    _rng: PhantomData<RNG>,
}

impl<RNG: McRngTraits> McVanillaEngineBase<RNG> {
    /// Builds the engine base (`mcvanillaengine.hpp:96`), registering with the
    /// process so its changes invalidate the attached instrument.
    ///
    /// # Errors
    ///
    /// Errors if neither `time_steps` nor `time_steps_per_year` is set, if both
    /// are set, or if either is `Some(0)` (`mcvanillaengine.hpp:111-122`).
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
    ) -> QlResult<Self> {
        require!(
            time_steps.is_some() || time_steps_per_year.is_some(),
            "no time steps provided"
        );
        require!(
            time_steps.is_none() || time_steps_per_year.is_none(),
            "both time steps and time steps per year were provided"
        );
        require!(
            time_steps != Some(0),
            "timeSteps must be positive, 0 not allowed"
        );
        require!(
            time_steps_per_year != Some(0),
            "timeStepsPerYear must be positive, 0 not allowed"
        );

        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(process.observable());

        Ok(McVanillaEngineBase {
            base,
            process,
            time_steps,
            time_steps_per_year,
            required_samples,
            max_samples,
            required_tolerance,
            brownian_bridge,
            antithetic_variate,
            control_variate,
            seed,
            _rng: PhantomData,
        })
    }

    /// The typed option arguments, for building the payoff-dependent pricer.
    pub fn arguments(&self) -> &OptionArguments {
        self.base.arguments()
    }

    /// The erased argument bundle the instrument fills in (delegation target for
    /// [`PricingEngine::arguments_mut`](crate::pricingengine::PricingEngine::arguments_mut)).
    pub fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    /// The last calculation's results (delegation target for
    /// [`PricingEngine::results`](crate::pricingengine::PricingEngine::results)).
    pub fn results(&self) -> &dyn Results {
        self.base.results()
    }

    /// Clears the results ahead of a calculation.
    pub fn reset(&mut self) {
        self.base.reset();
    }

    /// The engine observable (delegation target for
    /// [`AsObservable`](crate::patterns::observable::AsObservable)).
    pub fn observable(&self) -> &Observable {
        self.base.observable()
    }

    /// The simulation time grid, from the option's last exercise date
    /// (`mcvanillaengine.hpp:153`).
    ///
    /// # Errors
    ///
    /// Errors if no exercise is set, if the process cannot map the date to a
    /// time, or on a degenerate grid.
    pub fn time_grid(&self) -> QlResult<TimeGrid> {
        let Some(exercise) = &self.arguments().exercise else {
            fail!("no exercise given");
        };
        let t = self.process.time(&exercise.last_date())?;
        if let Some(steps) = self.time_steps {
            TimeGrid::new(t, steps)
        } else if let Some(per_year) = self.time_steps_per_year {
            let steps = (per_year as Real * t) as Size;
            TimeGrid::new(t, steps.max(1))
        } else {
            fail!("time steps not specified");
        }
    }

    /// The path generator, seeded from the RNG policy
    /// (`mcvanillaengine.hpp:72`).
    ///
    /// # Errors
    ///
    /// Propagates a [`time_grid`](McVanillaEngineBase::time_grid),
    /// sequence-generator, or [`PathGenerator`] failure.
    pub fn path_generator(&self) -> QlResult<PathGenerator<RNG::RsgType>> {
        let grid = self.time_grid()?;
        let dimension = grid.size() - 1;
        let generator = RNG::make_sequence_generator(dimension, self.seed)?;
        PathGenerator::from_time_grid(
            Shared::clone(&self.process),
            grid,
            generator,
            self.brownian_bridge,
        )
    }

    /// Runs the single-factor simulation: builds the [`PathGenerator`] from the
    /// RNG policy and prices each [`Path`] with `path_pricer`
    /// (`mcvanillaengine.hpp:40,72`).
    ///
    /// # Errors
    ///
    /// Propagates a generator, simulation, or accumulation failure.
    pub fn run<P: PathPricer<Path>>(&mut self, path_pricer: P) -> QlResult<()> {
        let generator = self.path_generator()?;
        self.run_with(generator, path_pricer)
    }

    /// Runs the simulation with a caller-supplied generator and pricer, then
    /// writes the mean (and, when `RNG::ALLOWS_ERROR_ESTIMATE`, the error
    /// estimate) into the results (`mcvanillaengine.hpp:40`).
    ///
    /// This is the generalization seam over the path type: [`run`](Self::run)
    /// drives it with the single-factor [`PathGenerator`], and a multi-factor
    /// engine drives it with a [`MultiPathGenerator`](crate::methods::montecarlo::MultiPathGenerator).
    /// The mean/error result plumbing is path-type-agnostic.
    ///
    /// # Errors
    ///
    /// Propagates a simulation or accumulation failure.
    pub fn run_with<PG, P>(&mut self, generator: PG, path_pricer: P) -> QlResult<()>
    where
        PG: PathGen,
        P: PathPricer<PG::PathType>,
    {
        let mut simulation =
            McSimulation::<PG, P>::new(self.antithetic_variate, self.control_variate);
        simulation.calculate(
            generator,
            path_pricer,
            self.required_tolerance,
            self.required_samples,
            self.max_samples,
        )?;

        let mean = simulation.sample_accumulator()?.mean()?;
        self.base.results_mut().instrument.value = Some(mean);
        if RNG::ALLOWS_ERROR_ESTIMATE {
            let error = simulation.error_estimate()?;
            self.base.results_mut().instrument.error_estimate = Some(error);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;
    use crate::exercise::EuropeanExercise;
    use crate::handle::{Handle, RelinkableHandle};
    use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff};
    use crate::interestrate::Compounding;
    use crate::math::randomnumbers::rngtraits::PseudoRandom;
    use crate::methods::montecarlo::Path;
    use crate::option::OptionType;
    use crate::quotes::make_quote_handle;
    use crate::shared::{Shared, shared};
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
        shared(crate::processes::BlackScholesMertonProcess::new(
            spot.handle(),
            flat_yield(Q),
            flat_yield(R),
            vol.handle(),
        )) as Shared<dyn StochasticProcess1D>
    }

    fn engine(
        time_steps: Option<Size>,
        time_steps_per_year: Option<Size>,
        required_samples: Option<Size>,
    ) -> McVanillaEngineBase<PseudoRandom> {
        McVanillaEngineBase::new(
            gbs_process(),
            time_steps,
            time_steps_per_year,
            false,
            false,
            false,
            required_samples,
            None,
            None,
            42,
        )
        .unwrap()
    }

    fn set_option(engine: &mut McVanillaEngineBase<PseudoRandom>, expiry: Date) {
        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_mut::<OptionArguments>()
            .unwrap();
        args.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Call, 100.0))
            as Shared<dyn StrikedTypePayoff>);
        args.exercise =
            Some(shared(EuropeanExercise::new(expiry)) as Shared<dyn crate::exercise::Exercise>);
    }

    #[test]
    fn time_grid_uses_fixed_step_count() {
        let mut e = engine(Some(12), None, None);
        set_option(&mut e, Date::new(15, Month::June, 2027));
        assert_eq!(e.time_grid().unwrap().size(), 13);
    }

    #[test]
    fn time_grid_per_year_keeps_at_least_one_step() {
        let mut e = engine(None, Some(1), None);
        set_option(&mut e, Date::new(15, Month::December, 2026));
        assert_eq!(e.time_grid().unwrap().size(), 2);
    }

    #[test]
    fn run_writes_the_mean_and_error_estimate() {
        const K: Real = 7.25;
        let mut e = engine(Some(4), None, Some(1_000));
        set_option(&mut e, Date::new(15, Month::June, 2027));
        e.run(|_: &Path| K).unwrap();

        let results = (e.results() as &dyn Any)
            .downcast_ref::<OneAssetOptionResults>()
            .unwrap();
        assert_eq!(results.instrument.value, Some(K));
        assert!(results.instrument.error_estimate.is_some());
    }

    #[test]
    fn both_time_step_forms_are_rejected() {
        assert!(
            McVanillaEngineBase::<PseudoRandom>::new(
                gbs_process(),
                Some(12),
                Some(50),
                false,
                false,
                false,
                None,
                None,
                None,
                42,
            )
            .is_err()
        );
    }

    #[test]
    fn missing_time_steps_are_rejected() {
        assert!(
            McVanillaEngineBase::<PseudoRandom>::new(
                gbs_process(),
                None,
                None,
                false,
                false,
                false,
                None,
                None,
                None,
                42,
            )
            .is_err()
        );
    }

    #[test]
    fn zero_time_steps_are_rejected() {
        assert!(
            McVanillaEngineBase::<PseudoRandom>::new(
                gbs_process(),
                Some(0),
                None,
                false,
                false,
                false,
                None,
                None,
                None,
                42,
            )
            .is_err()
        );
    }
}
