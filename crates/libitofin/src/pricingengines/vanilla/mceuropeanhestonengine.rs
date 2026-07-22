//! Monte Carlo Heston-model engine for European options.
//!
//! Port of `ql/pricingengines/vanilla/mceuropeanhestonengine.hpp`: the concrete
//! Monte Carlo engine for European vanilla options priced under the 2-factor
//! [`HestonProcess`]. It builds a [`EuropeanHestonPathPricer`] from the option's
//! payoff and the process discount factor (`mceuropeanhestonengine.hpp:114-132`),
//! drives a [`MultiPathGenerator`] over the process, and accumulates through the
//! [`McSimulation`] spine. The [`MakeMcEuropeanHestonEngine`] builder ports the
//! C++ `MakeMCEuropeanHestonEngine` factory (`mceuropeanhestonengine.hpp:62`).
//!
//! Divergences from `mceuropeanhestonengine.hpp`, all deliberate:
//! - **self-contained over the multi-factor spine, not `McVanillaEngineBase`**:
//!   C++ derives `MCEuropeanHestonEngine` from `MCVanillaEngine<MultiVariate>`
//!   (`mceuropeanhestonengine.hpp:42-43`), whose `process_` is a
//!   `StochasticProcess` (the multi-factor base). In this Rust port
//!   [`StochasticProcess`] and
//!   [`StochasticProcess1D`](crate::stochasticprocess::StochasticProcess1D) are
//!   siblings, not parent/child, and
//!   [`McVanillaEngineBase`](crate::pricingengines::vanilla::McVanillaEngineBase)
//!   stores the single-factor `StochasticProcess1D`, so a [`HestonProcess`]
//!   cannot be handed to it. Rather than fake a 1D facade for a 2-factor process,
//!   this engine embeds [`OneAssetOptionEngine`] directly and drives
//!   [`McSimulation`]/[`MonteCarloModel`](crate::methods::montecarlo::MonteCarloModel)/[`MultiPathGenerator`]
//!   itself. The C++ `run_with` body it replaces is ten lines; the accumulation
//!   machinery is reused untouched. Follow-up #479 unifies the base on the
//!   multi-factor process (the C++-faithful direction).
//! - **concrete Heston process**: C++ holds a generic `StochasticProcess` and
//!   `dynamic_pointer_cast`s to `P = HestonProcess` in `pathPricer()`
//!   (`mceuropeanhestonengine.hpp:121-123`). The engine here holds a
//!   `Shared<HestonProcess>` concretely, so that downcast is a compile-time fact.
//! - **payoff downcast stays a run-time guard**: the option's payoff is an erased
//!   [`StrikedTypePayoff`], so the C++ "non-plain payoff given" downcast
//!   (`mceuropeanhestonengine.hpp:116-119`) survives as a run-time `Err` in
//!   [`calculate`](MCEuropeanHestonEngine::calculate).
//! - **antithetic variate is SUPPORTED**: unlike the single-factor
//!   [`MakeMcEuropeanEngine`](crate::pricingengines::vanilla::MakeMcEuropeanEngine),
//!   whose antithetic path is a fail-loud deferral, the multi-factor
//!   [`MultiPathGenerator`] wires the live antithetic negation, so
//!   [`with_antithetic_variate`](MakeMcEuropeanHestonEngine::with_antithetic_variate)
//!   reaches [`MonteCarloModel`](crate::methods::montecarlo::MonteCarloModel)
//!   averaging (the C++ `MakeMCEuropeanHestonEngine` default).
//! - **no `with_brownian_bridge`**: the C++ builder omits it
//!   (`mceuropeanhestonengine.hpp:65-72`); the generator is always driven with
//!   `brownian_bridge = false`.

use std::any::Any;
use std::marker::PhantomData;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{
    OneAssetOptionEngine, OneAssetOptionResults, OptionArguments, PlainVanillaPayoff,
    StrikedTypePayoff, TypePayoff,
};
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::math::statistics::MeanStdDev;
use crate::math::timegrid::TimeGrid;
use crate::methods::montecarlo::{McSimulation, MultiPath, MultiPathGenerator, PathPricer};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::payoff::Payoff;
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::processes::HestonProcess;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess;
use crate::types::{DiscountFactor, Real, Size};
use crate::{fail, require};

/// Prices one realized [`MultiPath`] as the discounted terminal payoff of asset
/// 0 (`mceuropeanhestonengine.hpp:228-235`): `payoff(multiPath[0].back()) *
/// discount`. Asset 0 is the spot; asset 1 is the variance and is never read.
pub struct EuropeanHestonPathPricer {
    payoff: PlainVanillaPayoff,
    discount: DiscountFactor,
}

impl EuropeanHestonPathPricer {
    /// Builds the pricer from the option type, strike, and terminal discount
    /// factor (`mceuropeanhestonengine.hpp:219-226`).
    ///
    /// # Errors
    ///
    /// Errors if `strike` is negative (`mceuropeanhestonengine.hpp:224`).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(
        option_type: OptionType,
        strike: Real,
        discount: DiscountFactor,
    ) -> QlResult<EuropeanHestonPathPricer> {
        require!(strike >= 0.0, "strike less than zero not allowed");
        Ok(EuropeanHestonPathPricer {
            payoff: PlainVanillaPayoff::new(option_type, strike),
            discount,
        })
    }
}

impl PathPricer<MultiPath> for EuropeanHestonPathPricer {
    /// The discounted terminal payoff of asset 0
    /// (`mceuropeanhestonengine.hpp:230-234`). The C++ `QL_REQUIRE(n > 0)` guard
    /// is unnecessary: the generator always builds a non-empty path.
    fn price(&self, multi_path: &MultiPath) -> Real {
        self.payoff.value(multi_path[0].back()) * self.discount
    }
}

/// Monte Carlo pricing engine for European vanilla options under the Heston
/// model (`mceuropeanhestonengine.hpp:42`), generic over the RNG policy `RNG`.
pub struct MCEuropeanHestonEngine<RNG> {
    base: OneAssetOptionEngine,
    process: Shared<HestonProcess>,
    time_steps: Option<Size>,
    time_steps_per_year: Option<Size>,
    antithetic_variate: bool,
    required_samples: Option<Size>,
    required_tolerance: Option<Real>,
    max_samples: Option<Size>,
    seed: u32,
    _rng: PhantomData<RNG>,
}

impl<RNG: McRngTraits> MCEuropeanHestonEngine<RNG> {
    /// Builds the engine (`mceuropeanhestonengine.hpp:100-108`). Prefer
    /// [`MakeMcEuropeanHestonEngine`] for the validated construction path.
    ///
    /// # Errors
    ///
    /// Errors if neither `time_steps` nor `time_steps_per_year` is set, if both
    /// are set, or if either is `Some(0)`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: Shared<HestonProcess>,
        time_steps: Option<Size>,
        time_steps_per_year: Option<Size>,
        antithetic_variate: bool,
        required_samples: Option<Size>,
        required_tolerance: Option<Real>,
        max_samples: Option<Size>,
        seed: u32,
    ) -> QlResult<MCEuropeanHestonEngine<RNG>> {
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

        Ok(MCEuropeanHestonEngine {
            base,
            process,
            time_steps,
            time_steps_per_year,
            antithetic_variate,
            required_samples,
            required_tolerance,
            max_samples,
            seed,
            _rng: PhantomData,
        })
    }

    /// The simulation time grid, from the option's exercise date via the Heston
    /// process day count (`mcvanillaengine.hpp:153`). Mirrors the inherited base
    /// grid logic: a fixed step count, or `floor(stepsPerYear * t)` clamped to at
    /// least one step.
    ///
    /// # Errors
    ///
    /// Errors if no exercise is set, if the process cannot map the date to a
    /// time, or on a degenerate grid.
    pub fn time_grid(&self) -> QlResult<TimeGrid> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
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
}

impl<RNG: McRngTraits> AsObservable for MCEuropeanHestonEngine<RNG> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<RNG: McRngTraits> PricingEngine for MCEuropeanHestonEngine<RNG> {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// Builds the [`EuropeanHestonPathPricer`], drives a [`MultiPathGenerator`]
    /// over the Heston process through the [`McSimulation`] spine, and writes the
    /// mean (and, when the RNG policy supports it, the error estimate) into the
    /// results (`mceuropeanhestonengine.hpp:114-132`, `mcvanillaengine.hpp:40`).
    ///
    /// # Errors
    ///
    /// Errors on a missing/non-European exercise, a missing or non-plain payoff,
    /// or any propagated grid/discount/generator/simulation failure.
    fn calculate(&mut self) -> QlResult<()> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        if exercise.exercise_type() != ExerciseType::European {
            fail!("not an European option");
        }
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff: &dyn StrikedTypePayoff = &**payoff;
        let Some(payoff) = (payoff as &dyn Any).downcast_ref::<PlainVanillaPayoff>() else {
            fail!("non-plain payoff given");
        };
        let option_type = payoff.option_type();
        let strike = payoff.strike();

        let grid = self.time_grid()?;
        let Some(last_time) = grid.back() else {
            fail!("empty time grid");
        };
        let discount = self
            .process
            .risk_free_rate()
            .current_link()?
            .discount(last_time, false)?;

        let dimensions = self.process.factors() * (grid.size() - 1);
        let generator = RNG::make_sequence_generator(dimensions, self.seed)?;
        let mpg = MultiPathGenerator::new(
            Shared::clone(&self.process) as Shared<dyn StochasticProcess>,
            grid,
            generator,
            false,
        )?;

        let pricer = EuropeanHestonPathPricer::new(option_type, strike, discount)?;

        let mut simulation = McSimulation::<
            MultiPathGenerator<RNG::RsgType>,
            EuropeanHestonPathPricer,
        >::new(self.antithetic_variate, false);
        simulation.calculate(
            mpg,
            pricer,
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

/// Factory for [`MCEuropeanHestonEngine`] (`mceuropeanhestonengine.hpp:62`),
/// generic over the RNG policy `RNG`.
///
/// Validation the C++ builder splits across its setters is deferred to
/// [`build`](MakeMcEuropeanHestonEngine::build) so the setters stay infallible
/// and chainable.
pub struct MakeMcEuropeanHestonEngine<RNG> {
    process: Shared<HestonProcess>,
    steps: Option<Size>,
    steps_per_year: Option<Size>,
    samples: Option<Size>,
    max_samples: Option<Size>,
    tolerance: Option<Real>,
    antithetic: bool,
    seed: u32,
    _rng: PhantomData<RNG>,
}

impl<RNG: McRngTraits> MakeMcEuropeanHestonEngine<RNG> {
    /// Starts a builder on the given Heston process
    /// (`mceuropeanhestonengine.hpp:136-139`).
    pub fn new(process: Shared<HestonProcess>) -> MakeMcEuropeanHestonEngine<RNG> {
        MakeMcEuropeanHestonEngine {
            process,
            steps: None,
            steps_per_year: None,
            samples: None,
            max_samples: None,
            tolerance: None,
            antithetic: false,
            seed: 0,
            _rng: PhantomData,
        }
    }

    /// Sets the fixed number of time steps (`mceuropeanhestonengine.hpp:143`).
    #[must_use]
    pub fn with_steps(mut self, steps: Size) -> Self {
        self.steps = Some(steps);
        self
    }

    /// Sets the number of time steps per year
    /// (`mceuropeanhestonengine.hpp:152`).
    #[must_use]
    pub fn with_steps_per_year(mut self, steps: Size) -> Self {
        self.steps_per_year = Some(steps);
        self
    }

    /// Sets the required number of samples (`mceuropeanhestonengine.hpp:161`).
    #[must_use]
    pub fn with_samples(mut self, samples: Size) -> Self {
        self.samples = Some(samples);
        self
    }

    /// Sets the required absolute tolerance
    /// (`mceuropeanhestonengine.hpp:170`).
    #[must_use]
    pub fn with_absolute_tolerance(mut self, tolerance: Real) -> Self {
        self.tolerance = Some(tolerance);
        self
    }

    /// Sets the maximum number of samples (`mceuropeanhestonengine.hpp:182`).
    #[must_use]
    pub fn with_max_samples(mut self, samples: Size) -> Self {
        self.max_samples = Some(samples);
        self
    }

    /// Sets the RNG seed (`mceuropeanhestonengine.hpp:189`).
    #[must_use]
    pub fn with_seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }

    /// Requests the antithetic-variate variance reduction
    /// (`mceuropeanhestonengine.hpp:196`). Supported: the multi-factor generator
    /// wires the antithetic negation.
    #[must_use]
    pub fn with_antithetic_variate(mut self, antithetic: bool) -> Self {
        self.antithetic = antithetic;
        self
    }

    /// Builds the configured [`MCEuropeanHestonEngine`]
    /// (`mceuropeanhestonengine.hpp:204-215`).
    ///
    /// # Errors
    ///
    /// Errors if neither or both of `steps`/`steps_per_year` are set
    /// (`mceuropeanhestonengine.hpp:144,153,205`), if both `samples` and
    /// `tolerance` are set (`mceuropeanhestonengine.hpp:162,171`), or if a
    /// tolerance is set on an RNG policy without an error estimate
    /// (`mceuropeanhestonengine.hpp:173`).
    pub fn build(self) -> QlResult<MCEuropeanHestonEngine<RNG>> {
        require!(
            self.steps.is_some() || self.steps_per_year.is_some(),
            "number of steps not given"
        );
        require!(
            self.steps.is_none() || self.steps_per_year.is_none(),
            "number of steps overspecified"
        );
        require!(
            !(self.samples.is_some() && self.tolerance.is_some()),
            "number of samples already set"
        );
        if self.tolerance.is_some() {
            require!(
                RNG::ALLOWS_ERROR_ESTIMATE,
                "chosen random generator policy does not allow an error estimate"
            );
        }

        MCEuropeanHestonEngine::new(
            self.process,
            self.steps,
            self.steps_per_year,
            self.antithetic,
            self.samples,
            self.tolerance,
            self.max_samples,
            self.seed,
        )
    }
}

#[cfg(test)]
mod builder_tests {
    //! Guards on [`MakeMcEuropeanHestonEngine::build`] validation
    //! (`mceuropeanhestonengine.hpp:144,153,162,171,205`).

    use super::MakeMcEuropeanHestonEngine;
    use crate::handle::Handle;
    use crate::interestrate::Compounding;
    use crate::math::randomnumbers::rngtraits::PseudoRandom;
    use crate::processes::HestonProcess;
    use crate::quotes::make_quote_handle;
    use crate::shared::{Shared, shared};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::Rate;

    fn flat(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            Date::new(15, Month::June, 2026),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn maker() -> MakeMcEuropeanHestonEngine<PseudoRandom> {
        let process = shared(HestonProcess::new(
            flat(0.05),
            flat(0.02),
            make_quote_handle(100.0).handle(),
            0.04,
            1.2,
            0.06,
            0.3,
            -0.5,
        ));
        MakeMcEuropeanHestonEngine::new(process)
    }

    #[test]
    fn missing_steps_is_rejected() {
        assert!(maker().with_samples(1_000).build().is_err());
    }

    #[test]
    fn overspecified_steps_is_rejected() {
        assert!(
            maker()
                .with_steps(1)
                .with_steps_per_year(50)
                .with_samples(1_000)
                .build()
                .is_err()
        );
    }

    #[test]
    fn samples_and_tolerance_together_are_rejected() {
        assert!(
            maker()
                .with_steps(1)
                .with_samples(1_000)
                .with_absolute_tolerance(0.02)
                .build()
                .is_err()
        );
    }

    #[test]
    fn antithetic_config_builds() {
        assert!(
            maker()
                .with_steps_per_year(11)
                .with_samples(1_000)
                .with_antithetic_variate(true)
                .with_seed(1234)
                .build()
                .is_ok()
        );
    }
}
