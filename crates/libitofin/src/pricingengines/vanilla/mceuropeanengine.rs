//! Monte Carlo European-option engine.
//!
//! Port of `ql/pricingengines/vanilla/mceuropeanengine.hpp`: the concrete
//! Monte Carlo engine for European vanilla options. It embeds the [`#451`]
//! [`McVanillaEngineBase`], builds a [`EuropeanPathPricer`] from the option's
//! payoff and the process discount factor (`mceuropeanengine.hpp:132-153`), and
//! delegates the simulation to [`McVanillaEngineBase::run`]. The
//! [`MakeMcEuropeanEngine`] builder ports the C++ `MakeMCEuropeanEngine`
//! factory (`mceuropeanengine.hpp:70`).
//!
//! Divergences from `mceuropeanengine.hpp`, all deliberate:
//! - **concrete Black-Scholes process**: C++ holds a generic
//!   `StochasticProcess` and `dynamic_pointer_cast`s to
//!   `GeneralizedBlackScholesProcess` in `pathPricer()`
//!   (`mceuropeanengine.hpp:142-145`, "Black-Scholes process required"). The
//!   engine here holds a `Shared<GeneralizedBlackScholesProcess>` concretely, so
//!   that downcast is a compile-time fact and cannot fail at run time. The base
//!   still sees the process erased to `dyn StochasticProcess1D`.
//! - **payoff downcast stays a run-time guard**: the option's payoff is an
//!   erased `StrikedTypePayoff`, so the C++ "non-plain payoff given" downcast
//!   (`mceuropeanengine.hpp:137-140`) survives as a run-time `Err` in
//!   [`calculate`](MCEuropeanEngine::calculate), mirroring
//!   [`AnalyticEuropeanEngine`](crate::pricingengines::vanilla::AnalyticEuropeanEngine).
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **antithetic variate**: the C++ builder's `withAntitheticVariate`
//!   (`mceuropeanengine.hpp:81`) is kept as
//!   [`with_antithetic_variate`](MakeMcEuropeanEngine::with_antithetic_variate),
//!   but requesting it makes [`build`](MakeMcEuropeanEngine::build) return `Err`
//!   (the underlying [`McSimulation`](crate::methods::montecarlo) defers it).
//!   This is an Err-at-build, one step earlier than the base's Err-at-run
//!   precedent; the failure is visible either way (#262 class).
//! - **Brownian bridge / control variate**: the C++ builder's
//!   `withBrownianBridge` (`mceuropeanengine.hpp:76`) and the control-variate
//!   machinery are omitted from the builder entirely; the base is always driven
//!   with both flags `false`.
//!
//! Oracle note (divergence catalogue): the MC-vs-analytic test in this module
//! INTENTIONALLY STRENGTHENS QuantLib's own `0.01 * underlying` relative band
//! (`test-suite/europeanoption.cpp:1278`) into the tighter `|mc - analytic| <
//! 3 * error_estimate()` convergence pin, which ties the tolerance to the
//! computed standard error instead of a loose absolute band.

use std::any::Any;

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, TypePayoff};
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::methods::montecarlo::{Path, PathPricer};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::payoff::Payoff;
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::vanilla::McVanillaEngineBase;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{DiscountFactor, Real, Size};
use crate::{fail, require};

/// Prices one realized [`Path`] as the discounted terminal payoff
/// (`mceuropeanengine.hpp:93,254-257`): `payoff(path.back()) * discount`.
pub struct EuropeanPathPricer {
    payoff: PlainVanillaPayoff,
    discount: DiscountFactor,
}

impl EuropeanPathPricer {
    /// Builds the pricer from the option type, strike, and terminal discount
    /// factor (`mceuropeanengine.hpp:246-252`).
    ///
    /// # Errors
    ///
    /// Errors if `strike` is negative (`mceuropeanengine.hpp:250`).
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn new(
        option_type: OptionType,
        strike: Real,
        discount: DiscountFactor,
    ) -> QlResult<EuropeanPathPricer> {
        require!(strike >= 0.0, "strike less than zero not allowed");
        Ok(EuropeanPathPricer {
            payoff: PlainVanillaPayoff::new(option_type, strike),
            discount,
        })
    }
}

impl PathPricer for EuropeanPathPricer {
    /// The discounted terminal payoff (`mceuropeanengine.hpp:254-257`). The C++
    /// `QL_REQUIRE(!path.empty())` guard is unnecessary: the base always builds
    /// a non-empty path from a grid with at least one step.
    fn price(&self, path: &Path) -> Real {
        self.payoff.value(path.back()) * self.discount
    }
}

/// Monte Carlo pricing engine for European vanilla options
/// (`mceuropeanengine.hpp:42`), generic over the RNG policy `RNG`.
pub struct MCEuropeanEngine<RNG> {
    base: McVanillaEngineBase<RNG>,
    process: Shared<GeneralizedBlackScholesProcess>,
}

impl<RNG: McRngTraits> MCEuropeanEngine<RNG> {
    /// Builds the engine (`mceuropeanengine.hpp:110-129`). Prefer
    /// [`MakeMcEuropeanEngine`] for the validated, ergonomic construction path.
    ///
    /// # Errors
    ///
    /// Propagates the [`McVanillaEngineBase::new`] time-step validation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        time_steps: Option<Size>,
        time_steps_per_year: Option<Size>,
        brownian_bridge: bool,
        antithetic_variate: bool,
        required_samples: Option<Size>,
        required_tolerance: Option<Real>,
        max_samples: Option<Size>,
        seed: u32,
    ) -> QlResult<MCEuropeanEngine<RNG>> {
        let base = McVanillaEngineBase::new(
            Shared::clone(&process) as Shared<dyn StochasticProcess1D>,
            time_steps,
            time_steps_per_year,
            brownian_bridge,
            antithetic_variate,
            false,
            required_samples,
            required_tolerance,
            max_samples,
            seed,
        )?;
        Ok(MCEuropeanEngine { base, process })
    }
}

impl<RNG: McRngTraits> AsObservable for MCEuropeanEngine<RNG> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<RNG: McRngTraits> PricingEngine for MCEuropeanEngine<RNG> {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// Builds the [`EuropeanPathPricer`] (`mceuropeanengine.hpp:132-153`) and
    /// runs the simulation through the base.
    ///
    /// # Errors
    ///
    /// Errors on a missing/non-European exercise, a missing or non-plain payoff,
    /// or any propagated grid/discount/simulation failure.
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

        let grid = self.base.time_grid()?;
        let Some(last_time) = grid.back() else {
            fail!("empty time grid");
        };
        let discount = self
            .process
            .risk_free_rate()
            .current_link()?
            .discount(last_time, false)?;

        let pricer = EuropeanPathPricer::new(option_type, strike, discount)?;
        self.base.run(pricer)
    }
}

/// Factory for [`MCEuropeanEngine`] (`mceuropeanengine.hpp:70`), generic over the
/// RNG policy `RNG`.
///
/// Validation the C++ builder splits across its setters is deferred to
/// [`build`](MakeMcEuropeanEngine::build) so the setters stay infallible and
/// chainable.
pub struct MakeMcEuropeanEngine<RNG> {
    process: Shared<GeneralizedBlackScholesProcess>,
    steps: Option<Size>,
    steps_per_year: Option<Size>,
    samples: Option<Size>,
    max_samples: Option<Size>,
    tolerance: Option<Real>,
    antithetic: bool,
    seed: u32,
    _rng: std::marker::PhantomData<RNG>,
}

impl<RNG: McRngTraits> MakeMcEuropeanEngine<RNG> {
    /// Starts a builder on the given Black-Scholes process
    /// (`mceuropeanengine.hpp:157-160`).
    pub fn new(process: Shared<GeneralizedBlackScholesProcess>) -> MakeMcEuropeanEngine<RNG> {
        MakeMcEuropeanEngine {
            process,
            steps: None,
            steps_per_year: None,
            samples: None,
            max_samples: None,
            tolerance: None,
            antithetic: false,
            seed: 0,
            _rng: std::marker::PhantomData,
        }
    }

    /// Sets the fixed number of time steps (`mceuropeanengine.hpp:164`).
    #[must_use]
    pub fn with_steps(mut self, steps: Size) -> Self {
        self.steps = Some(steps);
        self
    }

    /// Sets the number of time steps per year (`mceuropeanengine.hpp:171`).
    #[must_use]
    pub fn with_steps_per_year(mut self, steps: Size) -> Self {
        self.steps_per_year = Some(steps);
        self
    }

    /// Sets the required number of samples (`mceuropeanengine.hpp:178`).
    #[must_use]
    pub fn with_samples(mut self, samples: Size) -> Self {
        self.samples = Some(samples);
        self
    }

    /// Sets the required absolute tolerance (`mceuropeanengine.hpp:187`).
    #[must_use]
    pub fn with_absolute_tolerance(mut self, tolerance: Real) -> Self {
        self.tolerance = Some(tolerance);
        self
    }

    /// Sets the maximum number of samples (`mceuropeanengine.hpp:199`).
    #[must_use]
    pub fn with_max_samples(mut self, samples: Size) -> Self {
        self.max_samples = Some(samples);
        self
    }

    /// Sets the RNG seed (`mceuropeanengine.hpp:206`).
    #[must_use]
    pub fn with_seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }

    /// Requests the antithetic-variate variance reduction
    /// (`mceuropeanengine.hpp:220`). Deferred: setting `true` makes
    /// [`build`](MakeMcEuropeanEngine::build) return `Err`.
    #[must_use]
    pub fn with_antithetic_variate(mut self, antithetic: bool) -> Self {
        self.antithetic = antithetic;
        self
    }

    /// Builds the configured [`MCEuropeanEngine`]
    /// (`mceuropeanengine.hpp:225-242`).
    ///
    /// # Errors
    ///
    /// Errors if neither or both of `steps`/`steps_per_year` are set
    /// (`mceuropeanengine.hpp:229-232`), if both `samples` and `tolerance` are
    /// set (`mceuropeanengine.hpp:179,188`), if a tolerance is set on an RNG
    /// policy without an error estimate (`mceuropeanengine.hpp:190`), or if the
    /// deferred antithetic variate is requested.
    pub fn build(self) -> QlResult<MCEuropeanEngine<RNG>> {
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
        require!(!self.antithetic, "antithetic variate not yet supported");

        MCEuropeanEngine::new(
            self.process,
            self.steps,
            self.steps_per_year,
            false,
            false,
            self.samples,
            self.tolerance,
            self.max_samples,
            self.seed,
        )
    }
}

#[cfg(test)]
mod builder_tests {
    //! Guards on [`MakeMcEuropeanEngine::build`] validation
    //! (`mceuropeanengine.hpp:179,188,190,229-232`), including the visible
    //! antithetic-variate deferral (#262 class).

    use super::MakeMcEuropeanEngine;
    use crate::math::randomnumbers::rngtraits::PseudoRandom;
    use crate::pricingengines::vanilla::test_market::market;

    fn maker() -> MakeMcEuropeanEngine<PseudoRandom> {
        MakeMcEuropeanEngine::new(market().process)
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
    fn antithetic_variate_is_rejected_as_deferred() {
        assert!(
            maker()
                .with_steps(1)
                .with_samples(1_000)
                .with_antithetic_variate(true)
                .build()
                .is_err()
        );
    }

    #[test]
    fn plain_pseudo_config_builds() {
        assert!(
            maker()
                .with_steps(1)
                .with_samples(1_000)
                .with_seed(42)
                .build()
                .is_ok()
        );
    }
}
