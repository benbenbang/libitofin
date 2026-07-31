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

/// The direction the equity grid is laid out in (`cpp:165`, `cpp:171`).
const DIRECTION: Size = 0;

/// The truncation probability of the grid bounds (`cpp:162`).
const EPS: f64 = 0.0001;

/// The factor the grid bounds are widened by (`cpp:162`).
const SCALE_FACTOR: f64 = 1.5;

/// The density of the concentration around the strike (`cpp:163`).
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
