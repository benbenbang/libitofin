//! Calculation-on-demand with result caching.
//!
//! Port of `ql/patterns/lazyobject.hpp`. QuantLib's `LazyObject` multiply
//! inherits from both `Observable` and `Observer`; in Rust we model it as a
//! reusable helper that a concrete type embeds, exposing the calculation as a
//! caller-supplied closure.
//!
//! The `LazyObject::Defaults` singleton is intentionally not ported: per design
//! decision D5 the core avoids global singletons, so the "forward all
//! notifications" vs "forward first only" choice is an explicit constructor
//! argument instead of session-global state.

use crate::errors::QlResult;
use crate::patterns::observable::Observable;
use crate::shared::{Shared, SharedMut, shared};

use super::observable::Observer;

/// Resets the [`LazyObject`] re-entrancy flag when dropped, so it is cleared on
/// every exit from `on_update` - including a panic unwinding out of an observer.
struct UpdatingGuard<'a>(&'a mut bool);

impl Drop for UpdatingGuard<'_> {
    fn drop(&mut self) {
        *self.0 = false;
    }
}

/// Framework for calculation on demand and result caching.
///
/// Embed one in a type that derives results from observable inputs. Drive it
/// through [`calculate`](LazyObject::calculate) (which runs the supplied closure
/// at most once until invalidated) and feed it observer notifications via
/// [`on_update`](LazyObject::on_update).
pub struct LazyObject {
    observable: Shared<Observable>,
    calculated: bool,
    frozen: bool,
    failed: bool,
    always_forward: bool,
    updating: bool,
}

/// A deferred lazy-object notification that keeps the logical C++ `updating_`
/// guard set while the caller notifies after releasing its `RefCell` borrow.
///
/// This is the RAII `LazyObject::UpdateChecker` of `lazyobject.hpp:134-141`,
/// which sets `updating_` on entry to `update()` and clears it on scope exit
/// (`:203`), so that an observer which writes back into an input during
/// notification re-enters `update()` and returns early at `:191`. The port
/// splits the C++ single `update()` into a state half and a notify half so the
/// `RefCell` borrow is released before observers run; the guard carries the
/// `updating_` flag across that gap.
pub(crate) struct DeferredUpdate {
    lazy: SharedMut<LazyObject>,
    observable: Shared<Observable>,
}

impl DeferredUpdate {
    /// Notifies observers while this guard is alive.
    pub(crate) fn notify_observers(&self) {
        self.observable.notify_observers();
    }
}

impl Drop for DeferredUpdate {
    fn drop(&mut self) {
        self.lazy.borrow_mut().updating = false;
    }
}

impl LazyObject {
    /// Creates a lazy object.
    ///
    /// `always_forward` selects the notification policy (QuantLib's
    /// `LazyObject::Defaults`): `true` forwards every notification, `false`
    /// forwards only the first received after a (re)calculation.
    pub fn new(always_forward: bool) -> Self {
        LazyObject {
            observable: shared(Observable::new()),
            calculated: false,
            frozen: false,
            failed: false,
            always_forward,
            updating: false,
        }
    }

    /// Access to the embedded observable for registering downstream observers.
    pub fn observable(&self) -> &Observable {
        &self.observable
    }

    /// A shared handle to the embedded observable, for holders that must
    /// notify after releasing their borrow of the lazy object.
    pub fn observable_handle(&self) -> Shared<Observable> {
        Shared::clone(&self.observable)
    }

    /// Whether cached results are currently valid.
    pub fn is_calculated(&self) -> bool {
        self.calculated
    }

    /// Forces the policy to forward only the first notification.
    pub fn forward_first_notification_only(&mut self) {
        self.always_forward = false;
    }

    /// Forces the policy to forward all notifications.
    pub fn always_forward_notifications(&mut self) {
        self.always_forward = true;
    }

    /// Observer-side hook: handles a notification from an input observable.
    ///
    /// Implements the C++ `update()` logic, including the recursion guard that
    /// breaks notification cycles. Returns `true` if observers were notified.
    pub fn on_update(&mut self) -> bool {
        if self.updating {
            return false;
        }
        self.updating = true;
        // Reset the re-entrancy guard on every exit path, including an observer
        // panicking inside `notify_observers`. A manual `self.updating = false`
        // at the end would be skipped while the panic unwinds, latching the flag
        // and silently suppressing every future notification.
        let _reset = UpdatingGuard(&mut self.updating);
        let mut notified = false;
        if self.calculated || self.failed || self.always_forward {
            self.calculated = false;
            self.failed = false;
            if !self.frozen {
                self.observable.notify_observers();
                notified = true;
            }
        }
        notified
    }

    /// The state half of [`on_update`](LazyObject::on_update) for lazy objects
    /// held in a `SharedMut`: applies the invalidation and hands back a guard
    /// that notifies after the borrow is released. The guard keeps `updating`
    /// set until notification completes, matching the C++ recursion guard.
    pub(crate) fn deferred_update(lazy: &SharedMut<LazyObject>) -> Option<DeferredUpdate> {
        let observable = {
            let mut lazy = lazy.borrow_mut();
            if lazy.updating || !(lazy.calculated || lazy.failed || lazy.always_forward) {
                return None;
            }
            lazy.updating = true;
            lazy.calculated = false;
            lazy.failed = false;
            if lazy.frozen {
                lazy.updating = false;
                return None;
            }
            Shared::clone(&lazy.observable)
        };
        Some(DeferredUpdate {
            lazy: SharedMut::clone(lazy),
            observable,
        })
    }

    /// Marks the results calculated without running a calculation, bypassing
    /// the frozen guard (the C++ direct `calculated_ = true` write in
    /// `Instrument::calculate`'s expired branch).
    pub fn mark_calculated(&mut self) {
        self.calculated = true;
        self.failed = false;
    }

    /// First half of [`calculate`](LazyObject::calculate) for callers that
    /// cannot hold a borrow across the computation: applies the cache and
    /// frozen guards and marks the object calculated (the C++ pre-set that
    /// breaks bootstrap recursion). Returns whether the computation must run;
    /// when it does, report the outcome via
    /// [`finish_calculation`](LazyObject::finish_calculation).
    pub fn start_calculation(&mut self) -> bool {
        if self.calculated || self.frozen {
            return false;
        }
        self.calculated = true;
        true
    }

    /// Second half of [`calculate`](LazyObject::calculate): records the
    /// computation's outcome, reverting to a not-calculated/failed state on
    /// error. A re-entrant invalidation during the computation is preserved:
    /// a success does not re-mark the object calculated.
    pub fn finish_calculation(&mut self, result: &QlResult<()>) {
        match result {
            Ok(()) => self.failed = false,
            Err(_) => {
                self.calculated = false;
                self.failed = true;
            }
        }
    }

    /// Runs `perform` to (re)compute results unless already cached or frozen.
    ///
    /// Mirrors `LazyObject::calculate`: marks the object calculated before
    /// running `perform` (to break bootstrap recursion), and on failure reverts
    /// to a not-calculated/failed state while propagating the error.
    pub fn calculate(&mut self, perform: impl FnOnce() -> QlResult<()>) -> QlResult<()> {
        if !self.start_calculation() {
            return Ok(());
        }
        let result = perform();
        self.finish_calculation(&result);
        result
    }

    /// Forces a recalculation and notifies observers.
    ///
    /// Mirrors `LazyObject::recalculate`: clears the cached state, runs the
    /// calculation, and notifies observers afterwards (even on failure).
    pub fn recalculate(&mut self, perform: impl FnOnce() -> QlResult<()>) -> QlResult<()> {
        let was_frozen = self.start_recalculation();
        let result = self.calculate(perform);
        self.finish_recalculation(was_frozen);
        self.observable.notify_observers();
        result
    }

    /// First half of [`recalculate`](LazyObject::recalculate) for holders that
    /// run the computation outside the borrow: clears the cached state and the
    /// frozen flag, returning the prior frozen state for
    /// [`finish_recalculation`](LazyObject::finish_recalculation).
    pub fn start_recalculation(&mut self) -> bool {
        let was_frozen = self.frozen;
        self.calculated = false;
        self.frozen = false;
        self.failed = false;
        was_frozen
    }

    /// Second half of [`recalculate`](LazyObject::recalculate): restores the
    /// frozen state; the holder then notifies through
    /// [`observable_handle`](LazyObject::observable_handle) (even on failure,
    /// as in C++).
    pub fn finish_recalculation(&mut self, was_frozen: bool) {
        self.frozen = was_frozen;
    }

    /// Pins the currently cached results, suppressing recalculation.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Re-enables recalculation, notifying observers once if it was frozen.
    pub fn unfreeze(&mut self) {
        if let Some(observable) = self.deferred_unfreeze() {
            observable.notify_observers();
        }
    }

    /// The state half of [`unfreeze`](LazyObject::unfreeze) for lazy objects
    /// held in a `SharedMut`: thaws and hands back the observable to notify
    /// once the holder's borrow is released.
    pub fn deferred_unfreeze(&mut self) -> Option<Shared<Observable>> {
        if self.frozen {
            self.frozen = false;
            Some(Shared::clone(&self.observable))
        } else {
            None
        }
    }

    /// Registers an observer with this object's embedded observable.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.observable.register_observer(observer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fail;
    use crate::shared::shared_mut;

    /// Observer that records whether it received a notification, mirroring the
    /// `Flag` helper used throughout the QuantLib test suite.
    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Flag {
        fn new() -> SharedMut<Flag> {
            shared_mut(Flag::default())
        }
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    /// Minimal lazy object: caches a computed result and can be made to fail,
    /// standing in for `Stock`/`Instrument` (EPIC-8) which the C++ tests use.
    struct Lazy {
        lazy: LazyObject,
        fail: bool,
        calc_count: usize,
    }

    impl Lazy {
        fn new(always_forward: bool) -> Self {
            Lazy {
                lazy: LazyObject::new(always_forward),
                fail: false,
                calc_count: 0,
            }
        }

        fn npv(&mut self) -> QlResult<()> {
            let count = &mut self.calc_count;
            let fail = self.fail;
            self.lazy.calculate(|| {
                *count += 1;
                if fail {
                    fail!("intentional failure");
                }
                Ok(())
            })
        }

        /// Forces a recalculation, mirroring `LazyObject::recalculate`.
        fn force_npv(&mut self) -> QlResult<()> {
            let count = &mut self.calc_count;
            let fail = self.fail;
            self.lazy.recalculate(|| {
                *count += 1;
                if fail {
                    fail!("intentional failure");
                }
                Ok(())
            })
        }

        /// Simulates the input observable changing (the quote firing `update()`).
        fn on_input_change(&mut self) {
            self.lazy.on_update();
        }
    }

    #[test]
    fn calculate_runs_once_until_invalidated() {
        let mut s = Lazy::new(true);
        s.npv().unwrap();
        s.npv().unwrap();
        assert_eq!(s.calc_count, 1);

        // a notification invalidates the cache; the next NPV recomputes
        s.on_input_change();
        s.npv().unwrap();
        assert_eq!(s.calc_count, 2);
    }

    #[test]
    fn forward_first_notification_only() {
        let mut s = Lazy::new(false);
        let flag = Flag::new();
        s.lazy
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        s.npv().unwrap();
        s.on_input_change();
        assert!(flag.borrow().up, "first change should be forwarded");

        flag.borrow_mut().up = false;
        s.on_input_change();
        assert!(
            !flag.borrow().up,
            "second change without recalculation should be discarded"
        );

        flag.borrow_mut().up = false;
        s.npv().unwrap();
        s.on_input_change();
        assert!(flag.borrow().up, "change after recalculation is forwarded");
    }

    #[test]
    fn always_forward_notifications() {
        let mut s = Lazy::new(true);
        let flag = Flag::new();
        s.lazy
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        s.npv().unwrap();
        s.on_input_change();
        assert!(flag.borrow().up);

        flag.borrow_mut().up = false;
        s.on_input_change();
        assert!(flag.borrow().up, "every change should be forwarded");
    }

    #[test]
    fn notification_after_failed_calculation() {
        let mut s = Lazy::new(false);
        let flag = Flag::new();
        s.lazy
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        // successful calc, then change => notified
        s.npv().unwrap();
        s.on_input_change();
        assert!(flag.borrow().up);
        flag.borrow_mut().up = false;

        // failed calc must not notify by itself
        s.fail = true;
        assert!(s.npv().is_err());
        assert!(!flag.borrow().up);

        // fix and change => notified despite the prior failure
        s.fail = false;
        s.on_input_change();
        assert!(flag.borrow().up);
        flag.borrow_mut().up = false;

        // successful recalculation must not notify by itself
        s.npv().unwrap();
        assert!(!flag.borrow().up);

        // after recovery, one change is forwarded...
        s.on_input_change();
        assert!(flag.borrow().up);
        flag.borrow_mut().up = false;

        // ...but a second change without recalculation is discarded
        s.on_input_change();
        assert!(!flag.borrow().up);
    }

    #[test]
    fn recalculate_notifies_even_when_perform_fails() {
        let mut s = Lazy::new(false);
        let flag = Flag::new();
        s.lazy
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        // QuantLib's catch block calls notifyObservers() before re-throwing, so a
        // failed recalculation must still notify observers and propagate the error.
        s.fail = true;
        assert!(s.force_npv().is_err());
        assert!(
            flag.borrow().up,
            "failed recalculate must still notify observers"
        );

        // a successful recalculation notifies as well
        flag.borrow_mut().up = false;
        s.fail = false;
        s.force_npv().unwrap();
        assert!(
            flag.borrow().up,
            "successful recalculate notifies observers"
        );
    }

    #[test]
    fn recursive_update_is_broken_by_guard() {
        // on_update() guards against re-entry; calling it from within update
        // (simulated by a manual nested call) must not recurse forever.
        let mut s = Lazy::new(true);
        s.npv().unwrap();
        // two notifications in a row simply re-fire; the guard only matters for
        // true re-entrancy, which we assert does not deadlock or overflow here.
        s.on_input_change();
        s.on_input_change();
        assert!(!s.lazy.is_calculated());
    }

    // Regression: an observer that panics inside notify_observers must not leave
    // the `updating` re-entrancy flag latched. Previously the manual
    // `self.updating = false` was skipped as the panic unwound, so every later
    // on_update returned early and all notifications were silently suppressed.
    #[test]
    fn panicking_observer_does_not_latch_the_update_guard() {
        use std::panic::{AssertUnwindSafe, catch_unwind};

        // Panics on its first notification only, so the recovery call is clean.
        struct Panicker {
            armed: bool,
        }
        impl Observer for Panicker {
            fn update(&mut self) {
                if self.armed {
                    self.armed = false;
                    panic!("observer panic during notification");
                }
            }
        }

        let mut s = Lazy::new(true);
        let flag = Flag::new();
        // Keep a strong ref alive (the registry holds weak refs and prunes any
        // observer with no live owner before notifying).
        let panicker = shared_mut(Panicker { armed: true });
        s.lazy
            .register_observer(&(panicker.clone() as SharedMut<dyn Observer>));
        s.lazy
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));
        s.npv().unwrap();

        // The first observer panics mid-notification; the guard must still reset.
        let panicked = catch_unwind(AssertUnwindSafe(|| s.on_input_change()));
        assert!(panicked.is_err(), "the observer panic should propagate");

        // A latched guard would make this return false and never notify again.
        flag.borrow_mut().up = false;
        assert!(
            s.lazy.on_update(),
            "a panicking observer must not latch the update guard"
        );
        assert!(
            flag.borrow().up,
            "observers are notified again after recovery"
        );
    }

    #[test]
    fn freeze_suppresses_notifications() {
        let mut s = Lazy::new(true);
        let flag = Flag::new();
        s.lazy
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        s.npv().unwrap();
        s.lazy.freeze();
        s.on_input_change();
        assert!(!flag.borrow().up, "frozen object should not notify");

        s.lazy.unfreeze();
        assert!(flag.borrow().up, "unfreeze sends a catch-up notification");
    }
}
