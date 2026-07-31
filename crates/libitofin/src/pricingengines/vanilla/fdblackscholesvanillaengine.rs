//! Finite-difference pricing engine for vanilla options.
//!
//! Port of `ql/pricingengines/vanilla/fdblackscholesvanillaengine.{hpp,cpp}`,
//! minus everything the FDM sub-umbrella has not reached yet. The engine lays
//! a `ln(S)` grid over the process, seeds it with the payoff, rolls it back
//! through [`FdmBlackScholesSolver`] and reads value, delta, gamma and theta
//! off the rolled grid at the spot (`cpp:109-215`).
//!
//! Deferred to #636, and omitted rather than accepted and ignored:
//!
//! - early exercise. C++ builds its step conditions through
//!   `FdmStepConditionComposite::vanillaComposite` (`cpp:186-191`), which grows
//!   an `FdmAmericanStepCondition` or an `FdmBermudanStepCondition` when the
//!   exercise is not European; neither is ported, so a non-European exercise is
//!   an explicit error here rather than a silently European price;
//! - cash dividends, both the `Spot` and the `Escrowed` model (`cpp:111-152`,
//!   `cpp:172-184`). The dividend schedule is always empty and the spot
//!   adjustment always zero, as on the C++ default path;
//! - the quanto branch (`cpp:163`) and the local-volatility branch
//!   (`cpp:206`), which neither
//!   [`fdm_black_scholes_mesher`] nor [`FdmBlackScholesSolver`] carries;
//! - the `MakeFdBlackScholesVanillaEngine` builder and the three extra
//!   constructors (`hpp:62-95`).

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::fail;
use crate::instruments::{Greeks, OneAssetOptionEngine, OneAssetOptionResults, OptionArguments};
use crate::methods::finitedifferences::meshers::{
    FdmMesher, FdmMesherComposite, fdm_black_scholes_mesher,
};
use crate::methods::finitedifferences::solvers::{
    FdmBlackScholesSolver, FdmSchemeDesc, FdmSolverDesc,
};
use crate::methods::finitedifferences::stepconditions::FdmStepConditionComposite;
use crate::methods::finitedifferences::utilities::{FdmInnerValueCalculator, fdm_log_inner_value};
use crate::patterns::observable::{AsObservable, Observable};
use crate::payoff::Payoff;
use crate::pricingengine::{Arguments, PricingEngine, Results};
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::Size;

/// The direction the equity grid is laid out in (`cpp:171`).
const DIRECTION: Size = 0;

/// The truncation probability of the grid bounds (`cpp:161`).
const EPS: f64 = 0.0001;

/// The factor the grid bounds are widened by (`cpp:161`).
const SCALE_FACTOR: f64 = 1.5;

/// The density of the concentration around the strike (`cpp:162`).
const C_POINT_DENSITY: f64 = 0.1;

/// Finite-difference pricing engine for European vanilla options.
///
/// Everything the engine builds - mesher, calculator, conditions, solver -
/// lives inside a single [`calculate`](PricingEngine::calculate), as in C++
/// (`cpp:109-215`); the constructor only records the grid sizes and the scheme.
pub struct FdBlackScholesVanillaEngine {
    base: OneAssetOptionEngine,
    process: Shared<GeneralizedBlackScholesProcess>,
    t_grid: Size,
    x_grid: Size,
    damping_steps: Size,
    scheme_desc: FdmSchemeDesc,
}

impl FdBlackScholesVanillaEngine {
    /// The engine pricing off `process` over a `t_grid` by `x_grid` mesh, with
    /// `damping_steps` implicit-Euler steps taken first and the rollback run
    /// under `scheme_desc`.
    ///
    /// C++ defaults the last three to `0`, `0` and `FdmSchemeDesc::Douglas()`
    /// (`hpp:52-60`); the crate has no default arguments, so all four are named
    /// at the call site.
    ///
    /// The registration with the process is load-bearing rather than
    /// decorative (`cpp:104`): the instrument caches its results behind a
    /// calculated flag, so without it a change to any quote the process reads
    /// would leave a stale price in place.
    pub fn new(
        process: Shared<GeneralizedBlackScholesProcess>,
        t_grid: Size,
        x_grid: Size,
        damping_steps: Size,
        scheme_desc: FdmSchemeDesc,
    ) -> FdBlackScholesVanillaEngine {
        let base =
            OneAssetOptionEngine::new(OptionArguments::default(), OneAssetOptionResults::default());
        base.register_with(process.observable());
        FdBlackScholesVanillaEngine {
            base,
            process,
            t_grid,
            x_grid,
            damping_steps,
            scheme_desc,
        }
    }
}

impl AsObservable for FdBlackScholesVanillaEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for FdBlackScholesVanillaEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        let arguments = self.base.arguments();
        let Some(exercise) = &arguments.exercise else {
            fail!("no exercise given");
        };
        if exercise.exercise_type() != ExerciseType::European {
            fail!("early exercise is not supported by the finite-difference engine yet");
        }
        let Some(payoff) = &arguments.payoff else {
            fail!("no payoff given");
        };
        let payoff = Shared::clone(payoff);
        let exercise_date = exercise.last_date();

        let maturity = self.process.time(&exercise_date)?;
        let strike = payoff.strike();

        let equity_mesher = fdm_black_scholes_mesher(
            self.x_grid,
            &self.process,
            maturity,
            strike,
            None,
            None,
            EPS,
            SCALE_FACTOR,
            Some((strike, C_POINT_DENSITY)),
            &[],
            0.0,
        )?;
        let mesher = shared(FdmMesherComposite::new(vec![equity_mesher])) as Shared<dyn FdmMesher>;

        let calculator = shared(fdm_log_inner_value(
            payoff as Shared<dyn Payoff>,
            Shared::clone(&mesher),
            DIRECTION,
        )) as Shared<dyn FdmInnerValueCalculator>;

        let solver_desc = FdmSolverDesc {
            mesher,
            bc_set: Vec::new(),
            condition: shared(FdmStepConditionComposite::new(&[], Vec::new())),
            calculator,
            maturity,
            time_steps: self.t_grid,
            damping_steps: self.damping_steps,
        };
        let solver = FdmBlackScholesSolver::new(
            Shared::clone(&self.process),
            strike,
            solver_desc,
            self.scheme_desc,
        );

        let spot = self.process.x0()?;
        let value = solver.value_at(spot)?;
        let delta = solver.delta_at(spot)?;
        let gamma = solver.gamma_at(spot)?;
        let theta = solver.theta_at(spot)?;

        let results = self.base.results_mut();
        results.instrument.value = Some(value);
        results.greeks = Greeks {
            delta: Some(delta),
            gamma: Some(gamma),
            theta,
            ..Greeks::default()
        };
        Ok(())
    }
}

#[cfg(test)]
mod test_fd_engines {
    //! The `testFdEngines` oracle of `test-suite/europeanoption.cpp:1241-1256`,
    //! which runs the shared `testEngineConsistency` harness (`:192-278`) with
    //! the finite-difference engine on a 500 by 500 grid and checks it against
    //! the analytic engine over the full market sweep.

    use std::time::Instant;

    use super::super::test_market::{market, today};
    use super::FdBlackScholesVanillaEngine;
    use crate::exercise::{Exercise, ExerciseType};
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, OneAssetOption, PlainVanillaPayoff};
    use crate::methods::finitedifferences::solvers::FdmSchemeDesc;
    use crate::option::OptionType::{self, Call, Put};
    use crate::pricingengine::PricingEngine;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::time::date::Date;
    use crate::types::{Rate, Real, Size, Volatility};

    /// `timeSteps` and `gridPoints` of `europeanoption.cpp:1246-1247`, which
    /// `makeOption` passes as the engine's `tGrid` and `xGrid` (`:158-163`).
    const T_GRID: Size = 500;
    const X_GRID: Size = 500;

    /// The single underlying of the harness (`:206`), and the reference the
    /// relative errors are taken against (`:264`).
    const UNDERLYING: Real = 100.0;

    /// `relativeTol` of `europeanoption.cpp:1249-1252`.
    const VALUE_TOLERANCE: Real = 1.0e-4;
    const DELTA_TOLERANCE: Real = 1.0e-6;
    const GAMMA_TOLERANCE: Real = 1.0e-6;
    const THETA_TOLERANCE: Real = 1.0e-3;

    /// `relativeError` of `test-suite/utilities.cpp:132-138`: the difference
    /// scaled by a reference that is the underlying here, not the quantity
    /// being compared.
    fn relative_error(x1: Real, x2: Real, reference: Real) -> Real {
        if reference != 0.0 {
            (x1 - x2).abs() / reference
        } else {
            (x1 - x2).abs()
        }
    }

    /// The option under test: the same payoff and exercise as the reference,
    /// on the same process, priced by the finite-difference engine
    /// (`europeanoption.cpp:158-163`).
    fn fd_option(
        market: &super::super::test_market::Market,
        option_type: OptionType,
        strike: Real,
        expiry: Date,
    ) -> EuropeanOption {
        let payoff = shared(PlainVanillaPayoff::new(option_type, strike));
        let exercise = shared(crate::exercise::EuropeanExercise::new(expiry));
        let mut option = EuropeanOption::new(payoff, exercise, Shared::clone(&market.settings));
        let engine = shared_mut(FdBlackScholesVanillaEngine::new(
            Shared::clone(&market.process),
            T_GRID,
            X_GRID,
            0,
            FdmSchemeDesc::douglas(),
        ));
        option
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        option
    }

    /// The C++ harness builds both options once per (type, strike) pair and
    /// mutates the four shared quotes underneath them (`:230-247`), so the
    /// eighteen repricings of each pair run through the observer chain rather
    /// than through fresh instruments. Rebuilding the option inside the quote
    /// loops would price correctly whether or not the engine registers with
    /// the process, and the registration is exactly what this sweep pins.
    #[test]
    fn fd_engine_matches_the_analytic_engine_over_the_market_sweep() {
        let started = Instant::now();
        let market = market();
        let expiry = today() + 360;

        let q_rates: [Rate; 2] = [0.00, 0.05];
        let r_rates: [Rate; 3] = [0.01, 0.05, 0.15];
        let vols: [Volatility; 3] = [0.11, 0.50, 1.20];

        let mut worst = [
            ("value", 0.0),
            ("delta", 0.0),
            ("gamma", 0.0),
            ("theta", 0.0),
        ];

        for option_type in [Call, Put] {
            for strike in [75.0, 100.0, 125.0] {
                let mut reference = market.option(option_type, strike, expiry);
                let mut option = fd_option(&market, option_type, strike, expiry);

                for q in q_rates {
                    for r in r_rates {
                        for vol in vols {
                            market.set(UNDERLYING, q, r, vol);

                            let value = option.npv().unwrap();
                            let mut checks =
                                vec![("value", reference.npv().unwrap(), value, VALUE_TOLERANCE)];
                            if value > UNDERLYING * 1.0e-5 {
                                checks.push((
                                    "delta",
                                    reference.delta().unwrap(),
                                    option.delta().unwrap(),
                                    DELTA_TOLERANCE,
                                ));
                                checks.push((
                                    "gamma",
                                    reference.gamma().unwrap(),
                                    option.gamma().unwrap(),
                                    GAMMA_TOLERANCE,
                                ));
                                checks.push((
                                    "theta",
                                    reference.theta().unwrap(),
                                    option.theta().unwrap(),
                                    THETA_TOLERANCE,
                                ));
                            }

                            for (name, expected, calculated, tolerance) in checks {
                                let error = relative_error(expected, calculated, UNDERLYING);
                                assert!(
                                    error <= tolerance,
                                    "{name} of {option_type:?} K={strike} q={q} r={r} v={vol}: \
                                     analytic {expected} vs finite difference {calculated} \
                                     (relative error {error} over {tolerance})"
                                );
                                for slot in &mut worst {
                                    if slot.0 == name && error > slot.1 {
                                        slot.1 = error;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        println!(
            "testFdEngines: {:?} for 108 combinations; worst relative errors {worst:?}",
            started.elapsed()
        );
    }

    /// The visible half of the early-exercise deferral: C++ prices an American
    /// exercise through `vanillaComposite` (`cpp:186-191`), and this port says
    /// so rather than quietly returning the European price.
    #[test]
    fn early_exercise_is_rejected_rather_than_priced_as_european() {
        struct AmericanStub {
            dates: [Date; 1],
        }
        impl Exercise for AmericanStub {
            fn exercise_type(&self) -> ExerciseType {
                ExerciseType::American
            }
            fn dates(&self) -> &[Date] {
                &self.dates
            }
        }

        let market = market();
        market.set(UNDERLYING, 0.00, 0.05, 0.20);
        let expiry = today() + 360;
        let european = fd_option(&market, Call, 100.0, expiry);

        let mut american = OneAssetOption::new(
            Shared::clone(european.payoff()),
            shared(AmericanStub { dates: [expiry] }) as Shared<dyn Exercise>,
            Shared::clone(&market.settings),
        );
        american
            .base_mut()
            .set_pricing_engine(european.base().pricing_engine().unwrap().clone());

        assert_eq!(
            american.npv().unwrap_err().message(),
            "early exercise is not supported by the finite-difference engine yet"
        );
    }
}

#[cfg(test)]
mod test_fd_engine_with_non_constant_parameters {
    //! The `testFdEngineWithNonConstantParameters` oracle of
    //! `test-suite/europeanoption.cpp:1578-1631`: the only European arm whose
    //! risk-free curve is not flat, and so the only one that pins the
    //! per-step forward reads of `set_time` to intervals that telescope to
    //! the right integrated rate. The flat sweep above cannot: every interval
    //! of a flat curve returns the same forward. What survives here is a read
    //! over a fixed short interval, which integrates too little rate over the
    //! year; a read over the whole life still integrates correctly and this
    //! oracle does not separate it from the correct one.

    use super::super::AnalyticEuropeanEngine;
    use super::super::test_market::today;
    use super::FdBlackScholesVanillaEngine;
    use crate::exercise::EuropeanExercise;
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
    use crate::interestrate::Compounding;
    use crate::math::interpolations::flat::BackwardFlat;
    use crate::methods::finitedifferences::solvers::FdmSchemeDesc;
    use crate::option::OptionType::Call;
    use crate::pricingengine::PricingEngine;
    use crate::processes::GeneralizedBlackScholesProcess;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::{FlatForward, ForwardCurve};
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::types::{Real, Size, Volatility};

    /// `u` and `v` of `cpp:1582-1583`; the strike of `cpp:1610` is the spot.
    const UNDERLYING: Real = 190.0;
    const STRIKE: Real = 190.0;
    const VOLATILITY: Volatility = 0.20;

    /// `timeSteps` and `gridPoints` of `cpp:1617-1618`.
    const T_GRID: Size = 200;
    const X_GRID: Size = 201;

    /// `tolerance` of `cpp:1623`, absolute on the price rather than the
    /// relative measure the sweep uses.
    const TOLERANCE: Real = 0.01;

    /// The process of `cpp:1602-1605`. C++ names `BlackScholesProcess`, whose
    /// constructor supplies the dividend yield the generalized process needs
    /// as a flat zero `FlatForward` on Actual/365 Fixed
    /// (`ql/processes/blackscholesprocess.cpp:238-239`); with a zero rate the
    /// day counter is numerically inert.
    fn process() -> GeneralizedBlackScholesProcess {
        let day_counter = Actual360::new();
        let spot = shared(SimpleQuote::new(UNDERLYING)) as Shared<dyn Quote>;
        let vol = shared(BlackConstantVol::new(
            today(),
            None,
            VOLATILITY,
            day_counter.clone(),
        )) as Shared<dyn BlackVolTermStructure>;

        let dates = vec![
            today(),
            today() + 90,
            today() + 180,
            today() + 270,
            today() + 360,
        ];
        let forwards = vec![0.0, 0.001, 0.002, 0.005, 0.01];
        let risk_free =
            shared(ForwardCurve::new(dates, forwards, day_counter.clone(), BackwardFlat).unwrap())
                as Shared<dyn YieldTermStructure>;

        let dividend = shared(FlatForward::with_rate(
            today(),
            0.0,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>;

        GeneralizedBlackScholesProcess::new(
            Handle::new(spot),
            Handle::new(dividend),
            Handle::new(risk_free),
            Handle::new(vol),
        )
    }

    /// The forward the curve carries rises from 0.001 to 0.01 across the
    /// year, so a `set_time` pinned to a fixed short interval integrates too
    /// little rate and the price misses by around thirty times the tolerance.
    #[test]
    fn fd_engine_matches_the_analytic_engine_under_a_time_varying_rate() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let process = shared(process());

        let payoff = shared(PlainVanillaPayoff::new(Call, STRIKE));
        let exercise = shared(EuropeanExercise::new(today() + 360));
        let mut option = EuropeanOption::new(payoff, exercise, Shared::clone(&settings));

        let analytic = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&process)));
        option
            .base_mut()
            .set_pricing_engine(analytic as SharedMut<dyn PricingEngine>);
        let expected = option.npv().unwrap();

        let finite_difference = shared_mut(FdBlackScholesVanillaEngine::new(
            Shared::clone(&process),
            T_GRID,
            X_GRID,
            0,
            FdmSchemeDesc::douglas(),
        ));
        option
            .base_mut()
            .set_pricing_engine(finite_difference as SharedMut<dyn PricingEngine>);
        let calculated = option.npv().unwrap();

        let error = (expected - calculated).abs();
        assert!(
            error <= TOLERANCE,
            "analytic {expected} vs finite difference {calculated} \
             (absolute error {error} over {TOLERANCE})"
        );
    }
}
