//! Monte Carlo American-option engine.
//!
//! Port of `ql/pricingengines/vanilla/mcamericanengine.{hpp,cpp}`: the
//! Longstaff-Schwartz engine for American vanilla options.
//! [`AmericanPathPricer`] supplies the three things the backward induction needs
//! from the option (`mcamericanengine.cpp:31-72`), [`MCAmericanEngine`] wires it
//! into the two-pass [`McLongstaffSchwartzEngineBase`] driver, and
//! [`MakeMcAmericanEngine`] ports the `MakeMCAmericanEngine` factory
//! (`mcamericanengine.hpp:105`).
//!
//! The regression state is the spot SCALED by `1 / strike` (`:51,66`), for
//! numerical stability, while the exercise value is the payoff of the UNSCALED
//! spot (`:60`). The payoff itself is appended to the basis system as one extra
//! function (`:45-46`), so order `n` yields `n + 2` basis functions:
//! `{1, s, ..., s^n, payoff(s)}`, all evaluated at the scaled state.
//!
//! Divergences from `mcamericanengine.{hpp,cpp}`, all deliberate:
//! - **the striked-payoff scaling is unconditional**: C++ takes an
//!   `ext::shared_ptr<Payoff>` and only divides `scalingValue_` by the strike
//!   when the `dynamic_pointer_cast<StrikedTypePayoff>` succeeds (`:48-52`).
//!   `OptionArguments::payoff` is already a `StrikedTypePayoff` here, so the
//!   cast is a compile-time fact and the scaling is always `1 / strike`.
//! - **the polynomial-family check is a compile-time fact**: the C++ ctor
//!   rejects an unsupported `LsmBasisSystem::PolynomialType` at run time
//!   (`:38-43`); [`PolynomialType`] carries only the ported `Monomial`.
//! - **the process is concretely a [`GeneralizedBlackScholesProcess`]**, so the
//!   "generalized Black-Scholes process required" downcast
//!   (`mcamericanengine.hpp:190-193`) cannot fail at run time, as with
//!   [`MCEuropeanEngine`](super::MCEuropeanEngine).
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **`payoffAtExpiry` rejection** (`mcamericanengine.hpp:197-198`): the flag
//!   lives on the C++ `EarlyExercise` base, which arrives with
//!   `AmericanExercise` in #762; the guard is owed by that ticket. What this
//!   engine can check today, it does: a non-American exercise is rejected, which
//!   also closes the deferred Bermudan time grid.
//! - **control variate** (`mcamericanengine.hpp:74-77,176-180`): the CV path
//!   pricer, the analytic control engine, and the `max(0, value)` floor
//!   `calculate()` applies under it are omitted, as are the builder's
//!   `withControlVariate` and `withBasisSystem` (one family ported).
//! - **the multi-asset `MCAmericanBasketEngine`**, needing the `MultiPath` form
//!   of the Longstaff-Schwartz pricer.

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::instruments::StrikedTypePayoff;
use crate::math::randomnumbers::rngtraits::McRngTraits;
use crate::methods::montecarlo::{
    EarlyExercisePathPricer, LongstaffSchwartzPathPricer, LsmBasisSystem, Path, PolynomialType,
};
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::pricingengines::McLongstaffSchwartzEngineBase;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Real, Size};
use crate::{fail, require};

/// Values an American vanilla option along one realized path
/// (`mcamericanengine.cpp:31-72`).
pub struct AmericanPathPricer {
    payoff: Shared<dyn StrikedTypePayoff>,
    scaling_value: Real,
    polynomial_order: Size,
    polynomial_type: PolynomialType,
}

impl AmericanPathPricer {
    /// Builds the pricer, scaling the regression state by `1 / strike`
    /// (`mcamericanengine.cpp:48-52`).
    pub fn new(
        payoff: Shared<dyn StrikedTypePayoff>,
        polynomial_order: Size,
        polynomial_type: PolynomialType,
    ) -> AmericanPathPricer {
        let scaling_value = 1.0 / payoff.strike();
        AmericanPathPricer {
            payoff,
            scaling_value,
            polynomial_order,
            polynomial_type,
        }
    }
}

impl EarlyExercisePathPricer<Path> for AmericanPathPricer {
    type State = Real;

    /// The payoff of the UNSCALED spot (`mcamericanengine.cpp:55-61`). C++ round
    /// trips the state through the scaling rather than reading `path[t]`, and so
    /// does this, so the two agree to the last bit.
    fn value(&self, path: &Path, t: Size) -> Real {
        self.payoff.value(self.state(path, t) / self.scaling_value)
    }

    /// The spot scaled by `1 / strike` (`mcamericanengine.cpp:63-66`).
    fn state(&self, path: &Path, t: Size) -> Real {
        path[t] * self.scaling_value
    }

    /// The monomials plus the payoff as one extra function
    /// (`mcamericanengine.cpp:45-46`). The appended function takes the SCALED
    /// state and undoes the scaling before applying the payoff (`:57`).
    fn basis_system(&self) -> Vec<Box<dyn Fn(Real) -> Real>> {
        let mut v = LsmBasisSystem::path_basis_system(self.polynomial_order, self.polynomial_type);
        let payoff = Shared::clone(&self.payoff);
        let scaling_value = self.scaling_value;
        v.push(Box::new(move |state: Real| {
            payoff.value(state / scaling_value)
        }));
        v
    }
}

/// American vanilla Monte Carlo engine (`mcamericanengine.hpp:51`), generic over `RNG`.
pub struct MCAmericanEngine<RNG> {
    base: McLongstaffSchwartzEngineBase<RNG>,
    process: Shared<GeneralizedBlackScholesProcess>,
    polynomial_order: Size,
    polynomial_type: PolynomialType,
}

impl<RNG: McRngTraits> MCAmericanEngine<RNG> {
    /// Builds the engine (`mcamericanengine.hpp:140-172`), which fixes the
    /// Brownian bridge to `false` (`:161`). Prefer [`MakeMcAmericanEngine`].
    ///
    /// # Errors
    ///
    /// Propagates the driver's time-step validation.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        time_steps: Option<Size>,
        time_steps_per_year: Option<Size>,
        antithetic_variate: bool,
        required_samples: Option<Size>,
        required_tolerance: Option<Real>,
        max_samples: Option<Size>,
        seed: u32,
        polynomial_order: Size,
        polynomial_type: PolynomialType,
        n_calibration_samples: Option<Size>,
        antithetic_variate_calibration: Option<bool>,
        seed_calibration: Option<u32>,
    ) -> QlResult<MCAmericanEngine<RNG>> {
        let base = McLongstaffSchwartzEngineBase::new(
            Shared::clone(&process) as Shared<dyn StochasticProcess1D>,
            time_steps,
            time_steps_per_year,
            false,
            antithetic_variate,
            false,
            required_samples,
            required_tolerance,
            max_samples,
            seed,
            n_calibration_samples,
            antithetic_variate_calibration,
            seed_calibration,
        )?;
        Ok(MCAmericanEngine {
            base,
            process,
            polynomial_order,
            polynomial_type,
        })
    }

    /// The two-pass driver underneath, for the calibration settings it defaults.
    pub fn lsm_base(&self) -> &McLongstaffSchwartzEngineBase<RNG> {
        &self.base
    }

    /// The regression order the basis system is built to.
    pub fn polynomial_order(&self) -> Size {
        self.polynomial_order
    }

    /// The C++ `lsmPathPricer()` hook (`mcamericanengine.hpp:186-207`): a fresh
    /// [`LongstaffSchwartzPathPricer`] over an [`AmericanPathPricer`], discounted
    /// on the process risk-free curve.
    ///
    /// # Errors
    ///
    /// Errors on a missing payoff, a missing exercise, or an exercise that is
    /// not American (`:196`); propagates a grid or discount failure.
    pub fn lsm_path_pricer(&self) -> QlResult<Shared<LongstaffSchwartzPathPricer>> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        require!(
            exercise.exercise_type() == ExerciseType::American,
            "wrong exercise given"
        );
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };

        let early = shared(AmericanPathPricer::new(
            Shared::clone(payoff),
            self.polynomial_order,
            self.polynomial_type,
        )) as Shared<dyn EarlyExercisePathPricer<Path, State = Real>>;

        Ok(shared(LongstaffSchwartzPathPricer::new(
            &self.base.time_grid()?,
            early,
            &self.process.risk_free_rate(),
        )?))
    }
}

impl<RNG: McRngTraits> AsObservable for MCAmericanEngine<RNG> {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl<RNG: McRngTraits> PricingEngine for MCAmericanEngine<RNG> {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// Builds a fresh path pricer and runs both passes
    /// (`mcamericanengine.hpp:175-181`).
    ///
    /// # Errors
    ///
    /// Propagates a [`lsm_path_pricer`](MCAmericanEngine::lsm_path_pricer) or
    /// simulation failure.
    fn calculate(&mut self) -> QlResult<()> {
        let pricer = self.lsm_path_pricer()?;
        self.base.calculate_with(pricer)
    }
}

/// Factory for [`MCAmericanEngine`] (`mcamericanengine.hpp:105`), generic over
/// the RNG policy `RNG`.
///
/// As with [`MakeMcEuropeanEngine`](super::MakeMcEuropeanEngine), the validation
/// the C++ builder splits across its setters is deferred to
/// [`build`](MakeMcAmericanEngine::build) so the setters stay chainable.
pub struct MakeMcAmericanEngine<RNG> {
    process: Shared<GeneralizedBlackScholesProcess>,
    steps: Option<Size>,
    steps_per_year: Option<Size>,
    samples: Option<Size>,
    max_samples: Option<Size>,
    tolerance: Option<Real>,
    antithetic: bool,
    seed: u32,
    polynomial_order: Size,
    calibration_samples: Option<Size>,
    _rng: std::marker::PhantomData<RNG>,
}

impl<RNG: McRngTraits> MakeMcAmericanEngine<RNG> {
    /// Starts a builder on the given Black-Scholes process, with the C++
    /// defaults: polynomial order 2 and `Monomial` (`mcamericanengine.hpp:136`),
    /// no antithetic variate, and the driver's 2048 calibration samples.
    pub fn new(process: Shared<GeneralizedBlackScholesProcess>) -> MakeMcAmericanEngine<RNG> {
        MakeMcAmericanEngine {
            process,
            steps: None,
            steps_per_year: None,
            samples: None,
            max_samples: None,
            tolerance: None,
            antithetic: false,
            seed: 0,
            polynomial_order: 2,
            calibration_samples: None,
            _rng: std::marker::PhantomData,
        }
    }

    /// Sets the fixed number of time steps (`mcamericanengine.hpp:280`).
    #[must_use]
    pub fn with_steps(mut self, steps: Size) -> Self {
        self.steps = Some(steps);
        self
    }

    /// Sets the number of time steps per year (`mcamericanengine.hpp:287`).
    #[must_use]
    pub fn with_steps_per_year(mut self, steps: Size) -> Self {
        self.steps_per_year = Some(steps);
        self
    }

    /// Sets the required number of samples (`mcamericanengine.hpp:295`).
    #[must_use]
    pub fn with_samples(mut self, samples: Size) -> Self {
        self.samples = Some(samples);
        self
    }

    /// Sets the required absolute tolerance (`mcamericanengine.hpp:304`).
    #[must_use]
    pub fn with_absolute_tolerance(mut self, tolerance: Real) -> Self {
        self.tolerance = Some(tolerance);
        self
    }

    /// Sets the maximum number of samples (`mcamericanengine.hpp:317`).
    #[must_use]
    pub fn with_max_samples(mut self, samples: Size) -> Self {
        self.max_samples = Some(samples);
        self
    }

    /// Sets the RNG seed (`mcamericanengine.hpp:333`).
    #[must_use]
    pub fn with_seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }

    /// Requests the antithetic-variate variance reduction
    /// (`mcamericanengine.hpp:340`). Deferred: setting `true` makes
    /// [`build`](MakeMcAmericanEngine::build) return `Err`.
    #[must_use]
    pub fn with_antithetic_variate(mut self, antithetic: bool) -> Self {
        self.antithetic = antithetic;
        self
    }

    /// Sets the regression order (`mcamericanengine.hpp:266`).
    #[must_use]
    pub fn with_polynomial_order(mut self, order: Size) -> Self {
        self.polynomial_order = order;
        self
    }

    /// Sets the number of calibration paths (`mcamericanengine.hpp:325`).
    #[must_use]
    pub fn with_calibration_samples(mut self, samples: Size) -> Self {
        self.calibration_samples = Some(samples);
        self
    }

    /// Builds the configured [`MCAmericanEngine`]
    /// (`mcamericanengine.hpp:348-370`).
    ///
    /// # Errors
    ///
    /// Errors if neither or both of `steps`/`steps_per_year` are set (`:352-355`),
    /// if both `samples` and `tolerance` are set (`:296,305`), if a tolerance is
    /// set on an RNG policy without an error estimate (`:307`), or if the
    /// deferred antithetic variate is requested.
    pub fn build(self) -> QlResult<MCAmericanEngine<RNG>> {
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

        MCAmericanEngine::new(
            self.process,
            self.steps,
            self.steps_per_year,
            false,
            self.samples,
            self.tolerance,
            self.max_samples,
            self.seed,
            self.polynomial_order,
            PolynomialType::Monomial,
            self.calibration_samples,
            None,
            None,
        )
    }
}
