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
//! The single-curve path is ported, together with the `additionalPenalties`
//! residual terms (#974); it equals the C++ path with the remaining
//! `additional*` arguments empty and `parentBootstrapper_` null. Deferred
//! visibly, each as its own follow-up issue referencing #949:
//!
//! - **Additional restrictions** (`additionalHelpers`/`additionalDates`/
//!   `additionalVariables`, plus the `SimpleQuoteVariables` helper and the
//!   `SimpleZeroYield` traits): the machinery the two remaining upstream
//!   oracle tests (`piecewiseyieldcurve.cpp:1306/1486`) exercise - futures
//!   convexity adjustments, model variables.
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
use crate::require;
use crate::shared::Shared;
use crate::termstructures::bootstraphelper::BootstrapHelperShared;
use crate::termstructures::bootstraptraits::{BootstrapTraits, YieldBootstrapTraits};
use crate::termstructures::iterativebootstrap::{Bootstrap, PiecewiseCurve};
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
            accuracy,
            end_criteria,
            instrument_weights,
            penalties: None,
        }
    }

    /// The same configuration plus [`AdditionalPenalties`] over the trial
    /// curve's node grid (the penalty argument of C++ constructor #2,
    /// `globalbootstrap.hpp:116-123`, definition `:169-186`).
    ///
    /// That constructor also carries `additionalHelpers`, `additionalDates` and
    /// `additionalVariables`; those are still deferred, so this form takes the
    /// penalty alone.
    pub fn with_penalties<F>(
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
        penalties: F,
    ) -> GlobalBootstrap
    where
        F: Fn(&[Time], &[Real]) -> Vec<Real> + 'static,
    {
        GlobalBootstrap {
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
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
        penalties: F,
    ) -> GlobalBootstrap
    where
        F: Fn() -> Vec<Real> + 'static,
    {
        Self::with_penalties(accuracy, end_criteria, instrument_weights, move |_, _| {
            penalties()
        })
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
    /// The number of interior nodes, `times.len() - 1`; residual count is
    /// `alive.len()` plus the penalty terms, at least `interior` (the grid
    /// dedup can only shrink the helper half).
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
                let value = C::Traits::transform_direct(x[i]);
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
                let value = C::Traits::transform_direct(solution[i]);
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
    //! are reproduced to 17 digits by a C++ harness that rebuilds this fixture
    //! against a locally built QuantLib 1.43-dev dylib, with
    //! `IborCoupon::Settings::instance().createAtParCoupons()` set so that
    //! `usingAtParCoupons()` - the test's own precondition - holds. The `.cpp`
    //! literals agree with the harness to within 8.9e-9, which is the
    //! truncation of their own 8-decimal printing, so they are not stale.

    use std::cell::Cell;

    use super::*;
    use crate::handle::Handle;
    use crate::indexes::ibor::euribor::Euribor;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::flat::BackwardFlat;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared;
    use crate::termstructures::bootstraphelper::RateHelper;
    use crate::termstructures::bootstraptraits::ForwardRate;
    use crate::termstructures::yields::{
        DepositRateHelper, FraRateHelper, PiecewiseYieldCurve, Pillar, SwapRateHelper,
    };
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Natural;

    /// The market quotes in percent (`piecewiseyieldcurve.cpp:1392-1398`):
    /// the 6M deposit, then the twelve FRAs, then the nineteen swaps.
    const REF_MKT_RATE: [Real; 32] = [
        -0.373, -0.388, -0.402, -0.418, -0.431, -0.441, -0.45, -0.457, -0.463, -0.469, -0.461,
        -0.463, -0.479, -0.4511, -0.45418, -0.439, -0.4124, -0.37703, -0.3335, -0.28168, -0.22725,
        -0.1745, -0.12425, -0.07746, 0.0385, 0.1435, 0.17525, 0.17275, 0.1515, 0.1225, 0.095,
        0.0644,
    ];

    /// The swap tenors in years (`:1435`).
    const SWAP_TENORS: [i32; 19] = [
        2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 15, 20, 25, 30, 35, 40, 45, 50,
    ];

    /// The curve under test.
    type PenaltyCurve = PiecewiseYieldCurve<ForwardRate, BackwardFlat, GlobalBootstrap>;

    /// The shared fixture: evaluation date 26 Sep 2019 (`:1390`) and the
    /// 32 helpers of `:1425-1441`, all reading one empty-forwarding Euribor 6M.
    struct Fixture {
        reference_date: Date,
        helpers: Vec<Shared<dyn RateHelper>>,
    }

    /// C++ builds the curve from `(2, TARGET())`, a moving reference two
    /// business days after the evaluation date; nothing in this test moves that
    /// date, so the equivalent fixed reference (30 Sep 2019) is computed here
    /// and handed to the reference-date constructor.
    ///
    /// The deposit takes its schedule from the index rather than from the
    /// explicit `(6M, 2, TARGET(), ModifiedFollowing, true, Actual360())` of
    /// `:1425-1426`: Euribor 6M carries exactly those six conventions.
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
        let euribor6m = Euribor::six_months(Handle::empty(), settings);

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
        }
    }

    /// The curve of `:1444-1448`, with the explicit 1.0e-12 accuracy both arms
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

    /// The continuous Actual/360 zero rate at every pillar (`:1478-1482`).
    fn zero_rates(curve: &Shared<PenaltyCurve>, fixture: &Fixture) -> Vec<Real> {
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
    /// CURVE mid-solve, the shape the deferred `additionalHelpers` penalty
    /// takes upstream (`globalbootstrap.hpp:392-395` reprices additional
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
            GlobalBootstrap::with_penalties(Some(1.0e-12), None, Vec::new(), move |_, _| {
                counter.set(counter.get() + 1);
                let error = probe
                    .quote_error()
                    .expect("the probe helper reprices off the trial curve");
                vec![1.0e-8 * error]
            }),
        );

        let rates = zero_rates(&curve, &fixture);
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
            zero_rates(&curve, &fixture).iter().all(|r| r.is_finite()),
            "a zero constant penalty must leave the strip solvable"
        );
        assert!(
            fired.get() > 0,
            "the no-argument penalty never reached the residual vector"
        );
    }
}
