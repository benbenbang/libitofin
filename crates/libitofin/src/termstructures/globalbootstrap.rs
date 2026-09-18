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
//! The single-curve path is ported in full: the `additionalPenalties`
//! residual terms (#974), the `additionalHelpers`/`additionalDates`
//! restrictions (#976) and the `additionalVariables` optimizer coordinates
//! with their `SimpleQuoteVariables` implementation (#977). It equals the C++
//! path with `parentBootstrapper_` null. The multi-curve machinery is here
//! too: the [`MultiCurveBootstrapContributor`] interface (`hpp:40-49`), the
//! [`MultiCurveBootstrap`] parent it links to (`hpp:51-67`) and the
//! `parentBootstrapper_` branch of `calculate` (`hpp:408-411`). The
//! [`MultiCurve`](crate::termstructures::multicurve::MultiCurve) wrapper that
//! assembles the contributors (`ql/termstructures/multicurve.hpp`) is ported
//! too. Deferred visibly, as its own follow-up issue (#995) referencing #949:
//!
//! - **The two-curve spreaded self-reprice oracle**
//!   (`testMultiCurvePiecewiseYieldCurveAndSpreadedCurve`) and the basis rate
//!   helper it needs, which exercise the joint solve end to end.
//!
//! The C++ optimizer override (`shared_ptr<OptimizationMethod>`) is not
//! carried either: the default `LevenbergMarquardt(accuracy, accuracy,
//! accuracy)` (`globalbootstrap.hpp:225`) is built per run. The Rust
//! [`OptimizationMethod`](crate::math::optimization::method::OptimizationMethod)
//! `minimize` takes `&mut self`, which a stored trait object cannot offer
//! from the `&self` [`Bootstrap::calculate`]; an override can be added if a
//! use case needs it. The
//! `EndCriteria` override is carried (it is `Copy`), defaulting to
//! `EndCriteria(1000, 10, accuracy, accuracy, accuracy)`
//! (`globalbootstrap.hpp:228`) - deliberately different literals from
//! `LocalBootstrap`'s `(100, 10, 0, accuracy, 0)`.
//!
//! ## The C++ method split
//!
//! C++ spreads the driver over `setup`/`initialize`/`setupCostFunction`/
//! `setCostFunctionArgument`/`evaluateCostFunction`/`setToValid`/`calculate`
//! with mutable members carrying state between them. `setupCostFunction`
//! through `setToValid` are kept separable here as private methods that
//! [`Bootstrap::calculate`] recomposes exactly as C++'s `calculate` does
//! (`globalbootstrap.hpp:405-429`), handing each other an explicit owned state
//! instead of members; `setup` and `initialize` fold into the first of them, as
//! for the other bootstrap algorithms:
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
//!   `calculate` - so it lives in
//!   [`MultiCurveBootstrapContributor::setup_cost_function`], the only path
//!   that reaches a contributing curve without going through that `calculate`.
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
use std::rc::Weak;

use crate::errors::{QlError, QlResult};
use crate::math::array::Array;
use crate::math::interpolations::Interpolator;
use crate::math::optimization::constraint::NoConstraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::math::optimization::endcriteria::EndCriteria;
use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use crate::math::optimization::method::OptimizationMethod;
use crate::math::optimization::problem::Problem;
use crate::patterns::observable::{Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, WeakMut};
use crate::termstructures::bootstraphelper::{BootstrapHelperShared, RateHelper};
use crate::termstructures::bootstraptraits::{BootstrapTraits, YieldBootstrapTraits};
use crate::termstructures::iterativebootstrap::{Bootstrap, PiecewiseCurve};
use crate::termstructures::yields::PiecewiseYieldCurve;
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

/// Additional penalties whose failures propagate out of the bootstrap.
pub type FallibleAdditionalPenalties = dyn Fn(&[Time], &[Real]) -> QlResult<Vec<Real>>;

/// Additional node dates whose failures propagate out of the bootstrap.
pub type FallibleAdditionalDates = dyn Fn() -> QlResult<Vec<Date>>;

/// The additional optimizer variables (`AdditionalBootstrapVariables`,
/// `globalbootstrap.hpp:69-76`).
///
/// Coordinates the global solve varies alongside the curve nodes, living
/// OUTSIDE the curve: they are appended after the node coordinates in the
/// optimizer's vector, and each trial point's tail is written back into
/// whatever the implementation drives - upstream, external quotes the rate
/// helpers read ([`SimpleQuoteVariables`]).
///
/// A variable adds a degree of freedom without adding a residual, so a system
/// that gains variables this way needs matching [`AdditionalPenalties`] terms
/// to stay solvable.
///
/// Both methods take `&self`, as the driver holds the variables behind a shared
/// reference through the solve, and both return [`QlResult`] (D4): reading a
/// quote back is fallible.
///
/// [`SimpleQuoteVariables`]: crate::termstructures::globalbootstrapvars::SimpleQuoteVariables
pub trait AdditionalBootstrapVariables {
    /// The initial guesses, in optimizer space, one per variable.
    ///
    /// `valid_data` is the driver's warm-restart flag: on a re-solve over an
    /// unchanged grid the previous solution is still in place, and an
    /// implementation may seed itself from it rather than from its configured
    /// guesses.
    ///
    /// # Errors
    ///
    /// Implementation-defined; the driver propagates it out of `calculate`.
    fn initialize(&self, valid_data: bool) -> QlResult<Vec<Real>>;

    /// Writes a trial point, in optimizer space, back into the variables.
    ///
    /// # Errors
    ///
    /// Implementation-defined; the driver parks it like a failed reprice.
    fn update(&self, x: &[Real]) -> QlResult<()>;
}

/// The global bootstrap (`GlobalBootstrap`, single-curve core).
///
/// Carries the stopping-accuracy override, the `EndCriteria` override, the
/// per-instrument residual weights, the additional penalty terms, and the
/// additional helpers and dates those penalties are built around, and the
/// additional optimizer variables; defaults mirror the C++ constructor
/// (`accuracy = Null`, `endCriteria = nullptr`, `instrumentWeights = {}`, no
/// penalties, no additional restrictions, no additional variables,
/// `globalbootstrap.hpp:112-115`), with everything resolved from the curve at
/// calculation time.
///
/// The boxed penalty closure is neither `Clone` nor `Debug`, so those two
/// derives are gone; `Default` is hand-rolled because a curve built through
/// [`PiecewiseYieldCurve::new`](crate::termstructures::yields::PiecewiseYieldCurve::new)
/// still needs the empty configuration.
pub struct GlobalBootstrap {
    calculating: Cell<bool>,
    additional_helpers: Vec<Shared<dyn RateHelper>>,
    additional_dates: Option<Box<FallibleAdditionalDates>>,
    accuracy: Option<Real>,
    end_criteria: Option<EndCriteria>,
    instrument_weights: Vec<Real>,
    penalties: Option<Box<FallibleAdditionalPenalties>>,
    additional_variables: Option<Box<dyn AdditionalBootstrapVariables>>,
    /// The parent of a contributing curve (`mutable parentBootstrapper_`,
    /// `globalbootstrap.hpp:156`), set by
    /// [`MultiCurveBootstrapContributor::set_parent_bootstrapper`] and read at
    /// the top of [`Bootstrap::calculate`]. Strong on purpose: a `Weak` here
    /// would dangle once the caller drops the wrapper holding the parent, and
    /// `calculate` would then fall back to the single-curve solve without
    /// saying so. The parent holds its contributors weakly, so there is no
    /// cycle.
    parent: RefCell<Option<Shared<MultiCurveBootstrap>>>,
    /// The state of the multi-curve solve in flight, stashed by
    /// [`MultiCurveBootstrapContributor::setup_cost_function`] because the
    /// parent calls the later pieces as separate steps and cannot carry it
    /// between them. The single-curve `calculate` keeps its own local state and
    /// never touches this.
    state: RefCell<Option<GlobalBootstrapState>>,
    /// The [`GlobalCost`] adapter's NaN-length bookkeeping, for the multi-curve
    /// path only: the parent evaluates through the trait rather than through a
    /// [`GlobalCost`], so the cell that adapter owns for the single-curve solve
    /// lives here instead.
    penalty_len: Cell<Size>,
}

struct CalculationGuard<'a>(&'a Cell<bool>);

impl Drop for CalculationGuard<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
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
            calculating: Cell::new(false),
            additional_helpers: Vec::new(),
            additional_dates: None,
            accuracy,
            end_criteria,
            instrument_weights,
            penalties: None,
            additional_variables: None,
            parent: RefCell::new(None),
            state: RefCell::new(None),
            penalty_len: Cell::new(0),
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
    /// can read their `implied_quote`. Its `additionalVariables` argument is a
    /// separate step, [`with_additional_variables`](Self::with_additional_variables).
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
        Self::with_fallible_penalties(
            additional_helpers,
            additional_dates
                .map(|dates| Box::new(move || Ok(dates())) as Box<FallibleAdditionalDates>),
            accuracy,
            end_criteria,
            instrument_weights,
            move |times, data| Ok(penalties(times, data)),
        )
    }

    /// Fallible additional restrictions, preserving callback errors during fitting.
    ///
    /// Existing infallible constructors delegate here. Callback failures invalidate
    /// the calculation and are returned by the next curve query.
    pub fn with_fallible_penalties<F>(
        additional_helpers: Vec<Shared<dyn RateHelper>>,
        additional_dates: Option<Box<FallibleAdditionalDates>>,
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
        instrument_weights: Vec<Real>,
        penalties: F,
    ) -> GlobalBootstrap
    where
        F: Fn(&[Time], &[Real]) -> QlResult<Vec<Real>> + 'static,
    {
        GlobalBootstrap {
            calculating: Cell::new(false),
            additional_helpers,
            additional_dates,
            accuracy,
            end_criteria,
            instrument_weights,
            penalties: Some(Box::new(penalties)),
            additional_variables: None,
            parent: RefCell::new(None),
            state: RefCell::new(None),
            penalty_len: Cell::new(0),
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

    /// Attaches the [`AdditionalBootstrapVariables`] the solve varies alongside
    /// the curve nodes (the `additionalVariables` argument of C++ constructors
    /// #2 and #3, `globalbootstrap.hpp:122`/`:130`).
    ///
    /// A consuming builder rather than a further constructor parameter: the
    /// C++ argument is trailing and defaulted, so every existing call site
    /// means "no additional variables", and a builder step keeps them
    /// unchanged.
    #[must_use]
    pub fn with_additional_variables(
        mut self,
        variables: Box<dyn AdditionalBootstrapVariables>,
    ) -> GlobalBootstrap {
        self.additional_variables = Some(variables);
        self
    }
}

/// The multi-curve contributor interface (`MultiCurveBootstrapContributor`,
/// `globalbootstrap.hpp:40-49`): the five entry points a
/// [`MultiCurveBootstrap`] drives on every curve it joins into one solve. They
/// are the four pieces the single-curve [`Bootstrap::calculate`] recomposes,
/// re-exposed one at a time, plus the parent link.
///
/// The C++ methods are `const` and infallible. Here they return [`QlResult`]
/// (D4): each one reaches fallible Rust code - an interpolation rebuild, a
/// reprice, a quote read - and the stacked cost closure needs the same
/// fallible-to-NaN bridge the single-curve [`CostFunction`] adapter applies.
pub trait MultiCurveBootstrapContributor {
    /// `setParentBootstrapper` (`globalbootstrap.hpp:209-211`): links this
    /// contributor to the parent that drives it, so a later query on it runs
    /// the joint solve rather than its own single-curve one.
    fn set_parent_bootstrapper(&self, parent: Shared<MultiCurveBootstrap>);

    /// `setupCostFunction` (`globalbootstrap.hpp:319-374`): marks the curve
    /// calculated (`:324`), installs the grid and returns this contributor's
    /// guess for the parent to concatenate. The state the three methods below
    /// read is stashed here, because the parent calls them as separate steps.
    ///
    /// # Errors
    ///
    /// Whatever the setup step fails on: an invalid quote, too few curve
    /// points, an interpolation rebuild.
    fn setup_cost_function(&self) -> QlResult<Array>;

    /// `setCostFunctionArgument` (`globalbootstrap.hpp:376-387`): writes this
    /// contributor's slice of the trial point into its curve.
    ///
    /// # Errors
    ///
    /// A failed interpolation rebuild, or a failed additional-variable write.
    fn set_cost_function_argument(&self, x: &[Real]) -> QlResult<()>;

    /// `evaluateCostFunction` (`globalbootstrap.hpp:389-400`): this
    /// contributor's residuals, for the parent to concatenate.
    ///
    /// # Errors
    ///
    /// A failed reprice, or a call before [`setup_cost_function`](Self::setup_cost_function).
    fn evaluate_cost_function(&self) -> QlResult<Array>;

    /// `setToValid` (`globalbootstrap.hpp:213`), taking this contributor's
    /// slice of the solved vector.
    ///
    /// Divergence: the C++ method is argument-free, because its parent never
    /// re-applies the solution and simply leaves each curve at the optimizer's
    /// last trial point (`globalbootstrap.cpp:114-115`). This port pins the
    /// solution, exactly as the single-curve path does, so the parent must
    /// offset-slice the global solution per contributor and hand each its own.
    ///
    /// # Errors
    ///
    /// A failed interpolation rebuild, or a call before
    /// [`setup_cost_function`](Self::setup_cost_function).
    fn set_to_valid(&self, solution: &[Real]) -> QlResult<()>;

    /// Undoes the "calculated" mark that
    /// [`setup_cost_function`](Self::setup_cost_function) set, without
    /// notifying anyone.
    ///
    /// Divergence, and a Rust-only one: C++ has no counterpart. When a later
    /// contributor's setup throws, `runMultiCurveBootstrap` leaves the earlier
    /// ones marked calculated over a partial grid and leaves the consequences
    /// to the caller, a known rough edge upstream. This port reverts them
    /// instead, so [`MultiCurveBootstrap::run`] never returns `Err` with a
    /// contributor still marked calculated: D4 makes the failure explicit, and
    /// D10 declines to hand a caller a half-marked curve that the next query
    /// would read as solved.
    ///
    /// The error path only. On success nothing is reverted and the sequence is
    /// C++'s, so no result any caller can observe changes.
    fn invalidate(&self);
}

/// The multi-curve parent (`MultiCurveBootstrap`, `globalbootstrap.hpp:51-67`,
/// definitions `globalbootstrap.cpp:26-116`): it holds the contributing curves
/// and joins their cost functions into one least-squares solve.
///
/// The C++ optimizer override (`shared_ptr<OptimizationMethod>`, `hpp:63`) is
/// not carried, for the reason the module doc gives for `GlobalBootstrap`'s:
/// `minimize` takes `&mut self`, which a stored trait object cannot offer from
/// the `&self` of [`run`](Self::run). The accuracy and the [`EndCriteria`]
/// override are carried instead, mirroring [`GlobalBootstrap`], and both are
/// resolved inside `run`; an unset accuracy takes the C++ literal `1e-10`
/// (`globalbootstrap.cpp:35`), there being no curve here to fall back to.
///
/// `setOtherContributorsToValid` (`hpp:59`) and `finalizeCalculation`
/// (`hpp:60`) are declared in C++ and defined nowhere in the tree, so they are
/// omitted rather than invented.
pub struct MultiCurveBootstrap {
    accuracy: Option<Real>,
    end_criteria: Option<EndCriteria>,
    /// The contributing curves (`contributors_`, `globalbootstrap.hpp:65`).
    /// Weak, mirroring the C++ raw `const*`: a contributor holds its parent
    /// strongly, so an owning link here would close the cycle.
    contributors: RefCell<Vec<Weak<dyn MultiCurveBootstrapContributor>>>,
    /// The observers notified between the set and evaluate phases of the
    /// stacked solve (`observers_`, `globalbootstrap.hpp:66`).
    observers: RefCell<Vec<WeakMut<dyn Observer>>>,
    /// How many times [`run`](Self::run) has been entered, so a test can see
    /// that a contributor's `calculate` routed to the joint solve rather than
    /// running its own single-curve one. C++ needs no such counter: there, the
    /// routing is visible in a debugger and nowhere else.
    runs: Cell<Size>,
}

impl MultiCurveBootstrap {
    /// The accuracy constructor (`globalbootstrap.cpp:26-29`), which upstream
    /// builds both the optimizer and the `EndCriteria` from the one number.
    pub fn new(accuracy: Real) -> MultiCurveBootstrap {
        MultiCurveBootstrap::configured(Some(accuracy), None)
    }

    /// The override constructor (`globalbootstrap.cpp:31-39`) minus its
    /// dropped optimizer argument: an explicit [`EndCriteria`], or `None` for
    /// the `1e-10` default. That default is built in [`run`](Self::run) rather
    /// than here because `EndCriteria::new` is fallible and a constructor that
    /// cannot fail is the more useful one.
    pub fn with_end_criteria(end_criteria: Option<EndCriteria>) -> MultiCurveBootstrap {
        MultiCurveBootstrap::configured(None, end_criteria)
    }

    fn configured(
        accuracy: Option<Real>,
        end_criteria: Option<EndCriteria>,
    ) -> MultiCurveBootstrap {
        MultiCurveBootstrap {
            accuracy,
            end_criteria,
            contributors: RefCell::new(Vec::new()),
            observers: RefCell::new(Vec::new()),
            runs: Cell::new(0),
        }
    }

    /// `add` (`globalbootstrap.cpp:41-44`): registers a contributing curve and
    /// links it back to this parent.
    ///
    /// C++ reaches its own `shared_ptr` through `enable_shared_from_this`,
    /// which Rust has no equivalent of from `&self`, so the parent is taken as
    /// the [`Shared`] the caller already holds.
    pub fn add(self: &Shared<Self>, contributor: &Shared<dyn MultiCurveBootstrapContributor>) {
        self.contributors
            .borrow_mut()
            .push(Shared::downgrade(contributor));
        contributor.set_parent_bootstrapper(Shared::clone(self));
    }

    /// `addObserver` (`globalbootstrap.cpp:46-48`): an observer the stacked
    /// solve notifies between its set and evaluate phases.
    pub fn add_observer(&self, observer: &SharedMut<dyn Observer>) {
        self.observers
            .borrow_mut()
            .push(SharedMut::downgrade(observer));
    }

    /// The number of contributing curves registered by [`add`](Self::add), for
    /// the `MultiCurve` wrapper's wiring tests.
    #[cfg(test)]
    pub(crate) fn contributor_count(&self) -> usize {
        self.contributors.borrow().len()
    }

    /// The number of observers registered by
    /// [`add_observer`](Self::add_observer), for the `MultiCurve` wrapper's
    /// wiring tests.
    #[cfg(test)]
    pub(crate) fn observer_count(&self) -> usize {
        self.observers.borrow().len()
    }

    /// `runMultiCurveBootstrap` (`globalbootstrap.cpp:50-116`): the stacked
    /// solve over every contributor at once.
    ///
    /// Each contributor sets up and contributes its guess block (`cpp:52-59`);
    /// one least-squares solve then drives the concatenation, splitting every
    /// trial point back into the blocks (`cpp:61-105`); finally each
    /// contributor is pinned with its slice of the solution (`cpp:114-115`).
    ///
    /// The solver's cost function is infallible while the contributors are not,
    /// so [`StackedCost`] parks the first failure and answers with NaN
    /// residuals - the same bridge the single-curve [`GlobalCost`] uses - and
    /// the parked error is raised here, after `minimize` returns.
    ///
    /// Divergence: on any failure this reverts the "calculated" mark on every
    /// contributor it had already set up. C++ leaves them marked (`hpp:324`
    /// sets the flag before anything can fail) and lets the caller live with
    /// half-solved curves reading as calculated.
    ///
    /// # Errors
    ///
    /// A contributor dropped since [`add`](Self::add), any failure inside a
    /// contributor's four solve steps, or a solve that does not reach the
    /// required accuracy.
    pub fn run(&self) -> QlResult<()> {
        self.runs.set(self.runs.get() + 1);

        // Upgraded once and held for the whole solve (`contributors_` is a raw
        // `const*` upstream, so C++ has nothing to upgrade).
        let mut contributors: Vec<Shared<dyn MultiCurveBootstrapContributor>> = Vec::new();
        for contributor in self.contributors.borrow().iter() {
            let Some(contributor) = contributor.upgrade() else {
                crate::fail!("multi-curve bootstrap: a contributing curve was dropped");
            };
            contributors.push(contributor);
        }

        let mut set_up = 0;
        let outcome = self.solve(&contributors, &mut set_up);
        if outcome.is_err() {
            for contributor in &contributors[..set_up] {
                contributor.invalidate();
            }
        }
        outcome
    }

    /// The body of [`run`](Self::run), split out so the revert on failure sits
    /// in one place. `set_up` counts the contributors whose
    /// `setup_cost_function` was entered, which is what the caller reverts; it
    /// is incremented before the call because a contributor marks its curve
    /// calculated before it can fail (`globalbootstrap.hpp:324`).
    fn solve(
        &self,
        contributors: &[Shared<dyn MultiCurveBootstrapContributor>],
        set_up: &mut Size,
    ) -> QlResult<()> {
        // The guess concatenation (`globalbootstrap.cpp:52-59`), whose block
        // sizes are the offsets every later split uses.
        let mut guess: Vec<Real> = Vec::new();
        let mut sizes: Vec<Size> = Vec::new();
        for contributor in contributors {
            *set_up += 1;
            let block = contributor.setup_cost_function()?;
            sizes.push(block.size());
            guess.extend(block.iter().copied());
        }
        let guess = Array::from(guess);

        // Solver configuration (`globalbootstrap.cpp:35-39`, `:102-105`): the
        // literals are C++'s, with `1e-10` standing in for the curve accuracy
        // the single-curve path falls back to, there being no curve here.
        let accuracy = self.accuracy.unwrap_or(1.0e-10);
        let mut optimizer = LevenbergMarquardt::new(accuracy, accuracy, accuracy, false);
        let end_criteria = match self.end_criteria {
            Some(criteria) => criteria,
            None => EndCriteria::new(1000, Some(10), accuracy, accuracy, Some(accuracy))?,
        };

        let cost = StackedCost {
            contributors,
            sizes: &sizes,
            observers: &self.observers,
            error: RefCell::new(None),
            last_len: Cell::new(guess.size()),
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
            "global bootstrap failed to minimize to required accuracy (during multi curve bootstrap): {end_type}"
        );

        // The validity sweep (`globalbootstrap.cpp:114-115`), which upstream
        // takes no argument: this port re-pins each contributor from its slice
        // of the solution, as the single-curve path re-pins its nodes.
        let solved: &[Real] = &solution;
        let mut offset = 0;
        for (contributor, size) in contributors.iter().zip(&sizes) {
            contributor.set_to_valid(&solved[offset..offset + size])?;
            offset += size;
        }
        Ok(())
    }
}

/// The stacked cost (`runMultiCurveBootstrap`'s closure,
/// `globalbootstrap.cpp:61-100`): it splits each trial point into the
/// contributors' blocks, writes them all, notifies the observers, and only
/// then collects the contributors' residuals into one vector.
///
/// The two phases are separate loops on purpose (`cpp:70` / `:74-75` / `:82`).
/// A contributor's `evaluate` reads curves other contributors own, so every
/// write and the `borrow_mut` it takes must be finished before any read starts;
/// fusing the loops would take a shared borrow of a cell still mutably borrowed
/// and panic.
///
/// The notify loop walks `observers` and never `contributors`: a contributing
/// curve's own updater would clear the lazy flag `setup_cost_function` set, and
/// the next helper read inside the solve would route back through the parent
/// branch of `calculate` into [`MultiCurveBootstrap::run`].
///
/// Like [`GlobalCost`], it bridges fallible contributors to the infallible
/// [`CostFunction`]: the first error is parked and the evaluation answers with
/// NaN residuals, sized from the last successful evaluation or, before there is
/// one, from the guess - any non-empty length will do, since
/// [`CostFunction::value`] panics only on an empty one.
struct StackedCost<'a> {
    contributors: &'a [Shared<dyn MultiCurveBootstrapContributor>],
    sizes: &'a [Size],
    observers: &'a RefCell<Vec<WeakMut<dyn Observer>>>,
    error: RefCell<Option<QlError>>,
    last_len: Cell<Size>,
}

impl StackedCost<'_> {
    fn try_values(&self, x: &Array) -> QlResult<Array> {
        let trial: &[Real] = x;
        let mut offset = 0;
        for (contributor, size) in self.contributors.iter().zip(self.sizes) {
            contributor.set_cost_function_argument(&trial[offset..offset + size])?;
            offset += size;
        }

        // Upgraded out of the list before any `update` runs, so an observer
        // reaching back into the parent does not find `observers` borrowed.
        let observers: Vec<SharedMut<dyn Observer>> = self
            .observers
            .borrow()
            .iter()
            .filter_map(WeakMut::upgrade)
            .collect();
        for observer in observers {
            observer.borrow_mut().update();
        }

        let mut residuals: Vec<Real> = Vec::new();
        for contributor in self.contributors {
            residuals.extend(contributor.evaluate_cost_function()?.iter().copied());
        }
        Ok(Array::from(residuals))
    }
}

impl CostFunction for StackedCost<'_> {
    fn values(&self, x: &Array) -> Array {
        match self.try_values(x) {
            Ok(values) => {
                self.last_len.set(values.size());
                values
            }
            Err(err) => {
                let mut slot = self.error.borrow_mut();
                if slot.is_none() {
                    *slot = Some(err);
                }
                Array::filled(self.last_len.get(), Real::NAN)
            }
        }
    }
}

/// The state `setup` builds and the three later pieces read: the alive
/// helpers, their weights, and the interior node count of the grid it
/// installed. C++ carries the first two as the mutable members
/// `aliveInstruments_` and `aliveInstrumentWeights_` (`globalbootstrap.hpp:148`
/// and `:154`) and reads the third off the curve; the Rust pieces take `&self`
/// on a shared bootstrap, so what they hand each other is explicit and owned.
struct GlobalBootstrapState {
    alive: Vec<Shared<dyn RateHelper>>,
    alive_weights: Vec<Real>,
    /// The number of interior nodes, `times.len() - 1`, and the number of
    /// variables the solve carries; the residual count is `alive.len()` plus
    /// the penalty terms. The two are equal for a strip of distinct pillars
    /// and no additional dates; each additional date adds one variable the
    /// penalty terms have to answer for, and the least-squares solver rejects
    /// a system left with fewer residuals than variables.
    interior: Size,
}

impl GlobalBootstrap {
    /// `setupCostFunction` (`globalbootstrap.hpp:319-374`) with the `setup`
    /// guard and `initialize` (`:232-236`, `:244-315`) folded into it: the
    /// weights guard, the alive filters, the pillar grid and its installation,
    /// the hand-over of the curve to the helpers, and the initial guess.
    fn setup<C>(&self, curve: &C) -> QlResult<(GlobalBootstrapState, Array)>
    where
        C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
        C::Traits: YieldBootstrapTraits,
    {
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
            Some(dates) => dates()?
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

        // The initial guess (`:360-374`): the additional variables initialize
        // FIRST, then each interior node is updated through Traits::guess -
        // which depends on the previously updated nodes, so the writes are
        // sequential - and mapped into the optimizer space. The additional
        // guesses are APPENDED after the node guesses, which is the layout the
        // cost function's `x[interior..]` split relies on.
        let additional_guesses = match &self.additional_variables {
            Some(variables) => variables.initialize(valid_data)?,
            None => Vec::new(),
        };
        let mut guess = Array::with_size(interior + additional_guesses.len());
        {
            let mut cd = curve.curve_data().borrow_mut();
            for i in 0..interior {
                let g = C::Traits::guess(i + 1, cd.times(), cd.data(), valid_data);
                C::Traits::update_guess(cd.data_mut(), g, i + 1);
                guess[i] = C::Traits::transform_inverse(cd.data()[i + 1], cd.times()[i + 1]);
            }
            cd.rebuild(curve.interpolator(), interior)?;
        }
        for (i, additional_guess) in additional_guesses.into_iter().enumerate() {
            guess[interior + i] = additional_guess;
        }

        Ok((
            GlobalBootstrapState {
                alive,
                alive_weights,
                interior,
            },
            guess,
        ))
    }

    /// `setCostFunctionArgument` (`globalbootstrap.hpp:376-387`): the interior
    /// trial nodes and the full-grid rebuild inside a scoped `borrow_mut`, then
    /// the trial point's tail to the additional variables.
    fn set_argument<C>(&self, curve: &C, state: &GlobalBootstrapState, x: &[Real]) -> QlResult<()>
    where
        C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
        C::Traits: YieldBootstrapTraits,
    {
        {
            let mut cd = curve.curve_data().borrow_mut();
            for (i, &coordinate) in x[..state.interior].iter().enumerate() {
                let t = cd.times()[i + 1];
                let value = C::Traits::transform_direct(coordinate, t);
                C::Traits::update_guess(cd.data_mut(), value, i + 1);
            }
            cd.rebuild(curve.interpolator(), state.interior)?;
        }
        // The trial point's tail goes to the additional variables (`:386-388`),
        // still inside the C++ `setCostFunctionArgument` step and so before the
        // penalties. It writes no curve cell - upstream it writes external
        // quotes - but it does notify, which is why the helpers reaching those
        // quotes must hold them through an unregistered handle
        // (`Handle::new_unregistered`); an observing one would invalidate the
        // curve on every evaluation.
        if let Some(variables) = &self.additional_variables {
            variables.update(&x[state.interior..])?;
        }
        Ok(())
    }

    /// `evaluateCostFunction` (`globalbootstrap.hpp:389-400`): the penalty
    /// terms, then the alive helpers' weighted quote errors, as one residual
    /// vector. `penalty_len` is the solver adapter's NaN-length bookkeeping,
    /// written here because only this step knows the penalty count.
    fn evaluate<C>(
        &self,
        curve: &C,
        state: &GlobalBootstrapState,
        penalty_len: &Cell<Size>,
    ) -> QlResult<Array>
    where
        C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
        C::Traits: YieldBootstrapTraits,
    {
        // Only now that the `borrow_mut` of `set_argument` has dropped, so a
        // penalty that reads the curve back can take its own shared borrow
        // (`:392-395`).
        let penalty_errors = match &self.penalties {
            Some(penalties) => {
                let cd = curve.curve_data().borrow();
                penalties(cd.times(), cd.data())?
            }
            None => Vec::new(),
        };
        penalty_len.set(penalty_errors.len());

        let mut residuals = Array::with_size(state.alive.len() + penalty_errors.len());
        for (i, helper) in state.alive.iter().enumerate() {
            residuals[i] = helper.quote_error()? * state.alive_weights[i];
        }
        for (i, penalty_error) in penalty_errors.into_iter().enumerate() {
            residuals[state.alive.len() + i] = penalty_error;
        }
        Ok(residuals)
    }

    /// `setToValid` (`globalbootstrap.hpp:213`), which upstream is the bare
    /// `validCurve_ = true` because C++ leaves the curve at the optimizer's
    /// last trial point. This port pins the solution first, so the piece takes
    /// it.
    fn set_to_valid<C>(
        &self,
        curve: &C,
        state: &GlobalBootstrapState,
        solution: &[Real],
    ) -> QlResult<()>
    where
        C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
        C::Traits: YieldBootstrapTraits,
    {
        // Pin the returned solution: rewrite every interior node from the
        // optimizer's answer and rebuild, so the curve holds the solution
        // rather than the solver's last trial point; then mark the data as a
        // valid seed for the next bootstrap (`validCurve_ = true`, `:428`).
        {
            let mut cd = curve.curve_data().borrow_mut();
            for (i, &coordinate) in solution[..state.interior].iter().enumerate() {
                let t = cd.times()[i + 1];
                let value = C::Traits::transform_direct(coordinate, t);
                C::Traits::update_guess(cd.data_mut(), value, i + 1);
            }
            cd.rebuild(curve.interpolator(), state.interior)?;
            cd.set_valid(true);
        }
        // The additional variables are pinned to the solution too, after that
        // `borrow_mut` has dropped. C++ has no pin loop at all and simply
        // leaves the quotes at the optimizer's last trial point; this port
        // rewrites the nodes from the returned solution, so the quotes are
        // rewritten from the same vector and the two stay consistent.
        if let Some(variables) = &self.additional_variables {
            variables.update(&solution[state.interior..])?;
        }
        Ok(())
    }
}

/// The global cost: the solver-side adapter that pairs `set_argument` with
/// `evaluate` on every trial point, exactly as the C++ cost closure calls
/// `setCostFunctionArgument` then `evaluateCostFunction`
/// (`globalbootstrap.hpp:414-417`).
///
/// The [`CostFunction`] trait is infallible, so a failed rebuild or reprice
/// parks its error in `error` and returns NaN residuals, which the solver
/// adapter treats as an infeasible penalty; the driver surfaces the parked
/// error after the solve (D4), matching the C++ exception propagating out of
/// the cost closure.
struct GlobalCost<'a, C: PiecewiseCurve> {
    bootstrap: &'a GlobalBootstrap,
    curve: &'a C,
    state: &'a GlobalBootstrapState,
    /// The penalty-term count of the last evaluation, so a failed evaluation's
    /// NaN vector has the length the solver sized itself on.
    penalty_len: Cell<Size>,
    error: RefCell<Option<QlError>>,
}

impl<C> GlobalCost<'_, C>
where
    C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
    C::Traits: YieldBootstrapTraits,
{
    fn try_values(&self, x: &Array) -> QlResult<Array> {
        self.bootstrap.set_argument(self.curve, self.state, x)?;
        self.bootstrap
            .evaluate(self.curve, self.state, &self.penalty_len)
    }
}

impl<C> CostFunction for GlobalCost<'_, C>
where
    C: PiecewiseCurve<Helper = dyn RateHelper, TS = dyn YieldTermStructure>,
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
                let residuals = self.state.alive.len() + self.penalty_len.get();
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
        // The parent branch (`globalbootstrap.hpp:408-411`): a contributing
        // curve runs the joint solve instead of its own. The parent is bound
        // out of the `RefCell` before the call because `run` drives this
        // bootstrap's own cells; `setup_cost_function` marks the curve
        // calculated (`:324`), so those callbacks do not re-enter here.
        let parent = self.parent.borrow().clone();
        if let Some(parent) = parent {
            parent.run()?;
            return Ok(());
        }

        require!(
            !self.calculating.replace(true),
            "global bootstrap re-entered during calculation; additional-variable helpers must use unregistered quote handles"
        );
        let _guard = CalculationGuard(&self.calculating);
        let (state, guess) = self.setup(curve)?;

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
            bootstrap: self,
            curve,
            state: &state,
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

        self.set_to_valid(curve, &state, &solution)
    }
}

impl<T: YieldBootstrapTraits + 'static, I: Interpolator + 'static> MultiCurveBootstrapContributor
    for PiecewiseYieldCurve<T, I, GlobalBootstrap>
{
    fn set_parent_bootstrapper(&self, parent: Shared<MultiCurveBootstrap>) {
        *self.bootstrap().parent.borrow_mut() = Some(parent);
    }

    fn setup_cost_function(&self) -> QlResult<Array> {
        // `ts_->setCalculated(true)` comes first (`globalbootstrap.hpp:324`):
        // the parent reaches a contributing curve without going through its
        // `calculate`, so this is what stops a mid-solve read of that curve
        // from re-entering `calculate` and recursing into `run`.
        self.mark_calculated();
        let bootstrap = self.bootstrap();
        let (state, guess) = bootstrap.setup(self)?;
        *bootstrap.state.borrow_mut() = Some(state);
        Ok(guess)
    }

    fn set_cost_function_argument(&self, x: &[Real]) -> QlResult<()> {
        let bootstrap = self.bootstrap();
        let state = bootstrap.state.borrow();
        let Some(state) = state.as_ref() else {
            crate::fail!(
                "multi-curve contributor has no state: set_cost_function_argument ran before setup_cost_function"
            );
        };
        bootstrap.set_argument(self, state, x)
    }

    fn evaluate_cost_function(&self) -> QlResult<Array> {
        let bootstrap = self.bootstrap();
        let state = bootstrap.state.borrow();
        let Some(state) = state.as_ref() else {
            crate::fail!(
                "multi-curve contributor has no state: evaluate_cost_function ran before setup_cost_function"
            );
        };
        bootstrap.evaluate(self, state, &bootstrap.penalty_len)
    }

    fn set_to_valid(&self, solution: &[Real]) -> QlResult<()> {
        let bootstrap = self.bootstrap();
        let state = bootstrap.state.borrow();
        let Some(state) = state.as_ref() else {
            crate::fail!(
                "multi-curve contributor has no state: set_to_valid ran before setup_cost_function"
            );
        };
        bootstrap.set_to_valid(self, state, solution)
    }

    fn invalidate(&self) {
        self.invalidate_silently();
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
    //!
    //! Second oracle: `testGlobalBootstrap` (`:1306-1386`) - the same strip
    //! under `PiecewiseYieldCurve<SimpleZeroYield, Linear, GlobalBootstrap>`,
    //! now with the seven additional helpers, the additional dates and the
    //! penalty that ties the helpers' implied quotes to a line. Its 32 rates
    //! come from the same harness and match it to 1.9e-16.
    //!
    //! HONEST NEGATIVES, neither faked into an arm. Both concern branches the
    //! upstream fixture never reaches:
    //!
    //! - The **alive filter on additional helpers** (`hpp:256-262`): all seven
    //!   start twelve months out, so all seven are alive and the drop branch is
    //!   unexercised. Its instrument-side twin is the same line of code the
    //!   32 instruments already run.
    //! - The **invalid-quote guard on additional helpers** (`hpp:348-351`):
    //!   every additional quote is a valid `SimpleQuote`, so the guard is
    //!   unexercised. Reaching it needs an empty quote handle, which no
    //!   upstream test builds here.

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
    use crate::termstructures::globalbootstrapvars::SimpleQuoteVariables;
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

    #[test]
    fn fallible_callbacks_propagate_and_retry_after_failure() {
        for fail_dates in [false, true] {
            let fixture = fixture();
            let fail = shared(Cell::new(true));
            let date_fail = Shared::clone(&fail);
            let penalty_fail = Shared::clone(&fail);
            let curve = curve_with(
                &fixture,
                GlobalBootstrap::with_fallible_penalties(
                    Vec::new(),
                    Some(Box::new(move || {
                        require!(!fail_dates || !date_fail.get(), "date callback failed");
                        Ok(Vec::new())
                    })),
                    Some(1.0e-12),
                    None,
                    Vec::new(),
                    move |_, _| {
                        require!(fail_dates || !penalty_fail.get(), "penalty callback failed");
                        Ok(Vec::new())
                    },
                ),
            );
            for _ in 0..2 {
                let error = curve
                    .dates()
                    .expect_err("callback failure must remain visible");
                assert!(error.message().contains("callback failed"));
            }
            fail.set(false);
            curve
                .dates()
                .expect("a corrected callback retries the failed solve");
            for helper in &fixture.helpers {
                assert!(helper.quote_error().expect("helper reprices").abs() < 1.0e-9);
            }
        }
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

    /// A curve linked to a parent must run the JOINT solve and not its own
    /// (`globalbootstrap.hpp:408-411`).
    ///
    /// This is the guard against the silent-wrong the parent link exists to
    /// prevent: a contributor that quietly kept solving alone would still
    /// answer every query with plausible numbers, and no repricing assertion
    /// downstream would notice, because a single-curve solve reprices its own
    /// helpers perfectly. So the pin is on the ROUTING, and it is
    /// self-discriminating: the linked arm must bump the parent's run count and
    /// come back solved through the joint path, while the unlinked arm must
    /// leave the count alone and solve itself. The counter DELTA between the
    /// two arms is the discriminator, not its absolute value.
    ///
    /// The linked arm asserts the helpers reprice rather than comparing node
    /// vectors: the joint solve runs at the PARENT's accuracy, not the curve's,
    /// so its answer is a different point of the same basin than the
    /// single-curve solve's and the two node vectors do not agree bit for bit.
    /// The helpers are checked before the unlinked curve is built, because both
    /// curves are fitted to the same shared helper objects and the second setup
    /// re-points them.
    #[test]
    fn a_parent_link_routes_calculate_to_the_joint_solve() {
        let fixture = fixture();
        let parent = shared(MultiCurveBootstrap::new(1.0e-10));

        let linked = curve_with(
            &fixture,
            GlobalBootstrap::new(Some(1.0e-12), None, Vec::new()),
        );
        parent.add(&(Shared::clone(&linked) as Shared<dyn MultiCurveBootstrapContributor>));
        linked
            .calculate()
            .expect("the linked curve routes to the parent");
        assert_eq!(
            parent.runs.get(),
            1,
            "a linked curve's calculate did not route to the joint solve"
        );
        for (i, helper) in fixture.helpers.iter().enumerate() {
            let error = helper
                .quote_error()
                .expect("every helper reprices off the jointly solved curve");
            assert!(
                error.abs() < 1.0e-9,
                "helper {i} does not reprice off the jointly solved curve: {error}"
            );
        }

        let alone = curve_with(
            &fixture,
            GlobalBootstrap::new(Some(1.0e-12), None, Vec::new()),
        );
        alone.calculate().expect("an unlinked curve solves itself");
        assert_eq!(
            parent.runs.get(),
            1,
            "an unlinked curve reached a parent it was never added to"
        );
        assert_eq!(
            alone.curve_data().borrow().data().len(),
            fixture.helpers.len() + 1,
            "the unlinked curve did not solve its own nodes"
        );
    }

    /// `setup_cost_function` marks the curve calculated (`:324`), and that is
    /// what stops a mid-solve read of a contributing curve from re-entering
    /// `calculate` and recursing into the joint run.
    ///
    /// The parent reaches a contributor directly, never through the curve's own
    /// `calculate`, so the lazy flag the single-curve path sets in
    /// `start_calculation` is never set on this route. Dropping the
    /// `mark_calculated` call sends this `calculate` to `parent.run` and the
    /// count reads 1.
    #[test]
    fn setting_up_a_contributor_marks_its_curve_calculated() {
        let fixture = fixture();
        let parent = shared(MultiCurveBootstrap::new(1.0e-10));
        let curve = curve_with(
            &fixture,
            GlobalBootstrap::new(Some(1.0e-12), None, Vec::new()),
        );
        parent.add(&(Shared::clone(&curve) as Shared<dyn MultiCurveBootstrapContributor>));

        let guess = curve
            .setup_cost_function()
            .expect("the contributor installs its grid");
        assert_eq!(
            guess.size(),
            fixture.helpers.len(),
            "the guess must hold one coordinate per interior node"
        );

        curve
            .calculate()
            .expect("an already-calculated curve returns at once");
        assert_eq!(
            parent.runs.get(),
            0,
            "a curve marked calculated by setup_cost_function re-entered the joint solve"
        );
    }

    /// `add` (`globalbootstrap.cpp:41-44`) is a round trip: the parent records
    /// the contributor and the contributor records the parent.
    ///
    /// The two directions differ on purpose (D1/D3): the parent holds a `Weak`,
    /// mirroring the C++ raw `const*`, while the contributor holds a strong
    /// [`Shared`], mirroring the C++ `shared_ptr` member at `hpp:156`. A `Weak`
    /// on the contributor's side would dangle as soon as the caller dropped the
    /// parent, and the curve would fall back to solving alone.
    #[test]
    fn add_records_the_contributor_weakly_and_the_parent_strongly() {
        let fixture = fixture();
        let parent = shared(MultiCurveBootstrap::new(1.0e-10));
        let curve = curve_with(
            &fixture,
            GlobalBootstrap::new(Some(1.0e-12), None, Vec::new()),
        );

        parent.add(&(Shared::clone(&curve) as Shared<dyn MultiCurveBootstrapContributor>));

        let linked = curve.bootstrap().parent.borrow();
        let linked = linked.as_ref().expect("add did not link the parent back");
        assert!(
            Shared::ptr_eq(linked, &parent),
            "the contributor was linked to a different parent"
        );
        let contributors = parent.contributors.borrow();
        assert_eq!(contributors.len(), 1, "the parent recorded no contributor");
        assert!(
            contributors[0].upgrade().is_some(),
            "the parent's weak contributor does not resolve"
        );
    }

    /// A stand-in [`MultiCurveBootstrapContributor`] for the mechanism test.
    ///
    /// It owns `targets.len()` variables, guesses zero for each, and returns
    /// `x[i] - targets[i]` as its residuals, so the stacked solve drives it to
    /// its targets and the arithmetic stays checkable by eye. Real curves would
    /// not do: the thing under test is the ORDER and the SLICING the parent
    /// imposes across contributors, and a curve answers every question about
    /// its own numbers while saying nothing about either.
    ///
    /// Every call goes into a log shared with the other mocks, which is what
    /// makes the interleaving across contributors observable at all.
    struct MockContributor {
        id: usize,
        targets: Vec<Real>,
        log: Shared<RefCell<Vec<(usize, &'static str)>>>,
        trials: RefCell<Vec<Vec<Real>>>,
        solutions: RefCell<Vec<Vec<Real>>>,
        current: RefCell<Vec<Real>>,
        invalidations: Cell<usize>,
        setup_fails: bool,
    }

    type MockLog = Shared<RefCell<Vec<(usize, &'static str)>>>;

    impl MockContributor {
        fn new(id: usize, targets: Vec<Real>, log: &MockLog) -> Shared<MockContributor> {
            MockContributor::configured(id, targets, log, false)
        }

        /// The same, but its `setup_cost_function` fails, which is what the
        /// error-path arm drives.
        fn failing(id: usize, targets: Vec<Real>, log: &MockLog) -> Shared<MockContributor> {
            MockContributor::configured(id, targets, log, true)
        }

        fn configured(
            id: usize,
            targets: Vec<Real>,
            log: &MockLog,
            setup_fails: bool,
        ) -> Shared<MockContributor> {
            shared(MockContributor {
                id,
                targets,
                log: Shared::clone(log),
                trials: RefCell::new(Vec::new()),
                solutions: RefCell::new(Vec::new()),
                current: RefCell::new(Vec::new()),
                invalidations: Cell::new(0),
                setup_fails,
            })
        }

        fn record(&self, what: &'static str) {
            self.log.borrow_mut().push((self.id, what));
        }
    }

    impl MultiCurveBootstrapContributor for MockContributor {
        fn set_parent_bootstrapper(&self, _parent: Shared<MultiCurveBootstrap>) {}

        fn setup_cost_function(&self) -> QlResult<Array> {
            self.record("setup");
            if self.setup_fails {
                crate::fail!("mock contributor {} refuses to set up", self.id);
            }
            *self.current.borrow_mut() = vec![0.0; self.targets.len()];
            Ok(Array::with_size(self.targets.len()))
        }

        fn set_cost_function_argument(&self, x: &[Real]) -> QlResult<()> {
            self.record("set");
            self.trials.borrow_mut().push(x.to_vec());
            *self.current.borrow_mut() = x.to_vec();
            Ok(())
        }

        fn evaluate_cost_function(&self) -> QlResult<Array> {
            self.record("evaluate");
            let current = self.current.borrow();
            Ok(current
                .iter()
                .zip(&self.targets)
                .map(|(value, target)| value - target)
                .collect())
        }

        fn set_to_valid(&self, solution: &[Real]) -> QlResult<()> {
            self.record("valid");
            self.solutions.borrow_mut().push(solution.to_vec());
            Ok(())
        }

        fn invalidate(&self) {
            self.record("invalidate");
            self.invalidations.set(self.invalidations.get() + 1);
        }
    }

    /// The maximal runs of consecutive `set` or `evaluate` entries in a mock
    /// log, each with the contributor ids it covered, in order.
    fn phases(log: &[(usize, &'static str)]) -> Vec<(&'static str, Vec<usize>)> {
        let mut phases: Vec<(&'static str, Vec<usize>)> = Vec::new();
        for (id, what) in log {
            let (id, what) = (*id, *what);
            if what != "set" && what != "evaluate" {
                continue;
            }
            match phases.last_mut() {
                Some((kind, ids)) if *kind == what => ids.push(id),
                _ => phases.push((what, vec![id])),
            }
        }
        phases
    }

    /// The stacked solve drives its contributors two-phase, on their own
    /// slices, and pins each with its own slice of the answer
    /// (`globalbootstrap.cpp:50-116`).
    ///
    /// This is the real gate for the joint mechanism. The oracle a multi-curve
    /// port would reach for first - a bootstrapped curve plus a spreaded one -
    /// cannot see any of this: a spreaded curve reads its base live, so a
    /// contributor that ignored the parent entirely and solved alone converges
    /// to the SAME fixed point and reprices just as well. Only mocks that
    /// record what they were handed can tell a joint solve from two separate
    /// ones.
    ///
    /// The three properties, and what each would catch:
    /// - ORDERING (`cpp:70` / `:82`): every set phase covers BOTH contributors
    ///   before any evaluate phase begins. Fusing the two loops into one
    ///   per-contributor pass gives phases of one id each and fails the scan.
    ///   It is load-bearing rather than cosmetic: a coupled contributor's
    ///   evaluate reads a curve another contributor is still writing, and a
    ///   fused loop would meet a live `borrow_mut`.
    /// - SLICING (`cpp:64-71`): the 2-variable mock must always receive two
    ///   coordinates and the 1-variable mock exactly one, over the same number
    ///   of rounds.
    /// - CONVERGENCE AND SWEEP (`cpp:102-115`): each mock is pinned once, in
    ///   registration order, with a slice that reached its own targets. This is
    ///   also the second slicing pin, and the sharper one: an offset off by one
    ///   still hands out the right LENGTHS, but then the two mocks argue over
    ///   one coordinate and neither reaches its target.
    #[test]
    fn the_stacked_solve_drives_its_contributors_in_two_phases_on_their_own_slices() {
        let log: MockLog = shared(RefCell::new(Vec::new()));
        let first = MockContributor::new(0, vec![0.3, -0.7], &log);
        let second = MockContributor::new(1, vec![1.2], &log);
        let parent = shared(MultiCurveBootstrap::new(1.0e-12));
        parent.add(&(Shared::clone(&first) as Shared<dyn MultiCurveBootstrapContributor>));
        parent.add(&(Shared::clone(&second) as Shared<dyn MultiCurveBootstrapContributor>));

        parent.run().expect("the stacked system solves");

        let entries = log.borrow();
        let phases = phases(&entries);
        assert!(!phases.is_empty(), "the stacked solve never evaluated");
        assert_eq!(
            phases.len() % 2,
            0,
            "a set phase was left without its evaluate phase: {phases:?}"
        );
        for (k, (kind, ids)) in phases.iter().enumerate() {
            let expected = if k % 2 == 0 { "set" } else { "evaluate" };
            assert_eq!(
                *kind, expected,
                "phase {k} is a {kind} run where a {expected} run belongs"
            );
            assert_eq!(
                ids,
                &vec![0usize, 1usize],
                "phase {k} did not cover both contributors in registration order"
            );
        }

        let first_trials = first.trials.borrow();
        let second_trials = second.trials.borrow();
        assert_eq!(
            first_trials.len(),
            second_trials.len(),
            "the two contributors were driven over a different number of rounds"
        );
        assert!(
            first_trials.iter().all(|trial| trial.len() == 2),
            "the two-variable contributor was handed a slice of the wrong width"
        );
        assert!(
            second_trials.iter().all(|trial| trial.len() == 1),
            "the one-variable contributor was handed a slice of the wrong width"
        );

        assert_eq!(
            entries
                .iter()
                .filter(|(_, what)| *what == "valid")
                .copied()
                .collect::<Vec<_>>(),
            vec![(0, "valid"), (1, "valid")],
            "the validity sweep did not run once per contributor in registration order"
        );
        for mock in [&first, &second] {
            let solutions = mock.solutions.borrow();
            assert_eq!(solutions.len(), 1, "mock {} was pinned twice", mock.id);
            for (i, (value, target)) in solutions[0].iter().zip(&mock.targets).enumerate() {
                assert!(
                    (value - target).abs() < 1.0e-8,
                    "mock {} coordinate {i} was pinned at {value}, not its target {target}",
                    mock.id
                );
            }
            assert_eq!(
                mock.invalidations.get(),
                0,
                "a successful solve reverted mock {}",
                mock.id
            );
        }
    }

    /// A joint solve that fails part way must not leave the contributors it
    /// already set up marked as calculated.
    ///
    /// Divergence being pinned: `setup_cost_function` marks its curve
    /// calculated FIRST (`globalbootstrap.hpp:324`), so by the time a later
    /// contributor fails, the earlier ones would read as solved curves over a
    /// grid no solve ever finished. C++ leaves exactly that to the caller. The
    /// port reverts them instead, and this is the only arm that can see it.
    ///
    /// The failing contributor is reverted too, not just the ones before it: a
    /// real one marks its curve before its setup can fail, so the count that
    /// matters is how many setups were ENTERED, not how many returned.
    #[test]
    fn a_failed_setup_reverts_every_contributor_the_joint_solve_marked() {
        let log: MockLog = shared(RefCell::new(Vec::new()));
        let first = MockContributor::new(0, vec![0.3, -0.7], &log);
        let second = MockContributor::failing(1, vec![1.2], &log);
        let parent = shared(MultiCurveBootstrap::new(1.0e-12));
        parent.add(&(Shared::clone(&first) as Shared<dyn MultiCurveBootstrapContributor>));
        parent.add(&(Shared::clone(&second) as Shared<dyn MultiCurveBootstrapContributor>));

        assert!(
            parent.run().is_err(),
            "a contributor that cannot set up must fail the joint solve"
        );
        assert_eq!(
            first.invalidations.get(),
            1,
            "the contributor set up before the failure was left marked calculated"
        );
        assert_eq!(
            second.invalidations.get(),
            1,
            "the contributor that failed to set up was left marked calculated"
        );
        let entries = log.borrow();
        assert!(
            phases(&entries).is_empty(),
            "the solve went on to evaluate after a setup failure: {entries:?}"
        );
    }

    /// The recording [`AdditionalBootstrapVariables`] of the arm below: it
    /// delegates to a real [`SimpleQuoteVariables`] and records what the driver
    /// hands it.
    struct RecordingVariables {
        inner: SimpleQuoteVariables,
        valid_data_flags: RefCell<Vec<bool>>,
        trial_lengths: RefCell<Vec<Size>>,
        trial_firsts: RefCell<Vec<Real>>,
    }

    /// Lets the test keep reading the recordings after the boxed trait object
    /// has been handed to the bootstrap.
    impl AdditionalBootstrapVariables for Shared<RecordingVariables> {
        fn initialize(&self, valid_data: bool) -> QlResult<Vec<Real>> {
            self.as_ref().initialize(valid_data)
        }

        fn update(&self, x: &[Real]) -> QlResult<()> {
            self.as_ref().update(x)
        }
    }

    impl AdditionalBootstrapVariables for RecordingVariables {
        fn initialize(&self, valid_data: bool) -> QlResult<Vec<Real>> {
            self.valid_data_flags.borrow_mut().push(valid_data);
            self.inner.initialize(valid_data)
        }

        fn update(&self, x: &[Real]) -> QlResult<()> {
            self.trial_lengths.borrow_mut().push(x.len());
            self.trial_firsts.borrow_mut().push(x[0]);
            self.inner.update(x)
        }
    }

    /// The three additional-variable splices - the appended guess, the
    /// `x[interior..]` argument tail and the solution pin - end to end, plus
    /// what only a recording implementation can see.
    ///
    /// The system is deliberately minimal and SQUARE: a one-deposit strip is
    /// two nodes, so one interior node, and one external quote is one more
    /// variable; the residuals are the deposit's quote error and one penalty
    /// `q - 0.5`. The two halves are independent, so the solved answer is known
    /// in closed form - the deposit reprices and `q` lands on 0.5 - which is
    /// what pins the splices without a dylib. The variable is FLOORED at 0.0
    /// and guessed at 2.0, so it also travels through the `exp`/`ln` transform
    /// pair. The first recorded trial point is what pins that: the solver
    /// evaluates the guess first, so `x[interior]` must arrive as the value
    /// `initialize` returned, `ln(2 - 0)`. A guess appended in QUOTE space
    /// would arrive as 2, and one written before the nodes rather than after
    /// them would arrive as the node coordinate 0. The guess of 2.0 is what
    /// separates those three - a guess of 1.0 maps to 0 in optimizer space,
    /// which is also this strip's node coordinate, and the misplacement arm
    /// goes vacuous (probed both ways). The converged answer alone is blind to
    /// all of it: a guess is a start point, and the solve converges from any of
    /// them.
    ///
    /// The recorded flags are the ONLY way the warm-restart branch of
    /// `initialize` is observable: the converged answer is the same whether the
    /// second solve seeds itself from the previous solution or from the
    /// configured guess. Likewise the recorded lengths are the only direct
    /// evidence of where the argument vector is split.
    ///
    /// HONEST NEGATIVE: the solution-pin `update` is NOT pinned here, and is
    /// not pinnable through any converged value. The optimizer's last trial
    /// point already sits within the stopping accuracy of the solution it
    /// returns, so rewriting the variables from that solution moves them by
    /// less than the tolerance any assert can use (probed - dropping the pin
    /// leaves every arm green). It is carried for consistency with the node
    /// pin, which rewrites the curve from the same vector.
    #[test]
    fn the_additional_variables_receive_the_guess_the_trial_tail_and_the_solution() {
        let calendar = Target::new();
        let today = calendar.adjust(
            Date::new(15, Month::June, 2026),
            BusinessDayConvention::Following,
        );
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let index = Euribor::new(Period::new(3, TimeUnit::Months), Handle::empty(), settings)
            .expect("a 3M tenor is valid");
        let deposit_quote = shared(SimpleQuote::new(0.04557));
        let deposit = DepositRateHelper::new(
            Handle::new(Shared::clone(&deposit_quote) as Shared<dyn Quote>),
            &index,
        ) as Shared<dyn RateHelper>;

        let solved = shared(SimpleQuote::new(None));
        let variables = Shared::new(RecordingVariables {
            inner: SimpleQuoteVariables::new(vec![Shared::clone(&solved)], vec![2.0], vec![0.0])
                .expect("one guess and one bound for one quote"),
            valid_data_flags: RefCell::new(Vec::new()),
            trial_lengths: RefCell::new(Vec::new()),
            trial_firsts: RefCell::new(Vec::new()),
        });
        let penalty_quote = Shared::clone(&solved);
        let curve = PiecewiseYieldCurve::<SimpleZeroYield, Linear, _>::with_bootstrap(
            calendar.advance(
                today,
                2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            ),
            vec![deposit],
            Actual365Fixed::new(),
            Linear,
            GlobalBootstrap::with_grid_independent_penalties(
                Vec::new(),
                None,
                Some(1.0e-12),
                None,
                Vec::new(),
                move || {
                    let value = penalty_quote
                        .value()
                        .expect("the variable holds a trial value");
                    vec![value - 0.5]
                },
            )
            .with_additional_variables(Box::new(Shared::clone(&variables))),
        )
        .expect("a one-deposit strip builds a curve");

        curve.data().expect("the square system solves");

        let value = solved.value().expect("the variable is solved");
        assert!(
            (value - 0.5).abs() < 1.0e-8,
            "the additional variable solved to {value}, not 0.5"
        );
        assert_eq!(
            *variables.valid_data_flags.borrow(),
            vec![false],
            "the first solve is a cold start"
        );

        deposit_quote.set_value(0.05);
        curve.data().expect("the strip re-solves on the same grid");

        assert_eq!(
            *variables.valid_data_flags.borrow(),
            vec![false, true],
            "a re-solve over an unchanged grid must warm-restart"
        );
        let lengths = variables.trial_lengths.borrow();
        assert!(
            !lengths.is_empty(),
            "the trial tail never reached the variables"
        );
        assert!(
            lengths.iter().all(|length| *length == 1),
            "the argument vector was split at the wrong index: {lengths:?}"
        );
        let first_trial = variables.trial_firsts.borrow()[0];
        assert!(
            (first_trial - Real::ln(2.0)).abs() < 1.0e-15,
            "the solver's first trial point is {first_trial}, not the appended guess"
        );
    }
}
