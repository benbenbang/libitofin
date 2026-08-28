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

use crate::math::optimization::endcriteria::EndCriteria;
use crate::types::Real;

/// The global bootstrap (`GlobalBootstrap`, single-curve core).
///
/// Carries the stopping-accuracy override, the `EndCriteria` override and the
/// per-instrument residual weights; defaults mirror the C++ constructor
/// (`accuracy = Null`, `endCriteria = nullptr`, `instrumentWeights = {}`,
/// `globalbootstrap.hpp:112-115`), with everything resolved from the curve at
/// calculation time.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
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
