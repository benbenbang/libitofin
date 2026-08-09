//! Finite-difference pricing engine for vanilla options.
//!
//! Port of `ql/pricingengines/vanilla/fdblackscholesvanillaengine.{hpp,cpp}`,
//! minus everything the FDM sub-umbrella has not reached yet. The engine lays
//! a `ln(S)` grid over the process, seeds it with the payoff, rolls it back
//! through [`FdmBlackScholesSolver`] under the step conditions of
//! [`FdmStepConditionComposite::vanilla_composite`] and reads value, delta,
//! gamma and theta off the rolled grid at the spot (`cpp:109-215`).
//!
//! Deferred to #636, and omitted rather than accepted and ignored:
//!
//! - cash dividends, both the `Spot` and the `Escrowed` model (`cpp:111-152`,
//!   `cpp:172-184`). The dividend schedule is always empty and the spot
//!   adjustment always zero, as on the C++ default path;
//! - the quanto branch (`cpp:163`) and the local-volatility branch
//!   (`cpp:206`), which neither
//!   [`fdm_black_scholes_mesher`] nor [`FdmBlackScholesSolver`] carries;
//! - the `MakeFdBlackScholesVanillaEngine` builder and the three extra
//!   constructors (`hpp:62-95`).

use crate::errors::QlResult;
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

/// Finite-difference pricing engine for European, American and Bermudan
/// vanilla options.
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
        let exercise = Shared::clone(exercise);
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

        let risk_free = self.process.risk_free_rate().current_link()?;
        let Some(day_counter) = risk_free.day_counter() else {
            fail!("no day counter provided for the risk-free curve");
        };
        let condition = FdmStepConditionComposite::vanilla_composite(
            &exercise,
            Shared::clone(&mesher),
            Shared::clone(&calculator),
            risk_free.reference_date()?,
            &day_counter,
        )?;

        let solver_desc = FdmSolverDesc {
            mesher,
            bc_set: Vec::new(),
            condition,
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
    use crate::instrument::Instrument;
    use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
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
}

#[cfg(test)]
mod test_fd_values {
    //! The first half of the `testFdValues` oracle of
    //! `test-suite/americanoption.cpp:375-427`: American options priced on a
    //! 100 by 400 grid against the Ju-1998 table (`:256-322`) within 8e-2
    //! (`:389`). The second assertion of the C++ case (`:428-443`) compares
    //! the same values against the QR+ boundary-approximation engine, which is
    //! a different engine and out of scope here.
    //!
    //! The long-dated dividend-paying calls are what make this oracle
    //! discriminate: their early-exercise premium is around one, so an engine
    //! that rolled back under an empty condition list and returned the
    //! European price would miss by more than ten tolerances. The short-dated
    //! puts alone would not separate the two.

    use super::super::test_market::{Market, market, time_to_days, today};
    use super::FdBlackScholesVanillaEngine;
    use crate::exercise::{AmericanExercise, Exercise};
    use crate::instrument::Instrument;
    use crate::instruments::{OneAssetOption, PlainVanillaPayoff};
    use crate::methods::finitedifferences::solvers::FdmSchemeDesc;
    use crate::option::OptionType::{self, Call, Put};
    use crate::pricingengine::PricingEngine;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::types::{Rate, Real, Size, Time, Volatility};

    /// The grid of the `pdeEngine` of `:399`.
    const T_GRID: Size = 100;
    const X_GRID: Size = 400;

    /// `tolerance` of `:389`.
    const TOLERANCE: Real = 8.0e-2;

    /// One row of the Ju table (`:256-322`).
    struct JuValue {
        option_type: OptionType,
        strike: Real,
        spot: Real,
        q: Rate,
        r: Rate,
        t: Time,
        vol: Volatility,
        expected: Real,
    }

    /// The maturity of `:407`: the year fraction rounded to whole days on the
    /// 360-day year the market's Actual/360 curves count on, not the year
    /// fraction itself (`test-suite/utilities.hpp:141-143`).
    fn price(market: &Market, ju: &JuValue) -> Real {
        market.set(ju.spot, ju.q, ju.r, ju.vol);

        let exercise = AmericanExercise::over(today(), today() + time_to_days(ju.t)).unwrap();
        let mut option = OneAssetOption::new(
            shared(PlainVanillaPayoff::new(ju.option_type, ju.strike)),
            shared(exercise) as Shared<dyn Exercise>,
            Shared::clone(&market.settings),
        );
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
        option.npv().unwrap()
    }

    #[test]
    fn american_options_reproduce_the_ju_values() {
        let market = market();
        let rows = [
            JuValue {
                option_type: Put,
                strike: 40.0,
                spot: 40.0,
                q: 0.0,
                r: 0.0488,
                t: 0.3333,
                vol: 0.2,
                expected: 1.576,
            },
            JuValue {
                option_type: Put,
                strike: 45.0,
                spot: 40.0,
                q: 0.0,
                r: 0.0488,
                t: 0.5833,
                vol: 0.2,
                expected: 5.260,
            },
            JuValue {
                option_type: Call,
                strike: 100.0,
                spot: 100.0,
                q: 0.07,
                r: 0.03,
                t: 3.0,
                vol: 0.2,
                expected: 9.065,
            },
            JuValue {
                option_type: Call,
                strike: 100.0,
                spot: 120.0,
                q: 0.07,
                r: 0.03,
                t: 3.0,
                vol: 0.2,
                expected: 21.398,
            },
        ];

        for ju in &rows {
            let calculated = price(&market, ju);
            let error = (calculated - ju.expected).abs();
            println!(
                "testFdValues: {:?} K={} S={} t={}: Ju {} finite difference {calculated}",
                ju.option_type, ju.strike, ju.spot, ju.t, ju.expected
            );
            assert!(
                error <= TOLERANCE,
                "{:?} K={} S={} q={} r={} t={} v={}: Ju {} vs finite difference {calculated} \
                 (absolute error {error} over {TOLERANCE})",
                ju.option_type,
                ju.strike,
                ju.spot,
                ju.q,
                ju.r,
                ju.t,
                ju.vol,
                ju.expected
            );
        }
    }
}

#[cfg(test)]
mod test_fd_earliest_exercise_date {
    //! The `testFdEarliestExerciseDate` oracle of
    //! `test-suite/americanoption.cpp:2173-2255`: a deep in-the-money put whose
    //! exercise window is narrowed from the front.
    //!
    //! This is the only oracle that drives a non-zero `exercise_start`.
    //! `testFdValues` opens every window at the reference date, so the
    //! early-return of `FdmAmericanStepCondition::apply_to`
    //! (`fdmamericanstepcondition.cpp:37-38`) never fires there and an engine
    //! ignoring the earliest exercise date would pass it.

    use super::super::AnalyticEuropeanEngine;
    use super::FdBlackScholesVanillaEngine;
    use crate::exercise::{AmericanExercise, EuropeanExercise, Exercise};
    use crate::handle::Handle;
    use crate::instrument::Instrument;
    use crate::instruments::{OneAssetOption, PlainVanillaPayoff};
    use crate::interestrate::Compounding;
    use crate::methods::finitedifferences::solvers::FdmSchemeDesc;
    use crate::option::OptionType::Put;
    use crate::pricingengine::PricingEngine;
    use crate::processes::{BlackScholesMertonProcess, GeneralizedBlackScholesProcess};
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Rate, Real, Size, Volatility};

    /// The market of `:2186-2195`.
    const S0: Real = 80.0;
    const STRIKE: Real = 100.0;
    const SIGMA: Volatility = 0.25;
    const R: Rate = 0.05;
    const Q: Rate = 0.0;

    /// The grid of `:2206`.
    const T_GRID: Size = 200;
    const X_GRID: Size = 200;

    fn today() -> Date {
        Date::new(15, Month::January, 2025)
    }

    fn flat_rate(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            today(),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn process() -> Shared<GeneralizedBlackScholesProcess> {
        let spot = Handle::new(shared(SimpleQuote::new(S0)) as Shared<dyn Quote>);
        let vol = Handle::new(shared(BlackConstantVol::new(
            today(),
            None,
            SIGMA,
            Actual365Fixed::new(),
        )) as Shared<dyn BlackVolTermStructure>);
        shared(BlackScholesMertonProcess::new(
            spot,
            flat_rate(Q),
            flat_rate(R),
            vol,
        ))
    }

    /// The American put over `[earliest, maturity]`, priced by the
    /// finite-difference engine (`:2203-2208`).
    fn american_price(
        settings: &Shared<Settings<Date>>,
        process: &Shared<GeneralizedBlackScholesProcess>,
        earliest: Date,
        maturity: Date,
    ) -> Real {
        let exercise = AmericanExercise::over(earliest, maturity).unwrap();
        let mut option = OneAssetOption::new(
            shared(PlainVanillaPayoff::new(Put, STRIKE)),
            shared(exercise) as Shared<dyn Exercise>,
            Shared::clone(settings),
        );
        let engine = shared_mut(FdBlackScholesVanillaEngine::new(
            Shared::clone(process),
            T_GRID,
            X_GRID,
            0,
            FdmSchemeDesc::douglas(),
        ));
        option
            .base_mut()
            .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
        option.npv().unwrap()
    }

    #[test]
    fn narrowing_the_exercise_window_lowers_the_price_toward_the_european_one() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let process = process();
        let maturity = today() + Period::new(1, TimeUnit::Years);

        let full = american_price(&settings, &process, today(), maturity);
        let mid = american_price(
            &settings,
            &process,
            maturity - Period::new(6, TimeUnit::Months),
            maturity,
        );
        let late = american_price(
            &settings,
            &process,
            maturity - Period::new(3, TimeUnit::Months),
            maturity,
        );

        let mut european = OneAssetOption::new(
            shared(PlainVanillaPayoff::new(Put, STRIKE)),
            shared(EuropeanExercise::new(maturity)) as Shared<dyn Exercise>,
            Shared::clone(&settings),
        );
        let analytic = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&process)));
        european
            .base_mut()
            .set_pricing_engine(analytic as SharedMut<dyn PricingEngine>);
        let euro = european.npv().unwrap();

        println!("testFdEarliestExerciseDate: full {full} 6M {mid} 3M {late} european {euro}");

        assert!(
            full - euro > 1.0,
            "the early-exercise premium should be significant: full {full} european {euro}"
        );
        assert!(
            full - late > 0.01,
            "restricting the exercise window should reduce the price: full {full} late {late}"
        );
        assert!(
            late > euro + 0.01,
            "the restricted American should exceed the European: late {late} european {euro}"
        );
        assert!(
            mid >= late - 1e-8,
            "a wider window should give a higher price: 6M {mid} 3M {late}"
        );
        assert!(
            full >= mid - 1e-8,
            "the full window should give the highest price: full {full} 6M {mid}"
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
