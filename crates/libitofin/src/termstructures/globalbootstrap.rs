//! Global piecewise-curve bootstrap.
//!
//! Port of the single-curve core of `ql/termstructures/globalbootstrap.hpp`.
//! Where [`IterativeBootstrap`] solves one node at a time against the curve
//! built so far, `GlobalBootstrap` solves ALL interior node values
//! SIMULTANEOUSLY: it maps every node into an unconstrained optimizer space
//! through the traits' `transform_inverse`, hands the whole vector to a
//! Levenberg-Marquardt least-squares solve whose residuals are the alive
//! helpers' quote errors, and maps the solution back through
//! `transform_direct`.
//!
//! [`IterativeBootstrap`]: crate::termstructures::iterativebootstrap::IterativeBootstrap
//! [`Bootstrap::calculate`]: crate::termstructures::iterativebootstrap::Bootstrap::calculate
//!
//! ## What is ported, and what is deferred
//!
//! Only the single-curve path is ported; it equals the C++ path with every
//! `additional*` argument empty and `parentBootstrapper_` null. Deferred
//! visibly, each as its own follow-up issue referencing #949:
//!
//! - **Additional restrictions** (`additionalHelpers`/`additionalDates`/
//!   `additionalPenalties`/`additionalVariables`, plus the
//!   `SimpleQuoteVariables` helper and the `SimpleZeroYield` traits): the
//!   machinery the three upstream oracle tests
//!   (`piecewiseyieldcurve.cpp:1306/1388/1486`) exercise - futures convexity
//!   adjustments, penalty terms, model variables.
//! - **Multi-curve orchestration** (`MultiCurveBootstrap`,
//!   `MultiCurveBootstrapContributor`, `setParentBootstrapper`/`setToValid`,
//!   the `parentBootstrapper_` branch of `calculate`).
//!
//! The C++ optimizer override (`shared_ptr<OptimizationMethod>`) is not
//! carried either: the default `LevenbergMarquardt(accuracy, accuracy,
//! accuracy)` (`globalbootstrap.hpp:225`) is built per run. The Rust
//! [`OptimizationMethod`](crate::math::optimization::method::OptimizationMethod)
//! `minimize` takes `&mut self`, which a stored trait object cannot offer
//! from the `&self` [`Bootstrap::calculate`]; an override can ride with the
//! deferred additional-restrictions slice if a use case needs it. The
//! `EndCriteria` override is carried (it is `Copy`), defaulting to
//! `EndCriteria(1000, 10, accuracy, accuracy, accuracy)`
//! (`globalbootstrap.hpp:228`) - deliberately different literals from
//! `LocalBootstrap`'s `(100, 10, 0, accuracy, 0)`.
//!
//! ## The C++ method split folds into `calculate`
//!
//! C++ spreads the driver over `setup`/`initialize`/`setupCostFunction`/
//! `setCostFunctionArgument`/`evaluateCostFunction`/`calculate` with mutable
//! members carrying state between them. All of it folds into the single
//! [`Bootstrap::calculate`] here, as for the other bootstrap algorithms:
//!
//! - The `setup()` D1 half, registering the curve as an observer of every
//!   instrument (`globalbootstrap.hpp:217-218`), is performed - for any
//!   bootstrap - by the curve constructor (`PiecewiseYieldCurve::
//!   with_bootstrap`), which registers all instruments unconditionally. Its
//!   guards (the weights check, `:232-236`) run at the start of `calculate`.
//! - `initialize()` re-runs on every calculation. C++ caches it behind
//!   `initialized_` and repeats it only for a moving curve
//!   (`globalbootstrap.hpp:331-332`); the Rust curve is lazily recalculated
//!   only after an invalidation, and re-deriving the grid is idempotent, so
//!   no flag is kept. `ts_->setCalculated(true)` (`:324`) is a multi-curve
//!   artifact - the single-curve lazy flag is already set by the curve's own
//!   `calculate` - and is dropped with the multi-curve deferral.
//! - The `validCurve_` warm-restart flag lives on the curve's node storage
//!   ([`CurveData::is_valid`](crate::termstructures::bootstraptraits::CurveData::is_valid)),
//!   exactly as for [`IterativeBootstrap`]: a still-valid previous solution of
//!   matching size seeds the next solve (`:308-315`), and success marks the
//!   data valid again (`:428`).
//!
//! ## The per-evaluation full-grid rebuild
//!
//! C++ writes the trial nodes into `ts_->data_` and calls
//! `interpolation_.update()` in place (`:385`). The Rust interpolations have
//! no in-place update, so every cost evaluation rebuilds the interpolation
//! over the FULL grid - simpler than `LocalBootstrap`'s seam bookkeeping,
//! because there is no frozen prefix: every node is a variable of the one
//! global solve.
//!
//! ## Traits bound
//!
//! The driver requires
//! [`YieldBootstrapTraits`](crate::termstructures::bootstraptraits::YieldBootstrapTraits)
//! (for the transforms), mirroring the C++ WARNING that `GlobalBootstrap` is
//! known to work with the `Discount`/`ZeroYield`/`ForwardRate` IR traits
//! (`globalbootstrap.hpp:100-103`).

use std::cell::RefCell;

use crate::errors::{QlError, QlResult};
use crate::math::array::Array;
use crate::math::interpolations::Interpolator;
use crate::math::optimization::constraint::NoConstraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::EndCriteria;
use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::require;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::BootstrapHelperShared;
use crate::termstructures::bootstraptraits::{BootstrapTraits, YieldBootstrapTraits};
use crate::termstructures::iterativebootstrap::{Bootstrap, PiecewiseCurve};
use crate::types::{Real, Size};

/// The global bootstrap (`GlobalBootstrap`, single-curve core).
///
/// Carries the stopping-accuracy override, the `EndCriteria` override and the
/// per-instrument residual weights; defaults mirror the C++ constructor
/// (`accuracy = Null`, `endCriteria = nullptr`, `instrumentWeights = {}`,
/// `globalbootstrap.hpp:112-115`), with everything resolved from the curve at
/// calculation time.
#[derive(Clone, Debug, Default)]
pub struct GlobalBootstrap {
    accuracy: Option<Real>,
    end_criteria: Option<EndCriteria>,
    instrument_weights: Vec<Real>,
}

impl GlobalBootstrap {
    /// A global bootstrap with an optional accuracy override, an optional
    /// stopping-criteria override and optional per-instrument weights (empty
    /// weights resolve to 1.0 for every instrument).
    pub fn new(
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
    ) -> GlobalBootstrap {
        GlobalBootstrap {
            accuracy,
            end_criteria,
            instrument_weights,
        }
    }
}

/// The global cost (`setCostFunctionArgument` + `evaluateCostFunction`,
/// `globalbootstrap.hpp:379-403`): writes every interior trial node into the
/// node vector through `transform_direct`, rebuilds the full-grid
/// interpolation, and returns the alive helpers' weighted quote errors as the
/// residual vector.
///
/// The [`CostFunction`] trait is infallible, so a failed rebuild or reprice
/// parks its error in `error` and returns NaN residuals, which the solver
/// adapter treats as an infeasible penalty; the driver surfaces the parked
/// error after the solve (D4), matching the C++ exception propagating out of
/// the cost closure.
struct GlobalCost<'a, C: PiecewiseCurve>
where
    C::Traits: YieldBootstrapTraits,
{
    curve: &'a C,
    alive: &'a [Shared<C::Helper>],
    alive_weights: &'a [Real],
    /// The number of interior nodes, `times.len() - 1`; residual count is
    /// `alive.len()`, at least `interior` (the grid dedup can only shrink it).
    interior: Size,
    error: RefCell<Option<QlError>>,
}

impl<C: PiecewiseCurve> GlobalCost<'_, C>
where
    C::Traits: YieldBootstrapTraits,
{
    fn try_values(&self, x: &Array) -> QlResult<Array> {
        {
            let mut cd = self.curve.curve_data().borrow_mut();
            for i in 0..self.interior {
                let value = C::Traits::transform_direct(x[i]);
                C::Traits::update_guess(cd.data_mut(), value, i + 1);
            }
            cd.rebuild(self.curve.interpolator(), self.interior)?;
        }
        let mut residuals = Array::with_size(self.alive.len());
        for (i, helper) in self.alive.iter().enumerate() {
            residuals[i] = helper.quote_error()? * self.alive_weights[i];
        }
        Ok(residuals)
    }
}

impl<C: PiecewiseCurve> CostFunction for GlobalCost<'_, C>
where
    C::Traits: YieldBootstrapTraits,
{
    fn values(&self, x: &Array) -> Array {
        match self.try_values(x) {
            Ok(values) => values,
            Err(err) => {
                let mut slot = self.error.borrow_mut();
                if slot.is_none() {
                    *slot = Some(err);
                }
                std::iter::repeat_n(Real::NAN, self.alive.len()).collect()
            }
        }
    }
}

impl<C: PiecewiseCurve> Bootstrap<C> for GlobalBootstrap
where
    C::Traits: YieldBootstrapTraits,
{
    fn calculate(&self, curve: &C) -> QlResult<()> {
        let instruments = curve.instruments();
        let n = instruments.len();

        // The C++ setup() guard (`globalbootstrap.hpp:232-236`), surfaced
        // here because the trait has no setup hook.
        require!(
            self.instrument_weights.is_empty() || self.instrument_weights.len() == n,
            "GlobalBootstrap: number of instrument weights ({}) must match number of instruments ({n})",
            self.instrument_weights.len()
        );
        let mut weights = self.instrument_weights.clone();
        weights.resize(n, 1.0);

        // Alive instruments and their weights (`:244-254`): unlike
        // LocalBootstrap there IS a first-alive scan here. It runs over the
        // instrument list in its given order; sorting is not needed because
        // the pillar grid is sorted separately below and each helper carries
        // its own residual.
        let first_date = curve.initial_date()?;
        let mut alive: Vec<Shared<C::Helper>> = Vec::new();
        let mut alive_weights: Vec<Real> = Vec::new();
        for (helper, weight) in instruments.iter().zip(&weights) {
            if helper.pillar_date() > first_date {
                alive.push(Shared::clone(helper));
                alive_weights.push(*weight);
            }
        }

        // The pillar grid (`:280-294`): the first date plus every alive
        // pillar, sorted with duplicates merged - the one dedup in QuantLib's
        // bootstraps. A duplicate pillar leaves the least-squares system
        // overdetermined rather than rejected, unlike IterativeBootstrap.
        let mut dates = Vec::with_capacity(alive.len() + 1);
        dates.push(first_date);
        dates.extend(alive.iter().map(|helper| helper.pillar_date()));
        dates.sort_unstable();
        dates.dedup();

        let required = curve.interpolator().required_points();
        require!(
            dates.len() >= required,
            "GlobalBootstrap: not enough curve points ({}) for interpolation requiring at least {required}",
            dates.len()
        );

        let mut times = Vec::with_capacity(dates.len());
        for date in &dates {
            times.push(curve.time_from_reference(*date)?);
        }

        // maxDate covers every alive helper (`:301-306`).
        let mut max_date = *dates.last().expect("the grid holds the first date");
        for helper in &alive {
            max_date = max_date.max(helper.latest_relevant_date());
        }

        // Install the grid, seeding the nodes from a still-valid previous
        // solution when its shape matches, otherwise resetting to the curve's
        // initial value (`:308-315`).
        let nodes = dates.len();
        let interior = nodes - 1;
        let initial_value = curve.initial_value()?;
        let valid_data = {
            let mut cd = curve.curve_data().borrow_mut();
            let reuse = cd.is_valid() && cd.data().len() == nodes;
            cd.set_pillars(dates, times);
            if !reuse {
                cd.reset_data(initial_value, nodes);
            }
            cd.set_max_date(max_date);
            reuse
        };

        // Hand the curve to each alive helper and reject invalid quotes
        // (`:335-344`).
        let term_structure = curve.term_structure_shared()?;
        for helper in &alive {
            if helper.quote_value().is_err() {
                crate::fail!(
                    "instrument (maturity: {}, pillar: {}) has an invalid quote",
                    helper.maturity_date(),
                    helper.pillar_date()
                );
            }
            helper.set_term_structure(&term_structure);
        }

        // The initial guess (`:360-372`): update each interior node through
        // Traits::guess - which depends on the previously updated nodes, so
        // the writes are sequential - then map it into the optimizer space.
        let mut guess = Array::with_size(interior);
        {
            let mut cd = curve.curve_data().borrow_mut();
            for i in 0..interior {
                let g = C::Traits::guess(i + 1, cd.times(), cd.data(), valid_data);
                C::Traits::update_guess(cd.data_mut(), g, i + 1);
                guess[i] = C::Traits::transform_inverse(cd.data()[i + 1]);
            }
            cd.rebuild(curve.interpolator(), interior)?;
        }

        // Solver configuration (`:222-229`): the LM tolerances and the
        // EndCriteria literals (1000 iterations, three accuracies) are the
        // C++ defaults, distinct from LocalBootstrap's.
        let accuracy = self.accuracy.unwrap_or_else(|| curve.accuracy());
        let mut optimizer = LevenbergMarquardt::new(accuracy, accuracy, accuracy, false);
        let end_criteria = match self.end_criteria {
            Some(criteria) => criteria,
            None => EndCriteria::new(1000, Some(10), accuracy, accuracy, Some(accuracy))?,
        };

        let cost = GlobalCost::<C> {
            curve,
            alive: &alive,
            alive_weights: &alive_weights,
            interior,
            error: RefCell::new(None),
        };
        let no_constraint = NoConstraint;

        let (end_type, solution) = {
            let mut problem = Problem::new(&cost, &no_constraint, guess);
            let outcome = optimizer.minimize(&mut problem, &end_criteria);
            (outcome, problem.current_value().clone())
        };
        if let Some(inner) = cost.error.into_inner() {
            return Err(inner);
        }
        let end_type = end_type?;
        require!(
            end_type.succeeded(),
            "global bootstrap failed to minimize to required accuracy: {end_type}"
        );

        // Pin the returned solution: rewrite every interior node from the
        // optimizer's answer and rebuild, so the curve holds the solution
        // rather than the solver's last trial point; then mark the data as a
        // valid seed for the next bootstrap (`validCurve_ = true`, `:428`).
        {
            let mut cd = curve.curve_data().borrow_mut();
            for i in 0..interior {
                let value = C::Traits::transform_direct(solution[i]);
                C::Traits::update_guess(cd.data_mut(), value, i + 1);
            }
            cd.rebuild(curve.interpolator(), interior)?;
            cd.set_valid(true);
        }
        Ok(())
    }
}
