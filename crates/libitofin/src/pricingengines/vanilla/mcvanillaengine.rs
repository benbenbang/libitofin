//! Monte Carlo vanilla-option engine base.
//!
//! Port of `ql/pricingengines/vanilla/mcvanillaengine.hpp`: the shared plumbing
//! every Monte Carlo vanilla engine builds on. It selects the simulation
//! [`TimeGrid`] from the option's exercise date
//! (`mcvanillaengine.hpp:153`), builds the policy's path generator from the RNG
//! policy (`mcvanillaengine.hpp:72`), runs the [`McSimulation`], and writes the
//! mean (and, when the policy supports it, the error estimate) into the option
//! results (`mcvanillaengine.hpp:40`).
//!
//! Divergences from `mcvanillaengine.hpp`, all deliberate:
//! - **composition, not multiple inheritance**: C++ derives from both
//!   `Inst::engine` and `McSimulation` (`mcvanillaengine.hpp:37`). Rust has no
//!   MI, so [`McVanillaEngineBase`] *holds* an [`OneAssetOptionEngine`] and
//!   builds a fresh [`McSimulation`] per [`run`](McVanillaEngineBase::run). The
//!   payoff-dependent path pricer, C++'s pure-virtual `pathPricer()`, is passed
//!   into [`run`](McVanillaEngineBase::run) by the concrete engine (`#452`).
//! - **the MC policy names the process trait**: C++ holds the multi-factor
//!   `StochasticProcess` for both policies because its `StochasticProcess1D`
//!   IS-A `StochasticProcess`; here the two traits are siblings, so the
//!   [`McTraits`] policy `MC` (`SingleVariate` by default, `MultiVariate` for
//!   a multi-factor engine) fixes the process handle, the generator, and the
//!   path type together (`mctraits.hpp:39-57`). The generator dimension is
//!   `MC::factors(process) * (grid.size() - 1)` (`mcvanillaengine.hpp:74-77`),
//!   which for `SingleVariate` is the plain `grid.size() - 1`.
//! - **`Null` sentinels become [`Option`]**: the `timeSteps`,
//!   `timeStepsPerYear`, `requiredSamples`, `requiredTolerance`, and
//!   `maxSamples` sentinels (`mcvanillaengine.hpp:60-69`) are `Option` (D10).
//! - **statistics fixed to [`GeneralStatistics`](crate::math::statistics::GeneralStatistics)**: C++ is generic over `S`,
//!   defaulting to `Statistics` (`mcvanillaengine.hpp:36`); both concrete engines
//!   use that default, so `S` is fixed rather than a third generic.
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **control variate**: the flag threads to [`McSimulation`], which rejects
//!   it as deferred; `controlVariateValue` and the control
//!   pricing engine (`mcvanillaengine.hpp:82,126`) are not ported.

use std::marker::PhantomData;

use crate::errors::QlResult;
use crate::instruments::{OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::math::statistics::MeanStdDev;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{McSimulation, McTraits, PathGen, PathPricer, SingleVariate};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, Results};
use crate::types::{Real, Size};
use crate::{fail, require};

/// Shared Monte Carlo plumbing for vanilla-option engines, generic over the
/// RNG policy `RNG` and the MC policy `MC` (the C++ `RNG` and `MC` template
/// arguments, `mcvanillaengine.hpp:35`).
///
/// A concrete engine embeds one, delegates its
/// [`PricingEngine`](crate::pricingengine::PricingEngine) accessors to it, and
/// drives a calculation by building a path pricer and calling
/// [`run`](McVanillaEngineBase::run).
pub struct McVanillaEngineBase<RNG, MC: McTraits = SingleVariate> {
    base: OneAssetOptionEngine,
    process: MC::Process,
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

impl<RNG: McRngTraits, MC: McTraits> McVanillaEngineBase<RNG, MC> {
    /// Builds the engine base (`mcvanillaengine.hpp:96`), registering with the
    /// process so its changes invalidate the attached instrument.
    ///
    /// # Errors
    ///
    /// Errors if neither `time_steps` nor `time_steps_per_year` is set, if both
    /// are set, or if either is `Some(0)` (`mcvanillaengine.hpp:111-122`).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: MC::Process,
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
        base.register_with(MC::observable(&process));

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

    /// The results being filled, for an engine writing more than the mean.
    pub fn results_mut(&mut self) -> &mut OneAssetOptionResults {
        self.base.results_mut()
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
        let t = MC::time(&self.process, &exercise.last_date())?;
        if let Some(steps) = self.time_steps {
            TimeGrid::new(t, steps)
        } else if let Some(per_year) = self.time_steps_per_year {
            let steps = (per_year as Real * t) as Size;
            TimeGrid::new(t, steps.max(1))
        } else {
            fail!("time steps not specified");
        }
    }

    /// The policy's path generator, seeded from the RNG policy
    /// (`mcvanillaengine.hpp:72`).
    ///
    /// # Errors
    ///
    /// Propagates a [`time_grid`](McVanillaEngineBase::time_grid),
    /// sequence-generator, or generator-construction failure.
    pub fn path_generator(&self) -> QlResult<MC::Generator<RNG::RsgType>> {
        self.path_generator_with_seed(self.seed)
    }

    /// The same generator on a caller-supplied seed, the seam a two-pass engine
    /// needs to draw calibration paths from a stream independent of the pricing
    /// one (`mclongstaffschwartzengine.hpp:185-191`).
    ///
    /// # Errors
    ///
    /// As [`path_generator`](McVanillaEngineBase::path_generator).
    pub fn path_generator_with_seed(&self, seed: u32) -> QlResult<MC::Generator<RNG::RsgType>> {
        let grid = self.time_grid()?;
        let dimension = MC::factors(&self.process) * (grid.size() - 1);
        let generator = RNG::make_sequence_generator(dimension, seed)?;
        MC::path_generator(self.process.clone(), grid, generator, self.brownian_bridge)
    }

    /// Runs the simulation: builds the policy's path generator from the RNG
    /// policy and prices each path with `path_pricer`
    /// (`mcvanillaengine.hpp:40,72`).
    ///
    /// # Errors
    ///
    /// Propagates a generator, simulation, or accumulation failure.
    pub fn run<P: PathPricer<MC::PathType>>(&mut self, path_pricer: P) -> QlResult<()> {
        let generator = self.path_generator()?;
        self.run_with(generator, path_pricer)
    }

    /// Runs the simulation with a caller-supplied generator and pricer, then
    /// writes the mean (and, when `RNG::ALLOWS_ERROR_ESTIMATE`, the error
    /// estimate) into the results (`mcvanillaengine.hpp:40`).
    ///
    /// This is the seam under [`run`](Self::run) for an engine that builds its
    /// own generator; the mean/error result plumbing is path-type-agnostic.
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
    use crate::methods::montecarlo::{MultiPath, MultiVariate, Path};
    use crate::option::OptionType;
    use crate::processes::HestonProcess;
    use crate::quotes::make_quote_handle;
    use crate::shared::{Shared, shared};
    use crate::stochasticprocess::{StochasticProcess, StochasticProcess1D};
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

    /// The `MultiVariate` policy drives the same `run` over a
    /// [`MultiPathGenerator`](crate::methods::montecarlo::MultiPathGenerator):
    /// the RNG dimension is `factors * steps` (`mcvanillaengine.hpp:74-77`),
    /// which the generator constructor checks, so a wrong factor count would
    /// fail here rather than price.
    #[test]
    fn run_drives_the_multi_variate_policy() {
        const K: Real = 3.5;
        let process = shared(HestonProcess::new(
            flat_yield(R),
            flat_yield(Q),
            make_quote_handle(SPOT).handle(),
            0.04,
            1.2,
            0.06,
            0.3,
            -0.5,
        )) as Shared<dyn StochasticProcess>;
        let mut e = McVanillaEngineBase::<PseudoRandom, MultiVariate>::new(
            process,
            Some(4),
            None,
            false,
            false,
            false,
            Some(1_000),
            None,
            None,
            42,
        )
        .unwrap();
        let args = (e.arguments_mut() as &mut dyn Any)
            .downcast_mut::<OptionArguments>()
            .unwrap();
        args.exercise = Some(
            shared(EuropeanExercise::new(Date::new(15, Month::June, 2027)))
                as Shared<dyn crate::exercise::Exercise>,
        );

        assert_eq!(e.path_generator().unwrap().dimension(), 2 * 4);
        e.run(|_: &MultiPath| K).unwrap();

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
