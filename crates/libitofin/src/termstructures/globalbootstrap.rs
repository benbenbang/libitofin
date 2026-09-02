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
//! [`Bootstrap::additional_observables`]: crate::termstructures::iterativebootstrap::Bootstrap::additional_observables
//!
//! ## What is ported, and what is deferred
//!
//! The single-curve path is ported, together with the `additionalPenalties`
//! residual terms (#974) and the `additionalHelpers`/`additionalDates`
//! restrictions (#976); it equals the C++ path with the remaining
//! `additional*` argument empty and `parentBootstrapper_` null. Deferred
//! visibly, each as its own follow-up issue referencing #949:
//!
//! - **Additional variables** (`additionalVariables`, plus the
//!   `SimpleQuoteVariables` helper): the extra optimizer coordinates the
//!   futures-convexity oracle test (`piecewiseyieldcurve.cpp:1486`) exercises.
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
//! deferred additional-variables slice if a use case needs it. The
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
//!   with_bootstrap`), which registers all instruments unconditionally. The
//!   additional helpers of `:219-220` reach the same loop through
//!   [`Bootstrap::additional_observables`], the trait hook a bootstrap owning
//!   helpers of its own overrides. Its guards (the weights check, `:232-236`)
//!   run at the start of `calculate`.
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
//! ## Where the penalty terms run
//!
//! C++ splits the trial write (`setCostFunctionArgument`, `:379-390`) from the
//! residual assembly (`evaluateCostFunction`, `:392-403`), so the penalty
//! closure runs with no mutable state alive. This port keeps that separation
//! deliberately: the penalty is invoked only after the `borrow_mut` that
//! rewrites the nodes has dropped. A penalty may read the curve back - the
//! upstream one reprices an additional helper - which takes a shared borrow of
//! the same `RefCell`, and a live `borrow_mut` would panic there.
//!
//! ## Traits bound
//!
//! The driver requires
//! [`YieldBootstrapTraits`](crate::termstructures::bootstraptraits::YieldBootstrapTraits)
//! (for the transforms), mirroring the C++ WARNING that `GlobalBootstrap` is
//! known to work with the `Discount`/`ZeroYield`/`ForwardRate` IR traits
//! (`globalbootstrap.hpp:100-103`).

use std::cell::{Cell, RefCell};

use crate::errors::{QlError, QlResult};
use crate::math::array::Array;
use crate::math::interpolations::Interpolator;
use crate::math::optimization::constraint::NoConstraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::EndCriteria;
use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::patterns::observable::Observable;
use crate::require;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::{BootstrapHelperShared, RateHelper};
use crate::termstructures::bootstraptraits::{BootstrapTraits, YieldBootstrapTraits};
use crate::termstructures::iterativebootstrap::{Bootstrap, PiecewiseCurve};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::{Real, Size, Time};

/// The additional penalty terms (`AdditionalPenalties`,
/// `globalbootstrap.hpp:108-109`).
///
/// Extra least-squares residuals, appended after the alive helpers' weighted
/// quote errors. The closure is handed the FULL node grid of the trial curve -
/// times and values INCLUDING node 0 (`hpp:395`) - not the interior slice the
/// optimizer varies, so a penalty over `times.len() - 1` differences indexes
/// `data[i + 1] - data[i]` directly.
pub type AdditionalPenalties = dyn Fn(&[Time], &[Real]) -> Vec<Real>;

/// The additional node dates (`additionalDates`, `globalbootstrap.hpp:117`).
///
/// Extra grid dates, re-read on every calculation because the upstream functor
/// is evaluation-date relative (`hpp:266-267`). Each surviving date is one more
/// interior node - one more free variable of the solve - carrying no residual
/// of its own, so a system made under-determined this way needs the
/// [`AdditionalPenalties`] terms to stay solvable. Dates at or before the first
/// curve date are dropped (`hpp:268-274`).
pub type AdditionalDates = dyn Fn() -> Vec<Date>;

/// The global bootstrap (`GlobalBootstrap`, single-curve core).
///
/// Carries the stopping-accuracy override, the `EndCriteria` override, the
/// per-instrument residual weights and the additional penalty terms; defaults
/// mirror the C++ constructor (`accuracy = Null`, `endCriteria = nullptr`,
/// `instrumentWeights = {}`, no penalties, `globalbootstrap.hpp:112-115`), with
/// everything resolved from the curve at calculation time.
///
/// The boxed penalty closure is neither `Clone` nor `Debug`, so those two
/// derives are gone; `Default` is hand-rolled because a curve built through
/// [`PiecewiseYieldCurve::new`](crate::termstructures::yields::PiecewiseYieldCurve::new)
/// still needs the empty configuration.
pub struct GlobalBootstrap {
    additional_helpers: Vec<Shared<dyn RateHelper>>,
    additional_dates: Option<Box<AdditionalDates>>,
    accuracy: Option<Real>,
    end_criteria: Option<EndCriteria>,
    instrument_weights: Vec<Real>,
    penalties: Option<Box<AdditionalPenalties>>,
}

impl Default for GlobalBootstrap {
    fn default() -> GlobalBootstrap {
        GlobalBootstrap::new(None, None, Vec::new())
    }
}

impl GlobalBootstrap {
    /// A global bootstrap with an optional accuracy override, an optional
    /// stopping-criteria override and optional per-instrument weights (empty
    /// weights resolve to 1.0 for every instrument), and no penalty terms
    /// (C++ constructor #1, `globalbootstrap.hpp:112-115`).
    pub fn new(
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
    ) -> GlobalBootstrap {
        GlobalBootstrap {
            additional_helpers: Vec::new(),
            additional_dates: None,
            accuracy,
            end_criteria,
            instrument_weights,
            penalties: None,
        }
    }

    /// The additional-restrictions constructor (C++ constructor #2,
    /// `globalbootstrap.hpp:116-123`, definition `:169-186`): the
    /// [`AdditionalPenalties`] over the trial curve's node grid, plus the
    /// `additionalHelpers` the penalty reprices and the [`AdditionalDates`] the
    /// grid gains.
    ///
    /// The additional helpers are handed the curve and registered with it, but
    /// contribute neither a pillar date nor a residual: they exist so a penalty
    /// can read their `implied_quote`. Its `additionalVariables` argument is
    /// still deferred.
    pub fn with_penalties<F>(
        additional_helpers: Vec<Shared<dyn RateHelper>>,
        additional_dates: Option<Box<AdditionalDates>>,
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
        penalties: F,
    ) -> GlobalBootstrap
    where
        F: Fn(&[Time], &[Real]) -> Vec<Real> + 'static,
    {
        GlobalBootstrap {
            additional_helpers,
            additional_dates,
            accuracy,
            end_criteria,
            instrument_weights,
            penalties: Some(Box::new(penalties)),
        }
    }

    /// The same, for a penalty that does not read the node grid (C++
    /// constructor #3's `std::function<Array()>` form,
    /// `globalbootstrap.hpp:124-131`, definition `:196-206`, which wraps the
    /// no-argument closure into the two-argument one and delegates).
    pub fn with_grid_independent_penalties<F>(
        additional_helpers: Vec<Shared<dyn RateHelper>>,
        additional_dates: Option<Box<AdditionalDates>>,
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
        penalties: F,
    ) -> GlobalBootstrap
    where
        F: Fn() -> Vec<Real> + 'static,
    {
        Self::with_penalties(
            additional_helpers,
            additional_dates,
            accuracy,
            end_criteria,
            instrument_weights,
            move |_, _| penalties(),
        )
    }
}

/// The global cost (`setCostFunctionArgument` + `evaluateCostFunction`,
/// `globalbootstrap.hpp:379-403`): writes every interior trial node into the
/// node vector through `transform_direct`, rebuilds the full-grid
/// interpolation, and returns the alive helpers' weighted quote errors -
/// followed by the [`AdditionalPenalties`] terms, if any - as the residual
/// vector.
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
    penalties: Option<&'a AdditionalPenalties>,
    /// The number of interior nodes, `times.len() - 1`, and the number of
    /// variables the solve carries; the residual count is `alive.len()` plus
    /// the penalty terms. The two are equal for a strip of distinct pillars
    /// and no additional dates; each additional date adds one variable the
    /// penalty terms have to answer for, and the least-squares solver rejects
    /// a system left with fewer residuals than variables.
    interior: Size,
    /// The penalty-term count of the last evaluation, so a failed evaluation's
    /// NaN vector has the length the solver sized itself on.
    penalty_len: Cell<Size>,
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
                let t = cd.times()[i + 1];
                let value = C::Traits::transform_direct(x[i], t);
                C::Traits::update_guess(cd.data_mut(), value, i + 1);
            }
            cd.rebuild(self.curve.interpolator(), self.interior)?;
        }
        // Only now that the `borrow_mut` above has dropped, so a penalty that
        // reads the curve back can take its own shared borrow (`:392-395`).
        let penalty_errors = match self.penalties {
            Some(penalties) => {
                let cd = self.curve.curve_data().borrow();
                penalties(cd.times(), cd.data())
            }
            None => Vec::new(),
        };
        self.penalty_len.set(penalty_errors.len());

        let mut residuals = Array::with_size(self.alive.len() + penalty_errors.len());
        for (i, helper) in self.alive.iter().enumerate() {
            residuals[i] = helper.quote_error()? * self.alive_weights[i];
        }
        for (i, penalty_error) in penalty_errors.into_iter().enumerate() {
            residuals[self.alive.len() + i] = penalty_error;
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
                let residuals = self.alive.len() + self.penalty_len.get();
                std::iter::repeat_n(Real::NAN, residuals).collect()
            }
        }
    }
}

impl<C> Bootstrap<C> for GlobalBootstrap
where
    C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
    C::Traits: YieldBootstrapTraits,
{
    /// The additional helpers, all of them: the alive filter of `calculate`
    /// governs the solve, never observability (`globalbootstrap.hpp:219-220`).
    fn additional_observables(&self) -> Vec<Shared<Observable>> {
        self.additional_helpers
            .iter()
            .map(|helper| helper.base().observable_shared())
            .collect()
    }

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

        // The alive additional helpers (`:256-262`), on the same threshold as
        // the instruments; they carry no pillar and no residual.
        let alive_additional: Vec<&Shared<dyn RateHelper>> = self
            .additional_helpers
            .iter()
            .filter(|helper| helper.pillar_date() > first_date)
            .collect();

        // The additional dates (`:264-274`), re-read on every calculation
        // because the upstream functor is evaluation-date relative, with the
        // expired ones dropped before they can reach the grid.
        let additional_dates: Vec<Date> = match &self.additional_dates {
            Some(dates) => dates()
                .into_iter()
                .filter(|date| *date > first_date)
                .collect(),
            None => Vec::new(),
        };

        // The pillar grid (`:280-294`): the first date plus every alive
        // pillar plus the surviving additional dates, sorted with duplicates
        // merged - the one dedup in QuantLib's bootstraps. A duplicate pillar
        // leaves the least-squares system overdetermined rather than rejected,
        // unlike IterativeBootstrap.
        let mut dates = Vec::with_capacity(alive.len() + additional_dates.len() + 1);
        dates.push(first_date);
        dates.extend(alive.iter().map(|helper| helper.pillar_date()));
        dates.extend(additional_dates);
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

        // maxDate covers every alive helper, additional helpers included
        // (`:301-306`).
        let mut max_date = *dates.last().expect("the grid holds the first date");
        for helper in alive.iter().chain(alive_additional.iter().copied()) {
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

        // The same for the alive additional helpers, after the instruments and
        // under their own message (`:347-352`).
        for helper in &alive_additional {
            if helper.quote_value().is_err() {
                crate::fail!(
                    "additional instrument (maturity: {}) has an invalid quote",
                    helper.maturity_date()
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
                guess[i] = C::Traits::transform_inverse(cd.data()[i + 1], cd.times()[i + 1]);
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
            penalties: self.penalties.as_deref(),
            interior,
            penalty_len: Cell::new(0),
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
                let t = cd.times()[i + 1];
                let value = C::Traits::transform_direct(solution[i], t);
                C::Traits::update_guess(cd.data_mut(), value, i + 1);
            }
            cd.rebuild(curve.interpolator(), interior)?;
            cd.set_valid(true);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Oracle: `piecewiseyieldcurve.cpp` `testGlobalBootstrapPenalty`
    //! (`:1388-1483`) - a 32-instrument EUR strip (one 6M deposit, twelve
    //! FRAs, nineteen swaps) bootstrapped as `PiecewiseYieldCurve<ForwardRate,
    //! BackwardFlat, GlobalBootstrap>` once with no penalty and once under the
    //! upstream gradient penalty.
    //!
    //! The reference numbers are NOT the literals printed in the `.cpp`: they
    //! are reproduced at full precision by a C++ harness that rebuilds this
    //! fixture against a locally built QuantLib 1.43-dev dylib, with
    //! `IborCoupon::Settings::instance().createAtParCoupons()` set so that
    //! `usingAtParCoupons()` - the test's own precondition - holds. The `.cpp`
    //! literals agree with the harness to within 8.9e-9, inside their own
    //! 8-decimal printing (61 of the 64 are the dylib value rounded to 8
    //! decimals, the rest truncated), so they are not stale.
    //!
    //! The asserts keep the C++ tolerance of 1e-6, but the port is far tighter
    //! than that: every one of the 64 rates matches the dylib to better than
    //! 1e-9, and the worst node parts company only at 1e-12.

    use std::cell::Cell;

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::flat::BackwardFlat;
    use crate::math::interpolations::linear::Linear;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::TermStructure;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::{ForwardRate, SimpleZeroYield};
    use crate::termstructures::yields::{
        DepositRateHelper, FraRateHelper, PiecewiseYieldCurve, Pillar, SwapRateHelper,
    };
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Day, Month, Year};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Natural;

    /// The market quotes in percent (`piecewiseyieldcurve.cpp:1393-1397`):
    /// the 6M deposit, then the twelve FRAs, then the nineteen swaps.
    const REF_MKT_RATE: [Real; 32] = [
        -0.373, -0.388, -0.402, -0.418, -0.431, -0.441, -0.45, -0.457, -0.463, -0.469, -0.461,
        -0.463, -0.479, -0.4511, -0.45418, -0.439, -0.4124, -0.37703, -0.3335, -0.28168, -0.22725,
        -0.1745, -0.12425, -0.07746, 0.0385, 0.1435, 0.17525, 0.17275, 0.1515, 0.1225, 0.095,
        0.0644,
    ];

    /// The swap tenors in years (`:1436`).
    const SWAP_TENORS: [i32; 19] = [
        2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 20, 25, 30, 35, 40, 45, 50,
    ];

    /// The 32 pillar dates (`piecewiseyieldcurve.cpp:1401-1409`).
    const REF_DATE: [(Day, Month, Year); 32] = [
        (31, Month::March, 2020),
        (30, Month::April, 2020),
        (29, Month::May, 2020),
        (30, Month::June, 2020),
        (31, Month::July, 2020),
        (31, Month::August, 2020),
        (30, Month::September, 2020),
        (30, Month::October, 2020),
        (30, Month::November, 2020),
        (31, Month::December, 2020),
        (29, Month::January, 2021),
        (26, Month::February, 2021),
        (31, Month::March, 2021),
        (30, Month::September, 2021),
        (30, Month::September, 2022),
        (29, Month::September, 2023),
        (30, Month::September, 2024),
        (30, Month::September, 2025),
        (30, Month::September, 2026),
        (30, Month::September, 2027),
        (29, Month::September, 2028),
        (28, Month::September, 2029),
        (30, Month::September, 2030),
        (30, Month::September, 2031),
        (29, Month::September, 2034),
        (30, Month::September, 2039),
        (30, Month::September, 2044),
        (30, Month::September, 2049),
        (30, Month::September, 2054),
        (30, Month::September, 2059),
        (30, Month::September, 2064),
        (30, Month::September, 2069),
    ];

    /// The no-penalty pillar zero rates, reproduced from the C++ dylib and
    /// written in their shortest round-tripping form; `:1410-1415` prints the
    /// same values printed to 8 decimals.
    const REF_ZERO_RATE_NP: [Real; 32] = [
        -0.00373354067173059,
        -0.0038619401591129116,
        -0.003952053377431906,
        -0.004033031764634922,
        -0.004080332294683344,
        -0.00410875148971975,
        -0.004119347704602793,
        -0.004191606489573042,
        -0.0042481675261172285,
        -0.004299228525952772,
        -0.004280288678277469,
        -0.0042917785223669895,
        -0.00434401190355896,
        -0.0044524306053832785,
        -0.004485055406658176,
        -0.004336901365743163,
        -0.004074010693284356,
        -0.0037275150355157486,
        -0.0033005022038937737,
        -0.002791390998101853,
        -0.0022547726443914143,
        -0.0017342152462374019,
        -0.001236880404786612,
        -0.0007723647126770113,
        0.0003855052397250581,
        0.0014420799596420013,
        0.001759470920941431,
        0.00172834231444819,
        0.0015075667291268061,
        0.0012113127300807914,
        0.0009338400348746001,
        0.0006289189187075171,
    ];

    /// The `testGlobalBootstrap` pillar zero rates, reproduced from the C++
    /// dylib and written in their shortest round-tripping form; `:1337-1342`
    /// prints the same values to 8 decimals.
    const REF_ZERO_RATE_AD: [Real; 32] = [
        -0.00373354067173059,
        -0.003810050775257402,
        -0.0038768926341334457,
        -0.0039412379977853225,
        -0.0040770590967655115,
        -0.004136329462812865,
        -0.004119347704602793,
        -0.004163696290681206,
        -0.004205570570898868,
        -0.004244312604300202,
        -0.004278238728862865,
        -0.004309771141705333,
        -0.00434401190355896,
        -0.0044524306053832785,
        -0.004485055406658101,
        -0.0043369013657431075,
        -0.0040740106932843105,
        -0.0037275150355157113,
        -0.003300502203893726,
        -0.002791390998101797,
        -0.0022547726443913644,
        -0.0017342152462374019,
        -0.0012368804047865718,
        -0.0007723647126769745,
        0.00038554381038254917,
        0.0014424807165811571,
        0.001759949836190311,
        0.0017287285812646173,
        0.0015078180913406802,
        0.0012114528819535877,
        0.000933912094611891,
        0.0006289461592278805,
    ];

    /// The gradient-penalty pillar zero rates, reproduced from the C++ dylib and
    /// written in their shortest round-tripping form; `:1417-1422` prints the
    /// same values printed to 8 decimals.
    const REF_ZERO_RATE_GP: [Real; 32] = [
        -0.0037789204343363957,
        -0.003861265918257509,
        -0.003947374024601186,
        -0.0040291352443265075,
        -0.004095413491332919,
        -0.0041325177094445505,
        -0.00415463322202404,
        -0.004194838278258465,
        -0.004242382682770642,
        -0.004278749680844317,
        -0.0042971214597928705,
        -0.00431898196411309,
        -0.004360271377797676,
        -0.00445296974357845,
        -0.004485023476300989,
        -0.004336935907495182,
        -0.0040740612083099365,
        -0.0037275506595484164,
        -0.003300180655052014,
        -0.0027913299732067252,
        -0.002254907688857512,
        -0.0017342855088808304,
        -0.0012364330378685168,
        -0.0007729806599035981,
        0.0003854725793177982,
        0.001442061640980936,
        0.001759475820307776,
        0.0017283380850002651,
        0.0015075606415153413,
        0.0012113489415541431,
        0.0009337950842231714,
        0.0006289530535829015,
    ];

    /// The curve under test.
    type PenaltyCurve = PiecewiseYieldCurve<ForwardRate, BackwardFlat, GlobalBootstrap>;

    /// The shared fixture: evaluation date 26 Sep 2019 (`:1390`/`:1309`) and
    /// the 32 helpers of `:1428-1441` (`:1339-1352`), all reading one
    /// empty-forwarding Euribor 6M.
    ///
    /// The settings and the index are carried too: `testGlobalBootstrap` builds
    /// its additional helpers off the same index (`:1362`) and reads the
    /// evaluation date from its additional-dates functor (`:1290`).
    struct Fixture {
        reference_date: Date,
        helpers: Vec<Shared<dyn RateHelper>>,
        settings: Shared<Settings<Date>>,
        index: IborIndex,
    }

    /// C++ builds the curve from `(2, TARGET())`, a moving reference two
    /// business days after the evaluation date; nothing in this test moves that
    /// date, so the equivalent fixed reference (30 Sep 2019) is computed here
    /// and handed to the reference-date constructor.
    ///
    /// The deposit takes its schedule from the index rather than from the
    /// explicit `(6M, 2, TARGET(), ModifiedFollowing, true, Actual360())` of
    /// `:1428-1429`: Euribor 6M carries exactly those six conventions.
    fn fixture() -> Fixture {
        let calendar = Target::new();
        let today = Date::new(26, Month::September, 2019);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let reference_date = calendar.advance(
            today,
            2,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        let euribor6m = Euribor::six_months(Handle::empty(), Shared::clone(&settings));

        let mut helpers: Vec<Shared<dyn RateHelper>> = Vec::new();
        helpers.push(
            DepositRateHelper::from_rate(REF_MKT_RATE[0] / 100.0, &euribor6m)
                as Shared<dyn RateHelper>,
        );
        for (i, rate) in REF_MKT_RATE[1..=12].iter().enumerate() {
            let quote = Handle::new(shared(SimpleQuote::new(rate / 100.0)) as Shared<dyn Quote>);
            helpers.push(FraRateHelper::from_months(
                quote,
                i as Natural + 1,
                &euribor6m,
                true,
                Pillar::LastRelevantDate,
            ) as Shared<dyn RateHelper>);
        }
        for (i, tenor) in SWAP_TENORS.iter().enumerate() {
            helpers.push(SwapRateHelper::from_rate(
                REF_MKT_RATE[13 + i] / 100.0,
                Period::new(*tenor, TimeUnit::Years),
                calendar.clone(),
                Frequency::Annual,
                BusinessDayConvention::ModifiedFollowing,
                Thirty360::with_convention(Convention::BondBasis),
                &euribor6m,
            ) as Shared<dyn RateHelper>);
        }

        Fixture {
            reference_date,
            helpers,
            settings,
            index: euribor6m,
        }
    }

    /// The curve of `:1445-1448`, with the explicit 1.0e-12 accuracy both arms
    /// pass.
    fn curve_with(fixture: &Fixture, bootstrap: GlobalBootstrap) -> Shared<PenaltyCurve> {
        PiecewiseYieldCurve::with_bootstrap(
            fixture.reference_date,
            fixture.helpers.clone(),
            Actual365Fixed::new(),
            BackwardFlat,
            bootstrap,
        )
        .expect("the 32-helper strip builds a curve")
    }

    /// The continuous Actual/360 zero rate at every pillar (`:1459`/`:1481`,
    /// `:1383`). Taken through the term-structure trait so both oracle curves
    /// share it.
    fn zero_rates(curve: &dyn YieldTermStructure, fixture: &Fixture) -> Vec<Real> {
        fixture
            .helpers
            .iter()
            .map(|helper| {
                curve
                    .zero_rate_date(
                        helper.pillar_date(),
                        Actual360::new(),
                        Compounding::Continuous,
                        Frequency::Annual,
                        false,
                    )
                    .expect("the bootstrapped curve prices every pillar")
                    .rate()
            })
            .collect()
    }

    /// The re-entrancy pin of the penalty slice: a penalty that READS THE
    /// CURVE mid-solve, the shape the `additionalHelpers` penalty takes
    /// upstream (`globalbootstrap.hpp:392-395` reprices additional
    /// helpers off the trial curve).
    ///
    /// The residual is a tiny multiple of the first helper's own quote error,
    /// which is driven to zero by the helper residual anyway, so the argmin is
    /// unmoved and the strip still reprices; what the arm pins is that the
    /// shared borrow inside `implied_quote` is reachable at all. Invoking the
    /// penalty while the node-rewriting `borrow_mut` is still live panics with
    /// "already mutably borrowed", and neither the upstream gradient penalty
    /// nor the no-argument one would notice: they never touch the `RefCell`.
    #[test]
    fn a_penalty_that_reprices_a_helper_solves() {
        let fixture = fixture();
        let probe = Shared::clone(&fixture.helpers[0]);
        let fired = shared(Cell::new(0_usize));
        let counter = Shared::clone(&fired);
        let curve = curve_with(
            &fixture,
            GlobalBootstrap::with_penalties(
                Vec::new(),
                None,
                Some(1.0e-12),
                None,
                Vec::new(),
                move |_, _| {
                    counter.set(counter.get() + 1);
                    let error = probe
                        .quote_error()
                        .expect("the probe helper reprices off the trial curve");
                    vec![1.0e-8 * error]
                },
            ),
        );

        let rates = zero_rates(curve.as_ref(), &fixture);
        assert!(
            rates.iter().all(|rate| rate.is_finite()),
            "the solved curve must carry finite zero rates"
        );
        assert!(
            fired.get() > 0,
            "the curve-reading penalty was never invoked"
        );
        for (i, helper) in fixture.helpers.iter().enumerate() {
            let error = helper
                .quote_error()
                .expect("every helper reprices off the solved curve");
            assert!(
                error.abs() < 1.0e-9,
                "helper {i} does not reprice under a curve-reading penalty: {error}"
            );
        }
    }

    /// The no-argument penalty adapter (C++ constructor #3,
    /// `globalbootstrap.hpp:196-206`), pinned by its call count.
    ///
    /// HONEST NEGATIVE: this ctor cannot be pinned through the curve. A penalty
    /// that ignores the node grid returns the same residual at every trial
    /// point, so it cannot move the argmin; and a ZERO one cannot move the
    /// stopping point either, since it adds nothing to the cost. The count is
    /// therefore the only evidence the adapter reaches the residual vector -
    /// which it must, since the residual length the solver sizes itself on
    /// grows from 32 to 33.
    #[test]
    fn the_no_argument_penalty_adapter_is_invoked() {
        let fixture = fixture();
        let fired = shared(Cell::new(0_usize));
        let counter = Shared::clone(&fired);
        let curve = curve_with(
            &fixture,
            GlobalBootstrap::with_grid_independent_penalties(
                Vec::new(),
                None,
                Some(1.0e-12),
                None,
                Vec::new(),
                move || {
                    counter.set(counter.get() + 1);
                    vec![0.0]
                },
            ),
        );

        assert!(
            zero_rates(curve.as_ref(), &fixture)
                .iter()
                .all(|r| r.is_finite()),
            "a zero constant penalty must leave the strip solvable"
        );
        assert!(
            fired.get() > 0,
            "the no-argument penalty never reached the residual vector"
        );
    }
    /// The penalty-argument contract (`globalbootstrap.hpp:395`): the closure
    /// is handed the FULL node grid, node 0 included, not the interior slice
    /// the optimizer varies.
    ///
    /// HONEST NEGATIVE: `testGlobalBootstrapPenalty` cannot pin this. The
    /// `ForwardRate` traits mirror node 0 onto node 1, so the gradient
    /// penalty's first term is identically zero and an interior-only slice
    /// solves to the same curve (probe-confirmed at gate). The contract is
    /// therefore pinned directly: 33 nodes for 32 helpers, a zero time at the
    /// front, and the mirrored node the oracle's blindness rests on.
    #[test]
    fn the_penalty_sees_the_full_node_grid() {
        let fixture = fixture();
        let seen = shared(Cell::new((
            0_usize,
            0_usize,
            Real::NAN,
            Real::NAN,
            Real::NAN,
        )));
        let record = Shared::clone(&seen);
        let curve = curve_with(
            &fixture,
            GlobalBootstrap::with_penalties(
                Vec::new(),
                None,
                Some(1.0e-12),
                None,
                Vec::new(),
                move |times, data| {
                    record.set((times.len(), data.len(), times[0], data[0], data[1]));
                    Vec::new()
                },
            ),
        );

        assert!(
            zero_rates(curve.as_ref(), &fixture)
                .iter()
                .all(|r| r.is_finite())
        );
        let (times_len, data_len, first_time, node0, node1) = seen.get();
        assert_eq!(
            times_len,
            fixture.helpers.len() + 1,
            "the closure must see every node"
        );
        assert_eq!(data_len, times_len);
        assert_eq!(first_time, 0.0, "node 0 is the reference-date node");
        assert_eq!(node0, node1, "ForwardRate mirrors node 0 onto node 1");
    }

    /// ARM 0 of `testGlobalBootstrapPenalty` (`:1450-1453`): the 32 pillar
    /// dates. External truth that stands on its own - it fixes the helper
    /// construction (index conventions, FRA start offsets, swap tenors and the
    /// `Pillar::LastRelevantDate` choice) without any reference to the solve,
    /// so a mis-specified strip fails here rather than smearing into the rates.
    #[test]
    fn global_bootstrap_penalty_pillar_dates() {
        let fixture = fixture();
        assert_eq!(
            fixture.reference_date,
            Date::new(30, Month::September, 2019)
        );
        for (i, (day, month, year)) in REF_DATE.iter().enumerate() {
            assert_eq!(
                fixture.helpers[i].pillar_date(),
                Date::new(*day, *month, *year),
                "helper {i} sits on the wrong pillar"
            );
        }
    }

    /// The two rate arms of `testGlobalBootstrapPenalty` at the C++ tolerance
    /// of 1e-6 (0.01 basis points): the no-penalty curve of `:1445-1448` and
    /// the gradient-penalty curve of `:1472-1475`, whose penalty
    /// `0.01 * (data[i + 1] - data[i]) / (times[i + 1] - times[i])` over
    /// `times.len() - 1` terms (`:1464-1470`) reads the FULL node grid,
    /// node 0 included.
    ///
    /// The C++ no-penalty arm passes an EMPTY `std::function<Array()>`, which
    /// constructor #3 turns into no penalty at all rather than into a
    /// zero-length one, so it is [`GlobalBootstrap::new`] here.
    ///
    /// VACUITY GUARD: a port that accepted the penalty and then ignored it
    /// would solve ONE curve and hand it to both arms, and both tables would
    /// still pass at 1e-6 for the 20 pillars where they agree. The two tables
    /// are therefore asserted to DIFFER first: the gradient penalty moves the
    /// short end by 4.5e-5, forty-five times the assert tolerance.
    #[test]
    fn global_bootstrap_penalty_zero_rates() {
        let fixture = fixture();
        let no_penalty = zero_rates(
            curve_with(
                &fixture,
                GlobalBootstrap::new(Some(1.0e-12), None, Vec::new()),
            )
            .as_ref(),
            &fixture,
        );
        let gradient_penalty = zero_rates(
            curve_with(
                &fixture,
                GlobalBootstrap::with_penalties(
                    Vec::new(),
                    None,
                    Some(1.0e-12),
                    None,
                    Vec::new(),
                    |times, data| {
                        (0..times.len() - 1)
                            .map(|i| 0.01 * (data[i + 1] - data[i]) / (times[i + 1] - times[i]))
                            .collect()
                    },
                ),
            )
            .as_ref(),
            &fixture,
        );

        let separation = no_penalty
            .iter()
            .zip(&gradient_penalty)
            .map(|(np, gp)| (np - gp).abs())
            .fold(0.0, Real::max);
        assert!(
            separation > 1.0e-5,
            "the penalty did not move the solve: the two arms agree to {separation}"
        );

        for (i, expected) in REF_ZERO_RATE_NP.iter().enumerate() {
            assert!(
                (no_penalty[i] - expected).abs() < 1.0e-6,
                "no-penalty zero rate {i}: {} vs {expected}",
                no_penalty[i]
            );
        }
        for (i, expected) in REF_ZERO_RATE_GP.iter().enumerate() {
            assert!(
                (gradient_penalty[i] - expected).abs() < 1.0e-6,
                "gradient-penalty zero rate {i}: {} vs {expected}",
                gradient_penalty[i]
            );
        }
    }

    /// The curve of `testGlobalBootstrap` (`piecewiseyieldcurve.cpp:1367-1372`):
    /// the same 32-helper strip under the simply compounded zero-yield traits
    /// and a linear interpolation.
    type AdditionalCurve = PiecewiseYieldCurve<SimpleZeroYield, Linear, GlobalBootstrap>;

    /// The seven additional helpers of `:1357-1364`: FRAs on the strip's own
    /// index, at a flat -0.004, starting 12 to 18 months out. They add neither
    /// a pillar nor a residual; the penalty below is the only thing that reads
    /// them.
    fn additional_helpers(fixture: &Fixture) -> Vec<Shared<dyn RateHelper>> {
        (0..7)
            .map(|i| {
                let quote = Handle::new(shared(SimpleQuote::new(-0.004)) as Shared<dyn Quote>);
                FraRateHelper::from_months(
                    quote,
                    12 + i,
                    &fixture.index,
                    true,
                    Pillar::LastRelevantDate,
                ) as Shared<dyn RateHelper>
            })
            .collect()
    }

    /// The `additionalDates` functor of `:1287-1303`: the five monthly dates
    /// past spot, with the evaluation date minus one day pushed to the FRONT
    /// and minus two days appended at the BACK. Both of those precede the
    /// curve's first date and must be dropped; the vector is deliberately left
    /// unsorted, since the grid sorts it.
    fn additional_dates(fixture: &Fixture) -> Box<AdditionalDates> {
        let settings = Shared::clone(&fixture.settings);
        Box::new(move || {
            let calendar = Target::new();
            let today = settings
                .evaluation_date()
                .expect("the fixture sets an evaluation date");
            let settlement = calendar.advance(
                today,
                2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            let mut dates: Vec<Date> = (1..=5)
                .map(|i| {
                    calendar.advance(
                        settlement,
                        i,
                        TimeUnit::Months,
                        BusinessDayConvention::Following,
                        false,
                    )
                })
                .collect();
            dates.insert(0, today - 1);
            dates.push(today - 2);
            dates
        })
    }

    /// The `additionalErrors` functor of `:1271-1285`: the seven additional
    /// helpers' implied quotes forced onto a straight line, so the five
    /// interior ones are pinned by the two ends. These are the residuals that
    /// answer for the five extra variables the additional dates create.
    fn additional_errors(helpers: Vec<Shared<dyn RateHelper>>) -> impl Fn() -> Vec<Real> {
        move || {
            let implied = |i: usize| {
                helpers[i]
                    .implied_quote()
                    .expect("an additional helper reprices off the trial curve")
            };
            let a = implied(0);
            let b = implied(6);
            (0..5)
                .map(|k| (5.0 - k as Real) / 6.0 * a + (1.0 + k as Real) / 6.0 * b - implied(1 + k))
                .collect()
        }
    }

    /// The full `testGlobalBootstrap` configuration, over the shared fixture.
    fn additional_curve(fixture: &Fixture) -> Shared<AdditionalCurve> {
        let helpers = additional_helpers(fixture);
        PiecewiseYieldCurve::with_bootstrap(
            fixture.reference_date,
            fixture.helpers.clone(),
            Actual365Fixed::new(),
            Linear,
            GlobalBootstrap::with_grid_independent_penalties(
                helpers.clone(),
                Some(additional_dates(fixture)),
                Some(1.0e-12),
                None,
                Vec::new(),
                additional_errors(helpers),
            ),
        )
        .expect("the 32-helper strip builds a curve")
    }

    /// ARM A of `testGlobalBootstrap` (`:1376-1378`): the pillar dates, both
    /// families. External truth that stands on its own - no reference to the
    /// solve - so a mis-specified strip or a mis-specified additional helper
    /// fails here rather than smearing into the rates. The seven additional
    /// pillars are the dylib's, and they also establish that all seven are
    /// alive (each is past the 30 Sep 2019 first date).
    #[test]
    fn global_bootstrap_pillar_dates() {
        let fixture = fixture();
        for (i, (day, month, year)) in REF_DATE.iter().enumerate() {
            assert_eq!(
                fixture.helpers[i].pillar_date(),
                Date::new(*day, *month, *year),
                "helper {i} sits on the wrong pillar"
            );
        }

        let expected = [
            Date::new(31, Month::March, 2021),
            Date::new(30, Month::April, 2021),
            Date::new(31, Month::May, 2021),
            Date::new(30, Month::June, 2021),
            Date::new(30, Month::July, 2021),
            Date::new(31, Month::August, 2021),
            Date::new(30, Month::September, 2021),
        ];
        for (i, helper) in additional_helpers(&fixture).iter().enumerate() {
            assert_eq!(
                helper.pillar_date(),
                expected[i],
                "additional helper {i} sits on the wrong pillar"
            );
            assert!(helper.pillar_date() > fixture.reference_date);
        }
    }

    /// ARM B, the headline oracle of `testGlobalBootstrap` (`:1381-1385`): the
    /// 32 pillar zero rates at the C++ tolerance of 1e-6 (0.01 basis points).
    ///
    /// The reference numbers are NOT the `.cpp` literals: they are reproduced
    /// at full precision by a C++ harness rebuilding this fixture against a
    /// locally built QuantLib dylib with
    /// `IborCoupon::Settings::instance().createAtParCoupons()` set, so that the
    /// test's own `usingAtParCoupons()` precondition holds. The literals agree
    /// with the harness to 5.3e-9, inside their 8-decimal printing.
    ///
    /// VACUITY GUARD: the numbers depend on the additional dates. Dropping the
    /// `additionalDates` functor leaves a 33-node grid whose solution moves 14
    /// of these 32 rates past the assert tolerance (worst 7.8e-5, dylib-
    /// measured), so this table cannot be reproduced by the #974 machinery
    /// alone.
    #[test]
    fn global_bootstrap_zero_rates() {
        let fixture = fixture();
        let rates = zero_rates(additional_curve(&fixture).as_ref(), &fixture);

        let worst = rates
            .iter()
            .zip(&REF_ZERO_RATE_AD)
            .map(|(rate, expected)| (rate - expected).abs())
            .fold(0.0, Real::max);
        assert!(
            worst < 1.0e-6,
            "the strip parts company with the dylib by {worst}"
        );
    }

    /// ARM C, the stale-date drop (`globalbootstrap.hpp:268-274`): the two
    /// dates before the curve's first date never reach the grid.
    ///
    /// The count is the discriminating half - 38 nodes is the reference date
    /// plus 32 pillars plus the FIVE surviving dates, and a port that kept the
    /// stale pair would carry 40 and would not solve, since the two extra
    /// variables have no residual answering for them. The membership asserts
    /// name which five survived.
    #[test]
    fn global_bootstrap_drops_stale_additional_dates() {
        let fixture = fixture();
        let dates = additional_curve(&fixture)
            .dates()
            .expect("the solved curve exposes its nodes");

        assert_eq!(
            dates.len(),
            38,
            "reference + 32 pillars + 5 surviving dates"
        );
        assert_eq!(dates[0], fixture.reference_date);
        for date in [
            Date::new(30, Month::October, 2019),
            Date::new(2, Month::December, 2019),
            Date::new(30, Month::December, 2019),
            Date::new(30, Month::January, 2020),
            Date::new(2, Month::March, 2020),
        ] {
            assert!(dates.contains(&date), "{date} should be a node");
        }
        for stale in [
            Date::new(25, Month::September, 2019),
            Date::new(24, Month::September, 2019),
        ] {
            assert!(!dates.contains(&stale), "{stale} precedes the first date");
        }
    }

    /// ARM E: at the solution BOTH residual families vanish - the 32 helpers'
    /// quote errors and the five penalty terms alike (the dylib reaches 6.6e-16
    /// and 4.7e-16).
    ///
    /// This is what makes the enlarged system square: 37 variables answered by
    /// 32 + 5 residuals. It also exercises the additional helpers' own
    /// `set_term_structure` loop (`hpp:347-352`) - an additional helper that
    /// was never handed the curve cannot imply a quote at all, and the penalty
    /// evaluated here would fail rather than come out small.
    #[test]
    fn global_bootstrap_zeroes_both_residual_families() {
        let fixture = fixture();
        let helpers = additional_helpers(&fixture);
        let curve = PiecewiseYieldCurve::<SimpleZeroYield, Linear, _>::with_bootstrap(
            fixture.reference_date,
            fixture.helpers.clone(),
            Actual365Fixed::new(),
            Linear,
            GlobalBootstrap::with_grid_independent_penalties(
                helpers.clone(),
                Some(additional_dates(&fixture)),
                Some(1.0e-12),
                None,
                Vec::new(),
                additional_errors(helpers.clone()),
            ),
        )
        .expect("the 32-helper strip builds a curve");
        curve.dates().expect("the strip solves");

        for (i, helper) in fixture.helpers.iter().enumerate() {
            let error = helper
                .quote_error()
                .expect("every helper reprices off the solved curve");
            assert!(error.abs() < 1.0e-9, "helper {i} does not reprice: {error}");
        }
        for (k, error) in additional_errors(helpers)().iter().enumerate() {
            assert!(
                error.abs() < 1.0e-9,
                "penalty term {k} does not vanish: {error}"
            );
        }
    }

    /// ARM F, the under-determined pin: the same additional dates with NO
    /// penalty leave 32 residuals against 37 variables, and the least-squares
    /// solver refuses the system (`levenbergmarquardt.rs:167-170`; the C++ LM
    /// raises the identical text).
    ///
    /// This is the arm that proves the surviving dates became free VARIABLES
    /// rather than decoration, and it proves it without the dylib: a port that
    /// inserted the dates into the grid but not into the optimizer's argument
    /// vector would solve happily here. An empty penalty vector stands in for
    /// the C++ null `additionalPenalties_`, which contributes no residual
    /// either.
    #[test]
    fn global_bootstrap_additional_dates_need_penalty_terms() {
        let fixture = fixture();
        let curve = PiecewiseYieldCurve::<SimpleZeroYield, Linear, _>::with_bootstrap(
            fixture.reference_date,
            fixture.helpers.clone(),
            Actual365Fixed::new(),
            Linear,
            GlobalBootstrap::with_penalties(
                Vec::new(),
                Some(additional_dates(&fixture)),
                Some(1.0e-12),
                None,
                Vec::new(),
                |_, _| Vec::new(),
            ),
        )
        .expect("construction is lazy");

        let message = curve
            .dates()
            .expect_err("32 residuals cannot pin 37 variables")
            .to_string();
        assert!(
            message.contains("less functions (32) than available variables (37)"),
            "unexpected failure: {message}"
        );
    }

    /// ARM G, the maxDate extension over the additional helpers
    /// (`globalbootstrap.hpp:305-306`).
    ///
    /// HONEST NEGATIVE: the oracle fixture is BLIND to this line. Its
    /// additional FRAs all mature in 2021, far inside the 2069 last pillar, so
    /// the dylib's MAX_DATE is the last pillar either way. The arm is therefore
    /// built deliberately: the deposit and twelve FRAs of the fixture, whose
    /// grid ends on 31 Mar 2021, plus one additional helper reaching 30 Sep
    /// 2021. Without the extension the curve would stop at the last pillar and
    /// the discount query below would fall outside its range.
    #[test]
    fn global_bootstrap_max_date_covers_an_additional_helper() {
        let fixture = fixture();
        let additional = Shared::clone(&additional_helpers(&fixture)[6]);
        let curve = PiecewiseYieldCurve::<SimpleZeroYield, Linear, _>::with_bootstrap(
            fixture.reference_date,
            fixture.helpers[..13].to_vec(),
            Actual365Fixed::new(),
            Linear,
            GlobalBootstrap::with_penalties(
                vec![Shared::clone(&additional)],
                None,
                Some(1.0e-12),
                None,
                Vec::new(),
                |_, _| Vec::new(),
            ),
        )
        .expect("the front of the strip builds a curve");

        let last_pillar = fixture.helpers[12].pillar_date();
        let max_date = curve.max_date();
        assert_ne!(
            max_date, last_pillar,
            "the additional helper must push the maximum past the last pillar"
        );
        assert_eq!(max_date, additional.latest_relevant_date());
        assert!(
            curve.discount_date(max_date, false).is_ok(),
            "the extended range must be queryable without extrapolation"
        );
    }
}
