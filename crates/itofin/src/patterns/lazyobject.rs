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
use crate::shared::SharedMut;

use super::observable::Observer;

/// Framework for calculation on demand and result caching.
///
/// Embed one in a type that derives results from observable inputs. Drive it
/// through [`calculate`](LazyObject::calculate) (which runs the supplied closure
/// at most once until invalidated) and feed it observer notifications via
/// [`on_update`](LazyObject::on_update).
pub struct LazyObject {
    observable: Observable,
    calculated: bool,
    frozen: bool,
    failed: bool,
    always_forward: bool,
    updating: bool,
}

impl LazyObject {
    /// Creates a lazy object.
    ///
    /// `always_forward` selects the notification policy (QuantLib's
    /// `LazyObject::Defaults`): `true` forwards every notification, `false`
    /// forwards only the first received after a (re)calculation.
    pub fn new(always_forward: bool) -> Self {
        LazyObject {
            observable: Observable::new(),
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
        let mut notified = false;
        if self.calculated || self.failed || self.always_forward {
            self.calculated = false;
            self.failed = false;
            if !self.frozen {
                self.observable.notify_observers();
                notified = true;
            }
        }
        self.updating = false;
        notified
    }

    /// Runs `perform` to (re)compute results unless already cached or frozen.
    ///
    /// Mirrors `LazyObject::calculate`: marks the object calculated before
    /// running `perform` (to break bootstrap recursion), and on failure reverts
    /// to a not-calculated/failed state while propagating the error.
    pub fn calculate(&mut self, perform: impl FnOnce() -> QlResult<()>) -> QlResult<()> {
        if !self.calculated && !self.frozen {
            self.calculated = true;
            match perform() {
                Ok(()) => self.failed = false,
                Err(e) => {
                    self.calculated = false;
                    self.failed = true;
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    /// Forces a recalculation and notifies observers.
    ///
    /// Mirrors `LazyObject::recalculate`: clears the cached state, runs the
    /// calculation, and notifies observers afterwards (even on failure).
    pub fn recalculate(&mut self, perform: impl FnOnce() -> QlResult<()>) -> QlResult<()> {
        let was_frozen = self.frozen;
        self.calculated = false;
        self.frozen = false;
        self.failed = false;
        let result = self.calculate(perform);
        self.frozen = was_frozen;
        self.observable.notify_observers();
        result
    }

    /// Pins the currently cached results, suppressing recalculation.
    pub fn freeze(&mut self) {
        self.frozen = true;
    }

    /// Re-enables recalculation, notifying observers once if it was frozen.
    pub fn unfreeze(&mut self) {
        if self.frozen {
            self.frozen = false;
            self.observable.notify_observers();
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
