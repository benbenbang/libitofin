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
    /// Errors on a missing payoff, a missing exercise, an exercise that is not
    /// American (`:196`), or one paying at expiry (`:197-198`); propagates a
    /// grid or discount failure.
    pub fn lsm_path_pricer(&self) -> QlResult<Shared<LongstaffSchwartzPathPricer>> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        require!(
            exercise.exercise_type() == ExerciseType::American,
            "wrong exercise given"
        );
        require!(!exercise.payoff_at_expiry(), "payoff at expiry not handled");
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
    /// (`mcamericanengine.hpp:340`), on both the pricing and, by the driver's
    /// fallback, the calibration pass.
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
    /// if both `samples` and `tolerance` are set (`:296,305`), or if a tolerance
    /// is set on an RNG policy without an error estimate (`:307`).
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
        MCAmericanEngine::new(
            self.process,
            self.steps,
            self.steps_per_year,
            self.antithetic,
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

#[cfg(test)]
mod tests {
    use std::any::Any;

    use super::*;
    use crate::exercise::{AmericanExercise, EuropeanExercise, Exercise};
    use crate::instruments::{OptionArguments, PlainVanillaPayoff};
    use crate::math::array::Array;
    use crate::math::randomnumbers::rngtraits::PseudoRandom;
    use crate::math::timegrid::TimeGrid;
    use crate::option::OptionType;
    use crate::pricingengines::vanilla::test_market::{Market, market, time_to_days, today};
    use crate::time::date::Date;

    const STRIKE: Real = 40.0;
    const TOL: Real = 1e-14;

    fn american(expiry: Date) -> Shared<dyn Exercise> {
        shared(AmericanExercise::over(today(), expiry).unwrap()) as Shared<dyn Exercise>
    }

    fn put_pricer(order: Size) -> AmericanPathPricer {
        AmericanPathPricer::new(
            shared(PlainVanillaPayoff::new(OptionType::Put, STRIKE))
                as Shared<dyn StrikedTypePayoff>,
            order,
            PolynomialType::Monomial,
        )
    }

    fn path(values: [Real; 3]) -> Path {
        Path::new(TimeGrid::new(1.0, 2).unwrap(), Array::from(values)).unwrap()
    }

    fn flat_market() -> Market {
        let market = market();
        market.set(36.0, 0.0, 0.06, 0.20);
        market
    }

    fn engine(market: &Market) -> MCAmericanEngine<PseudoRandom> {
        MakeMcAmericanEngine::<PseudoRandom>::new(Shared::clone(&market.process))
            .with_steps(4)
            .with_samples(256)
            .with_seed(42)
            .build()
            .unwrap()
    }

    fn set_option(engine: &mut MCAmericanEngine<PseudoRandom>, exercise: Shared<dyn Exercise>) {
        let args = (engine.arguments_mut() as &mut dyn Any)
            .downcast_mut::<OptionArguments>()
            .unwrap();
        args.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Put, STRIKE))
            as Shared<dyn StrikedTypePayoff>);
        args.exercise = Some(exercise);
    }

    /// The scaling arithmetic of `mcamericanengine.cpp:48-66` on a fixture where
    /// every wrong wiring lands somewhere else: a K=40 put at a spot of 36 gives
    /// `scaling = 0.025`, `state = 0.9`, and `value = 4`. Dividing rather than
    /// multiplying in `state` gives 1440; evaluating the payoff at the SCALED
    /// state gives 39.1.
    #[test]
    fn the_state_is_scaled_and_the_value_is_not() {
        let pricer = put_pricer(3);
        let p = path([36.0, 44.0, 38.0]);

        assert!((pricer.state(&p, 0) - 0.9).abs() < TOL);
        assert!((pricer.state(&p, 1) - 1.1).abs() < TOL);
        assert!((pricer.value(&p, 0) - 4.0).abs() < TOL);
        assert_eq!(pricer.value(&p, 1), 0.0, "an OTM put is worth 0");
        assert!((pricer.value(&p, 2) - 2.0).abs() < TOL);
    }

    /// The exercise value goes through the same scaling round trip C++ takes
    /// (`mcamericanengine.cpp:58-60`), not a shortcut to `payoff(path[t])`. On a
    /// K=3 put at a spot of 0.89 the round trip is not the identity, so the two
    /// compositions differ in the last bit and the exact compare separates them.
    #[test]
    fn the_exercise_value_round_trips_through_the_scaling() {
        let pricer = AmericanPathPricer::new(
            shared(PlainVanillaPayoff::new(OptionType::Put, 3.0)) as Shared<dyn StrikedTypePayoff>,
            2,
            PolynomialType::Monomial,
        );
        let p = path([0.89, 0.89, 0.89]);

        assert_eq!(pricer.value(&p, 0), 2.1100000000000003);
        assert_ne!(pricer.value(&p, 0), 3.0 - 0.89);
    }

    /// The payoff is APPENDED to the monomials (`mcamericanengine.cpp:45-46`),
    /// so order 3 gives five functions and the last one, fed the SCALED state
    /// 0.9, must report the payoff of the unscaled 36.
    #[test]
    fn the_payoff_is_the_extra_basis_function() {
        for order in [2, 3] {
            let basis = put_pricer(order).basis_system();
            assert_eq!(
                basis.len(),
                order + 2,
                "order {order}: monomials plus payoff"
            );

            assert!((basis[0](0.9) - 1.0).abs() < TOL);
            assert!((basis[1](0.9) - 0.9).abs() < TOL);
            assert!(
                (basis[order + 1](0.9) - 4.0).abs() < TOL,
                "the appended function must unscale before applying the payoff"
            );
            assert_eq!(
                basis[order + 1](1.1),
                0.0,
                "and must stay a payoff, not a monomial"
            );
        }
    }

    /// The exercise guards of `mcamericanengine.hpp:196-198`: a plain American
    /// exercise is accepted, a European one and an American one paying at
    /// expiry are not.
    #[test]
    fn only_a_plain_american_exercise_builds_a_path_pricer() {
        let market = flat_market();
        let expiry = today() + time_to_days(1.0);

        let mut accepted = engine(&market);
        set_option(&mut accepted, american(expiry));
        assert!(accepted.lsm_path_pricer().is_ok());

        let mut rejected = engine(&market);
        set_option(
            &mut rejected,
            shared(EuropeanExercise::new(expiry)) as Shared<dyn Exercise>,
        );
        assert_eq!(
            rejected.lsm_path_pricer().err().unwrap().message(),
            "wrong exercise given"
        );

        let mut at_expiry = engine(&market);
        set_option(
            &mut at_expiry,
            shared(AmericanExercise::new(today(), expiry, true).unwrap()) as Shared<dyn Exercise>,
        );
        assert_eq!(
            at_expiry.lsm_path_pricer().err().unwrap().message(),
            "payoff at expiry not handled"
        );
    }

    /// An engine with nothing set reports the missing exercise before anything
    /// else touches it.
    #[test]
    fn a_missing_exercise_is_rejected() {
        let market = flat_market();
        let mut bare = engine(&market);
        assert_eq!(
            bare.lsm_path_pricer().err().unwrap().message(),
            "no exercise given"
        );

        let args = (bare.arguments_mut() as &mut dyn Any)
            .downcast_mut::<OptionArguments>()
            .unwrap();
        args.exercise = Some(american(today() + time_to_days(1.0)));
        assert_eq!(
            bare.lsm_path_pricer().err().unwrap().message(),
            "no payoff given"
        );
    }

    /// The builder defaults of `mcamericanengine.hpp:130-137`: order 2, no
    /// antithetic variate, and the driver's 2048 calibration samples.
    #[test]
    fn the_builder_defaults_match_the_cpp_factory() {
        let market = flat_market();
        let built = engine(&market);
        assert_eq!(built.polynomial_order(), 2);
        assert_eq!(built.lsm_base().n_calibration_samples(), 2048);
        assert!(!built.lsm_base().antithetic_variate_calibration());

        let overridden = MakeMcAmericanEngine::<PseudoRandom>::new(Shared::clone(&market.process))
            .with_steps(4)
            .with_samples(256)
            .with_polynomial_order(4)
            .with_calibration_samples(64)
            .build()
            .unwrap();
        assert_eq!(overridden.polynomial_order(), 4);
        assert_eq!(overridden.lsm_base().n_calibration_samples(), 64);
    }

    /// The build guards of `mcamericanengine.hpp:296,305,352-355`.
    #[test]
    fn the_builder_validates_its_named_parameters() {
        let market = flat_market();
        let maker = || MakeMcAmericanEngine::<PseudoRandom>::new(Shared::clone(&market.process));

        assert!(maker().with_samples(256).build().is_err(), "no steps given");
        assert!(
            maker()
                .with_steps(4)
                .with_steps_per_year(50)
                .with_samples(256)
                .build()
                .is_err(),
            "steps overspecified"
        );
        assert!(
            maker()
                .with_steps(4)
                .with_samples(256)
                .with_absolute_tolerance(0.02)
                .build()
                .is_err(),
            "samples and tolerance are exclusive"
        );
        let antithetic = maker()
            .with_steps(4)
            .with_samples(256)
            .with_antithetic_variate(true)
            .build()
            .unwrap();
        assert!(
            antithetic.lsm_base().antithetic_variate_calibration(),
            "the pricing flag must reach the engine, not a hardcoded false"
        );
        assert!(
            maker()
                .with_steps(4)
                .with_absolute_tolerance(0.02)
                .with_max_samples(4_096)
                .build()
                .is_ok()
        );
    }
}

#[cfg(test)]
mod oracle {
    //! The American-MC milestone: `test-suite/mclongstaffschwartzengine.cpp:123`
    //! `testAmericanOption`, the `i = 0, j = 0` case.
    //!
    //! Fixture, digit for digit from the C++ (`:127-193`): an American put on a
    //! spot of 36 struck at 36, `q = 0`, `r = 6%`, `sigma = 20%`, evaluation
    //! date 15 May 1998, curve reference 17 May 1998, maturity 17 May 1999,
    //! Actual365Fixed throughout. The engine is
    //! `MakeMCAmericanEngine<PseudoRandom>` with 75 steps, the antithetic
    //! variate, an absolute tolerance of 0.02, seed 42, polynomial order 3 over
    //! the Monomial basis, and the driver's default 2048 calibration paths. The
    //! evaluation date is deliberately NOT the curves' reference date; the C++
    //! fixture is the same way round.
    //!
    //! REFERENCE SUBSTITUTION, stated plainly. The C++ test reprices the option
    //! with `FdBlackScholesVanillaEngine(process, 401, 200)` and checks
    //! `|mc - fd| < 2.34 * errorEstimate` (`:201-206`). This crate's
    //! [`FdBlackScholesVanillaEngine`](super::FdBlackScholesVanillaEngine)
    //! rejects an American exercise (`fdblackscholesvanillaengine.rs:129`), so
    //! the reference here is the MC value QuantLib itself produces on this
    //! fixture, measured on a locally built QuantLib 1.43 dylib:
    //! `mc = 2.054422273006143`, `errorEstimate = 0.01775722870215829`,
    //! `exerciseProbability = 0.4897360703812317`, `fd = 2.08679820123328`.
    //! The band factor 2.34 is the C++ one unchanged.
    //!
    //! The 2.105 that the C++ file records in a comment (`:154`) is NOT usable
    //! as the reference: it is a third-party binomial number that fails the C++
    //! test's own band against QuantLib's own MC value
    //! (`|2.0544 - 2.105| = 0.0506 > 0.0416`), and an independent
    //! Cox-Rubinstein tree at 20001 steps puts the true American price at
    //! 2.08764, agreeing with the FD engine rather than with 2.105.
    //!
    //! Rust and C++ do not share a sample path bit for bit: the generated paths
    //! agree to 14 significant digits (a 75-step accumulation of last-bit
    //! ordering differences), and the near-collinear order-3 regression
    //! amplifies that into a different exercise boundary at only 2048
    //! calibration paths. The two agree where that noise washes out: at 131072
    //! calibration paths Rust prices 2.06774 against C++'s 2.06907, 0.07 error
    //! estimates apart, with exercise probabilities 0.4703 and 0.4712.
    //!
    //! Three independent gates: the price against QuantLib's own value within
    //! the C++ band, the exercise probability against 0.48013 within the C++
    //! 1.5% tolerance (`:154,214`), and the never-early-exercise floor, since
    //! an American put cannot be worth less than its European twin.
    //!
    //! Deferred with the ticket: the other five cases of the parameter grid, the
    //! four other basis families, the Brownian bridge (#453), the
    //! low-discrepancy variant (#454), and the control variate.

    use super::MakeMcAmericanEngine;
    use crate::exercise::{AmericanExercise, EuropeanExercise, Exercise};
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
    use crate::interestrate::Compounding;
    use crate::math::randomnumbers::rngtraits::PseudoRandom;
    use crate::option::OptionType;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::vanilla::AnalyticEuropeanEngine;
    use crate::processes::GeneralizedBlackScholesProcess;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::types::{Rate, Real, Volatility};

    const UNDERLYING: Real = 36.0;
    const DIVIDEND_YIELD: Rate = 0.0;
    const RISK_FREE_RATE: Rate = 0.06;
    const VOLATILITY: Volatility = 0.20;

    const QUANTLIB_MC_VALUE: Real = 2.054422273006143;
    const EXPECTED_EXERCISE_PROBABILITY: Real = 0.48013;

    fn todays_date() -> Date {
        Date::new(15, Month::May, 1998)
    }

    fn settlement_date() -> Date {
        Date::new(17, Month::May, 1998)
    }

    fn maturity() -> Date {
        Date::new(17, Month::May, 1999)
    }

    fn flat_curve(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            settlement_date(),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn process() -> Shared<GeneralizedBlackScholesProcess> {
        let spot = Handle::new(shared(SimpleQuote::new(UNDERLYING)) as Shared<dyn Quote>);
        let vol = Handle::new(shared(BlackConstantVol::new(
            settlement_date(),
            None,
            VOLATILITY,
            Actual365Fixed::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        shared(GeneralizedBlackScholesProcess::new(
            spot,
            flat_curve(DIVIDEND_YIELD),
            flat_curve(RISK_FREE_RATE),
            vol,
        ))
    }

    fn payoff() -> Shared<dyn StrikedTypePayoff> {
        shared(PlainVanillaPayoff::new(OptionType::Put, UNDERLYING))
            as Shared<dyn StrikedTypePayoff>
    }

    #[test]
    fn american_put_reproduces_the_quantlib_price_and_exercise_probability() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(todays_date());
        let process = process();

        let exercise = shared(AmericanExercise::over(settlement_date(), maturity()).unwrap())
            as Shared<dyn Exercise>;
        let mut american = VanillaOption::new(payoff(), exercise, Shared::clone(&settings));
        american.base_mut().set_pricing_engine(shared_mut(
            MakeMcAmericanEngine::<PseudoRandom>::new(Shared::clone(&process))
                .with_steps(75)
                .with_antithetic_variate(true)
                .with_absolute_tolerance(0.02)
                .with_seed(42)
                .with_polynomial_order(3)
                .build()
                .unwrap(),
        ) as SharedMut<dyn PricingEngine>);

        let calculated = american.npv().unwrap();
        let error_estimate = american.error_estimate().unwrap();
        let exercise_probability = american.result::<Real>("exerciseProbability").unwrap();

        let mut european = VanillaOption::new(
            payoff(),
            shared(EuropeanExercise::new(maturity())) as Shared<dyn Exercise>,
            settings,
        );
        european.base_mut().set_pricing_engine(
            shared_mut(AnalyticEuropeanEngine::new(process)) as SharedMut<dyn PricingEngine>
        );
        let european_value = european.npv().unwrap();

        assert!(
            (calculated - QUANTLIB_MC_VALUE).abs() < 2.34 * error_estimate,
            "price {calculated} +/- {error_estimate} misses QuantLib's \
             {QUANTLIB_MC_VALUE} by {}, band {}",
            (calculated - QUANTLIB_MC_VALUE).abs(),
            2.34 * error_estimate
        );
        assert!(
            (exercise_probability - EXPECTED_EXERCISE_PROBABILITY).abs() < 0.015,
            "exercise probability {exercise_probability} vs \
             {EXPECTED_EXERCISE_PROBABILITY}"
        );
        assert!(
            calculated > european_value,
            "an American put {calculated} cannot be worth less than its \
             European twin {european_value}"
        );
    }
}
