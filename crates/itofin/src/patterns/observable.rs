//! Observer/observable pattern.
//!
//! Port of the single-threaded branch of `ql/patterns/observable.{hpp,cpp}`
//! (design decision D1). The thread-safety machinery (`ObservableSettings`,
//! deferred updates, recursive mutexes, the `Proxy` indirection) is skipped
//! per the Rc-first port; only the observer registry and notification are kept.
//!
//! Observers are held by [`WeakMut`] back-references so the observer ⇄ observable
//! graph cannot form an `Rc` cycle. [`Observable::notify_observers`] snapshots the
//! live observers and drops every borrow before calling `update`, so an observer
//! may freely register, unregister or mutate the graph while being notified.

use std::cell::RefCell;

use crate::shared::{SharedMut, WeakMut};

/// Object that gets notified when an [`Observable`] it registered with changes.
///
/// Mirrors QuantLib's `Observer`. The corresponding `update()` is the single
/// required method; the registry plumbing lives on [`Observable`].
pub trait Observer {
    /// Called by the observables this instance registered with on a change.
    fn update(&mut self);
}

/// Object that notifies its changes to a set of observers.
///
/// Mirrors QuantLib's `Observable`. Embed one in any type that needs to notify;
/// register observers with [`register_observer`](Observable::register_observer)
/// and broadcast with [`notify_observers`](Observable::notify_observers).
///
/// The registry lives behind a `RefCell` so notification takes `&self` and never
/// holds a borrow across `update`: an observer may freely register, unregister
/// or otherwise touch this observable while being notified.
#[derive(Default)]
pub struct Observable {
    observers: RefCell<Vec<WeakMut<dyn Observer>>>,
}

impl Observable {
    /// Creates an observable with no registered observers.
    pub fn new() -> Self {
        Observable {
            observers: RefCell::new(Vec::new()),
        }
    }

    /// Registers an observer, returning `true` if it was newly added.
    ///
    /// Registration is idempotent by pointer identity, mirroring the
    /// `std::set<Observer*>` semantics of the C++ implementation.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        let weak = SharedMut::downgrade(observer);
        let mut observers = self.observers.borrow_mut();
        observers.retain(|w| w.strong_count() > 0);
        if observers.iter().any(|w| w.ptr_eq(&weak)) {
            return false;
        }
        observers.push(weak);
        true
    }

    /// Unregisters an observer, returning `true` if it had been registered.
    pub fn unregister_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        let target = SharedMut::downgrade(observer);
        let mut observers = self.observers.borrow_mut();
        let before = observers.len();
        observers.retain(|w| !w.ptr_eq(&target) && w.strong_count() > 0);
        before != observers.len()
    }

    /// Notifies every currently registered, still-live observer.
    ///
    /// The live observers are snapshotted (as strong refs) and the registry
    /// borrow is dropped before any `update` runs, so re-entrant
    /// registration/unregistration during `update` is safe and does not affect
    /// this round of notification. Dropped observers are pruned.
    pub fn notify_observers(&self) {
        let snapshot: Vec<SharedMut<dyn Observer>> = {
            let mut observers = self.observers.borrow_mut();
            observers.retain(|w| w.strong_count() > 0);
            observers.iter().filter_map(WeakMut::upgrade).collect()
        };
        for observer in snapshot {
            observer.borrow_mut().update();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Shared, SharedMut, shared_mut};

    /// Mirrors the C++ test `UpdateCounter`: counts received notifications.
    struct UpdateCounter {
        counter: usize,
    }

    impl UpdateCounter {
        fn new() -> SharedMut<UpdateCounter> {
            shared_mut(UpdateCounter { counter: 0 })
        }
    }

    impl Observer for UpdateCounter {
        fn update(&mut self) {
            self.counter += 1;
        }
    }

    fn as_observer(obs: &SharedMut<UpdateCounter>) -> SharedMut<dyn Observer> {
        obs.clone()
    }

    #[test]
    fn notify_increments_registered_observers() {
        let observable = Observable::new();
        let counter = UpdateCounter::new();
        assert!(observable.register_observer(&as_observer(&counter)));
        assert_eq!(counter.borrow().counter, 0);

        observable.notify_observers();
        assert_eq!(counter.borrow().counter, 1);

        observable.notify_observers();
        assert_eq!(counter.borrow().counter, 2);
    }

    #[test]
    fn registration_is_idempotent() {
        let observable = Observable::new();
        let counter = UpdateCounter::new();
        assert!(observable.register_observer(&as_observer(&counter)));
        assert!(!observable.register_observer(&as_observer(&counter)));

        observable.notify_observers();
        assert_eq!(counter.borrow().counter, 1);
    }

    #[test]
    fn unregister_stops_notifications() {
        let observable = Observable::new();
        let counter = UpdateCounter::new();
        observable.register_observer(&as_observer(&counter));

        assert!(observable.unregister_observer(&as_observer(&counter)));
        observable.notify_observers();
        assert_eq!(counter.borrow().counter, 0);

        // unregistering again reports no change
        assert!(!observable.unregister_observer(&as_observer(&counter)));
    }

    #[test]
    fn unregister_on_empty_is_harmless() {
        let observable = Observable::new();
        let counter = UpdateCounter::new();
        assert!(!observable.unregister_observer(&as_observer(&counter)));
    }

    #[test]
    fn dropped_observers_are_pruned() {
        let observable = Observable::new();
        let survivor = UpdateCounter::new();
        observable.register_observer(&as_observer(&survivor));
        {
            let transient = UpdateCounter::new();
            observable.register_observer(&as_observer(&transient));
        }
        // the transient observer was dropped; notifying must not panic and the
        // survivor still gets its update
        observable.notify_observers();
        assert_eq!(survivor.borrow().counter, 1);
    }

    /// Mirrors `testAddAndDeleteObserverDuringNotifyObservers`: an observer that
    /// registers more observers and drops some during its own `update()` must
    /// not break notification of the initially-registered set.
    struct ReentrantObserver {
        updates: usize,
        observable: Shared<Observable>,
        spawned: SharedMut<Vec<SharedMut<UpdateCounter>>>,
        spawn_count: usize,
    }

    impl Observer for ReentrantObserver {
        fn update(&mut self) {
            self.updates += 1;
            for _ in 0..self.spawn_count {
                let extra = UpdateCounter::new();
                self.observable
                    .register_observer(&(extra.clone() as SharedMut<dyn Observer>));
                self.spawned.borrow_mut().push(extra);
            }
        }
    }

    #[test]
    fn add_observers_during_notify_does_not_miss_initial_observers() {
        // The observable is shared (not wrapped in a RefCell) precisely because
        // notification now takes `&self`; an observer can re-enter it directly.
        let observable = Shared::new(Observable::new());
        let spawned: SharedMut<Vec<SharedMut<UpdateCounter>>> = shared_mut(Vec::new());

        let plain = UpdateCounter::new();
        observable.register_observer(&as_observer(&plain));

        let reentrant = shared_mut(ReentrantObserver {
            updates: 0,
            observable: observable.clone(),
            spawned: spawned.clone(),
            spawn_count: 10,
        });
        observable.register_observer(&(reentrant.clone() as SharedMut<dyn Observer>));

        observable.notify_observers();

        // both initially-registered observers were updated exactly once...
        assert_eq!(plain.borrow().counter, 1);
        assert_eq!(reentrant.borrow().updates, 1);
        // ...and the observers added mid-notification exist but were not part of
        // this notification round (snapshot-before-notify semantics).
        assert_eq!(spawned.borrow().len(), 10);
        assert!(spawned.borrow().iter().all(|o| o.borrow().counter == 0));
    }
}
