//! The user-facing multi-curve assembler (`MultiCurve`,
//! `ql/termstructures/multicurve.{hpp,cpp}`): it owns a
//! [`MultiCurveBootstrap`] and the member curves that form a dependency cycle,
//! links each member's internal handle non-owningly, hands back an external
//! handle, and fans an external change out to every member.
//!
//! ## Divergence from the C++ external handle (`multicurve.cpp:60-62`)
//!
//! C++ builds the external handle from an aliasing
//! `shared_ptr(shared_from_this(), curve.get())`, so holding any external
//! handle keeps the whole `MultiCurve` and every member alive. Rust has no
//! `Rc` aliasing constructor, and both substitutes are wrong: a per-curve
//! newtype re-implementing [`YieldTermStructure`] loses virtual dispatch, and
//! an `Rc<MultiCurve>` stored on a curve closes a reference cycle. So the
//! external handle owns only its own curve; the `MultiCurve` instance's
//! lifetime is the caller's, kept as a local that outlives the curves (the
//! intended usage). Dropping the `MultiCurve` while holding only an external
//! handle drops the co-contributor curves, and the next re-solve upgrades a
//! dropped contributor's `Weak` and returns the honest D4 error
//! [`MultiCurveBootstrap::run`] raises, never a silent single-curve fallback.

use std::cell::RefCell;
use std::rc::Weak;

use crate::errors::QlResult;
use crate::handle::{Handle, RelinkableHandle};
use crate::math::optimization::endcriteria::EndCriteria;
use crate::patterns::observable::Observer;
use crate::require;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::globalbootstrap::{MultiCurveBootstrap, MultiCurveBootstrapContributor};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::Real;

/// The observer half of a [`MultiCurve`] (`MultiCurve` is itself an `Observer`
/// in C++; this port composes the observer as a separate struct, the D1
/// pattern the curves use).
///
/// It holds a `Weak` back-reference so the member curves that register it never
/// keep the `MultiCurve` alive, mirroring the divergence documented on the
/// module: the wrapper's lifetime stays the caller's.
struct MultiCurveUpdater {
    multicurve: Weak<MultiCurve>,
}

impl Observer for MultiCurveUpdater {
    fn update(&mut self) {
        if let Some(multicurve) = self.multicurve.upgrade() {
            multicurve.update();
        }
    }

    /// A re-entrant notification that finds this updater already running is a
    /// member re-broadcasting through the notification the fan-out just
    /// triggered; replaying it would only churn the same invalidations, so it
    /// is dropped rather than deferred (the members' own `updating_` guard
    /// already reaches the fixed point).
    fn defer_reentrant_update(&self) -> bool {
        false
    }
}

/// Assembles a set of curves that form a dependency cycle
/// (`MultiCurve`, `multicurve.hpp:79-104`).
///
/// It owns the [`MultiCurveBootstrap`] that joins the bootstrapped members into
/// one solve and owns the member curves themselves; the internal handles the
/// caller links their helpers through are pointed at the members non-owningly,
/// so no reference cycle forms.
pub struct MultiCurve {
    bootstrap: Shared<MultiCurveBootstrap>,
    curves: RefCell<Vec<Shared<dyn YieldTermStructure>>>,
    updater: SharedMut<MultiCurveUpdater>,
}

impl MultiCurve {
    /// The accuracy constructor (`multicurve.cpp:24-25`): the parent builds its
    /// optimizer and stopping criteria from the one number.
    pub fn new(accuracy: Real) -> Shared<MultiCurve> {
        Self::assemble(MultiCurveBootstrap::new(accuracy))
    }

    /// The override constructor (`multicurve.cpp:27-29`) minus its dropped
    /// optimizer argument: an explicit [`EndCriteria`], or `None` for the
    /// default the parent builds in [`MultiCurveBootstrap::run`].
    pub fn with_end_criteria(end_criteria: Option<EndCriteria>) -> Shared<MultiCurve> {
        Self::assemble(MultiCurveBootstrap::with_end_criteria(end_criteria))
    }

    fn assemble(bootstrap: MultiCurveBootstrap) -> Shared<MultiCurve> {
        let bootstrap = shared(bootstrap);
        Shared::new_cyclic(|weak: &Weak<MultiCurve>| MultiCurve {
            bootstrap,
            curves: RefCell::new(Vec::new()),
            updater: shared_mut(MultiCurveUpdater {
                multicurve: weak.clone(),
            }),
        })
    }

    /// Adds a bootstrapped member (`addBootstrappedCurve`,
    /// `multicurve.cpp:31-42`): the curve joins the parent's joint solve, so on
    /// the next query its `calculate` routes to [`MultiCurveBootstrap::run`]
    /// rather than solving alone.
    ///
    /// The C++ `dynamic_pointer_cast` to `MultiCurveBootstrapProvider` is
    /// replaced by the generic bound `C: MultiCurveBootstrapContributor`: the
    /// caller names the contributing type at the call site, so the cast is a
    /// compile-time coercion and the provider indirection is not needed.
    pub fn add_bootstrapped_curve<C>(
        self: &Shared<Self>,
        internal: &RelinkableHandle<dyn YieldTermStructure>,
        curve: Shared<C>,
    ) -> QlResult<Handle<dyn YieldTermStructure>>
    where
        C: MultiCurveBootstrapContributor + YieldTermStructure + 'static,
    {
        require!(
            internal.handle().is_empty(),
            "internal handle must be empty; was the curve added already?"
        );
        self.bootstrap
            .add(&(Shared::clone(&curve) as Shared<dyn MultiCurveBootstrapContributor>));
        self.add_curve(internal, curve as Shared<dyn YieldTermStructure>)
    }

    /// Adds a non-bootstrapped member (`addNonBootstrappedCurve`,
    /// `multicurve.cpp:44-51`): the curve does not join the solve but is
    /// registered as an observer of the parent, so the stacked solve's
    /// mid-iteration notify reaches its downstream cache.
    pub fn add_non_bootstrapped_curve(
        self: &Shared<Self>,
        internal: &RelinkableHandle<dyn YieldTermStructure>,
        curve: Shared<dyn YieldTermStructure>,
    ) -> QlResult<Handle<dyn YieldTermStructure>> {
        require!(
            internal.handle().is_empty(),
            "internal handle must be empty; was the curve added already?"
        );
        self.bootstrap.add_observer(&curve.updater());
        self.add_curve(internal, curve)
    }

    /// The shared tail of both adders (`addCurve`, `multicurve.cpp:53-71`):
    /// points the internal handle at the curve non-owningly, registers this
    /// wrapper as an observer of the curve, takes ownership of the curve, and
    /// returns an external handle.
    ///
    /// The internal handle is linked weakly (`linkTo(..., null_deleter, false)`,
    /// `multicurve.cpp:56-57`): `curves` is the only strong owner, so the
    /// handle can neither keep the curve alive nor form the notification cycle
    /// the internal handles exist to avoid. The external handle owns only the
    /// curve, per the divergence documented on the module.
    fn add_curve(
        self: &Shared<Self>,
        internal: &RelinkableHandle<dyn YieldTermStructure>,
        curve: Shared<dyn YieldTermStructure>,
    ) -> QlResult<Handle<dyn YieldTermStructure>> {
        internal.link_to_weak(Shared::downgrade(&curve));
        let observer = SharedMut::clone(&self.updater) as SharedMut<dyn Observer>;
        curve.observable().register_observer(&observer);
        self.curves.borrow_mut().push(Shared::clone(&curve));
        Ok(Handle::new(curve))
    }

    /// Fans an external change out to every member (`MultiCurve::update`,
    /// `multicurve.cpp:73-76`).
    ///
    /// Divergence from the C++ `for (c : curves_) c->update();`: a member's own
    /// updater may already be on the stack (its quote changed, its updater
    /// notified this wrapper, and the fan-out now reaches back to it), so a
    /// plain `borrow_mut` would panic on the live borrow where C++ simply
    /// re-enters and relies on the callee's `updating_` guard. This mirrors
    /// [`Observable::notify_observers`] instead: it snapshots the members,
    /// releases the borrow, and skips any member whose updater is already
    /// borrowed - a member mid-update is already invalidating, so skipping it
    /// reaches the same fixed point.
    fn update(&self) {
        let members = self.curves.borrow().clone();
        for member in members {
            let updater = member.updater();
            if let Ok(mut observer) = updater.try_borrow_mut() {
                observer.update();
            }
        }
    }
}
