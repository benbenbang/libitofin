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
//!
//! A notification that cannot be delivered immediately - the observer is
//! already mid-`update` when a re-entrant notification reaches it, through any
//! observable - is queued on a thread-local pending list and re-delivered by
//! the outermost notification on the thread once the borrow releases, so no
//! notification is dropped; see [`Observable::notify_observers`].

use std::cell::{Cell, RefCell};

use crate::shared::{Shared, SharedMut, WeakMut, shared, shared_mut};

thread_local! {
    /// Nesting depth of [`Observable::notify_observers`] across all
    /// observables on this thread; the invocation returning to depth zero
    /// drains [`PENDING`].
    static DEPTH: Cell<usize> = const { Cell::new(0) };
    /// Re-entrancy latch so a drained observer re-notifying does not start a
    /// nested drain; the running drain loop picks its deferrals up instead.
    static DRAINING: Cell<bool> = const { Cell::new(false) };
    /// Observers a notification round could not borrow, shared by all
    /// observables so a miss is re-delivered no matter which observable's
    /// round it happened in.
    static PENDING: RefCell<Vec<WeakMut<dyn Observer>>> = const { RefCell::new(Vec::new()) };
}

/// RAII depth counter for [`DEPTH`], panic-safe like `LazyObject`'s guard.
struct DepthGuard;

impl DepthGuard {
    fn enter() -> Self {
        DEPTH.with(|depth| depth.set(depth.get() + 1));
        DepthGuard
    }
}

impl Drop for DepthGuard {
    fn drop(&mut self) {
        DEPTH.with(|depth| depth.set(depth.get() - 1));
    }
}

/// RAII latch for [`DRAINING`]; `try_enter` yields `None` when already set.
struct DrainGuard;

impl DrainGuard {
    fn try_enter() -> Option<Self> {
        if DRAINING.with(Cell::get) {
            return None;
        }
        DRAINING.with(|flag| flag.set(true));
        Some(DrainGuard)
    }
}

impl Drop for DrainGuard {
    fn drop(&mut self) {
        DRAINING.with(|flag| flag.set(false));
    }
}

/// Delivers one notification with the same re-entrancy discipline as
/// [`Observable::notify_observers`]: a busy observer is queued for the
/// outermost round instead of panicking on the live borrow. For observers
/// that forward to another observer directly, outside any observable's
/// registry.
pub(crate) fn deliver(observer: &SharedMut<dyn Observer>) {
    match observer.try_borrow_mut() {
        Ok(mut delivered) => delivered.update(),
        Err(_) => defer(observer),
    }
}

/// Queues an undeliverable observer for the outermost round, once.
fn defer(observer: &SharedMut<dyn Observer>) {
    let weak = SharedMut::downgrade(observer);
    PENDING.with(|pending| {
        let mut pending = pending.borrow_mut();
        if !pending.iter().any(|queued| queued.ptr_eq(&weak)) {
            pending.push(weak);
        }
    });
}

/// Delivers every queued miss; runs only at depth zero and never nests.
///
/// A drained `update` may notify further observables; their fresh misses land
/// in [`PENDING`] and are picked up by the next pass of the loop. An observer
/// still borrowed here is held by code outside any notification; it stays
/// queued for the next outermost notification rather than spinning.
fn drain_pending() {
    let Some(_draining) = DrainGuard::try_enter() else {
        return;
    };
    let mut stalled: Vec<WeakMut<dyn Observer>> = Vec::new();
    loop {
        let batch: Vec<WeakMut<dyn Observer>> =
            PENDING.with(|pending| pending.borrow_mut().drain(..).collect());
        if batch.is_empty() {
            break;
        }
        for weak in batch {
            let Some(observer) = weak.upgrade() else {
                continue;
            };
            match observer.try_borrow_mut() {
                Ok(mut observer) => observer.update(),
                Err(_) => {
                    if !stalled.iter().any(|queued| queued.ptr_eq(&weak)) {
                        stalled.push(weak);
                    }
                }
            }
        }
    }
    if !stalled.is_empty() {
        PENDING.with(|pending| pending.borrow_mut().append(&mut stalled));
    }
}

/// Object that gets notified when an [`Observable`] it registered with changes.
///
/// Mirrors QuantLib's `Observer`. The corresponding `update()` is the single
/// required method; the registry plumbing lives on [`Observable`].
pub trait Observer {
    /// Called by the observables this instance registered with on a change.
    fn update(&mut self);
}

/// Contract for types that embed an [`Observable`] and broadcast through it.
///
/// Mirrors QuantLib's "inherits from `Observable`" preconditions (e.g. the
/// `handle.hpp` requirement on handle pointees): observers register with the
/// embedded observable this accessor exposes.
pub trait AsObservable {
    /// Access to the embedded observable for registering observers.
    fn observable(&self) -> &Observable;
}

/// Observer that forwards every notification to another observable (the
/// C++ `update()` of `Link`, `DeltaVolQuote` and friends).
///
/// The forwarding target embeds this in front of its own [`Observable`]: the
/// forwarder registers with the source and passes each notification on to the
/// target's observers.
pub(crate) struct Forwarder {
    pub(crate) observable: Shared<Observable>,
}

impl Forwarder {
    /// Builds a fresh observable and the forwarder wired to it, the shared
    /// construction step of every forwarding type.
    pub(crate) fn new() -> (Shared<Observable>, SharedMut<Forwarder>) {
        let observable = shared(Observable::new());
        let forwarder = shared_mut(Forwarder {
            observable: Shared::clone(&observable),
        });
        (observable, forwarder)
    }
}

impl Observer for Forwarder {
    fn update(&mut self) {
        self.observable.notify_observers();
    }
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
        Observable::default()
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
        let mut removed = false;
        observers.retain(|w| {
            if w.ptr_eq(&target) {
                removed = true;
                return false;
            }
            w.strong_count() > 0
        });
        removed
    }

    /// Notifies every currently registered, still-live observer.
    ///
    /// The live observers are snapshotted (as strong refs) and the registry
    /// borrow is dropped before any `update` runs, so re-entrant
    /// registration/unregistration during `update` is safe and does not affect
    /// this round of notification. Dropped observers are pruned.
    ///
    /// An observer whose `RefCell` is already mutably borrowed when the round
    /// reaches it - typically its own `update` is on the stack and re-entrantly
    /// triggered this notification, the case QuantLib runs as direct recursion -
    /// cannot take the unsatisfiable second `&mut`. Instead of dropping the
    /// notification, the round queues it on a pending list shared by every
    /// observable on the thread, and the outermost notification re-delivers it
    /// once the borrow has been released. Delivery is thus guaranteed no matter
    /// which observable the re-entrant notification arrived through, and every
    /// observer converges on the final state - the fixed point of QuantLib's
    /// recursion. Updates are idempotent invalidations, so a re-delivered
    /// observer may hear one extra round; and as in C++, a graph whose
    /// observers keep writing back forever does not terminate.
    ///
    /// The one case deferred past the current notification is an observer kept
    /// borrowed by code outside any `update` (a caller holding a borrow across
    /// the notification): it stays queued and is delivered at the end of the
    /// next outermost notification on the thread.
    pub fn notify_observers(&self) {
        let snapshot: Vec<SharedMut<dyn Observer>> = {
            let mut observers = self.observers.borrow_mut();
            observers.retain(|w| w.strong_count() > 0);
            observers.iter().filter_map(WeakMut::upgrade).collect()
        };
        {
            let _depth = DepthGuard::enter();
            for observer in snapshot {
                match observer.try_borrow_mut() {
                    Ok(mut observer) => observer.update(),
                    Err(_) => defer(&observer),
                }
            }
        }
        if DEPTH.with(Cell::get) == 0 {
            drain_pending();
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
    fn unregister_unknown_observer_is_not_confused_by_dead_weaks() {
        // A registered-then-dropped observer leaves a dead weak in the registry;
        // unregistering a never-registered observer must still report `false`
        // rather than mistaking the dead-weak pruning for a real removal.
        let observable = Observable::new();
        let registered = UpdateCounter::new();
        observable.register_observer(&as_observer(&registered));
        {
            let transient = UpdateCounter::new();
            observable.register_observer(&as_observer(&transient));
        }

        let never_registered = UpdateCounter::new();
        assert!(!observable.unregister_observer(&as_observer(&never_registered)));
        // the genuinely registered observer is still notified afterwards
        observable.notify_observers();
        assert_eq!(registered.borrow().counter, 1);
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

    /// Observer that re-enters `notify_observers` from within its own `update`,
    /// as a write-back observer does through the notifying observable.
    struct Renotifier {
        updates: usize,
        observable: Shared<Observable>,
    }

    impl Observer for Renotifier {
        fn update(&mut self) {
            self.updates += 1;
            if self.updates == 1 {
                self.observable.notify_observers();
            }
        }
    }

    /// Listener registered with two observables, mirroring a composite
    /// quote's invalidator: handling observable A's notification triggers
    /// observable B, whose round finds the listener still mid-`update`.
    struct CrossNotifier {
        updates: usize,
        other: Shared<Observable>,
        fired: bool,
    }

    impl Observer for CrossNotifier {
        fn update(&mut self) {
            self.updates += 1;
            if !self.fired {
                self.fired = true;
                self.other.notify_observers();
            }
        }
    }

    #[test]
    fn cross_observable_reentrant_notification_is_redelivered() {
        let a = Shared::new(Observable::new());
        let b = Shared::new(Observable::new());

        let listener = shared_mut(CrossNotifier {
            updates: 0,
            other: b.clone(),
            fired: false,
        });
        a.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        b.register_observer(&(listener.clone() as SharedMut<dyn Observer>));

        let bystander = UpdateCounter::new();
        b.register_observer(&as_observer(&bystander));

        a.notify_observers();

        // b's round ran while the listener was in-flight from a's round: the
        // miss must be re-delivered once a's round unwinds, not dropped
        assert_eq!(listener.borrow().updates, 2);
        assert_eq!(bystander.borrow().counter, 1);
    }

    #[test]
    fn notification_blocked_by_an_outside_borrow_is_delivered_later() {
        let observable = Observable::new();
        let counter = UpdateCounter::new();
        observable.register_observer(&as_observer(&counter));

        {
            let held = counter.borrow();
            observable.notify_observers();
            // the observer cannot be updated while the caller holds it...
            assert_eq!(held.counter, 0);
        }

        // ...so it stays queued and the next outermost notification delivers
        // both its own round and the queued re-delivery (updates are
        // idempotent invalidations)
        observable.notify_observers();
        assert_eq!(counter.borrow().counter, 2);
    }

    #[test]
    fn reentrant_notification_defers_the_in_flight_observer() {
        let observable = Shared::new(Observable::new());
        let plain = UpdateCounter::new();
        observable.register_observer(&as_observer(&plain));

        let renotifier = shared_mut(Renotifier {
            updates: 0,
            observable: observable.clone(),
        });
        observable.register_observer(&(renotifier.clone() as SharedMut<dyn Observer>));

        observable.notify_observers();

        // instead of panicking on a second mutable borrow, the nested round
        // queues the in-flight observer and the outermost round drains the
        // queue, matching the two updates C++ delivers through recursion...
        assert_eq!(renotifier.borrow().updates, 2);
        // ...and the other observer hears the outer and nested rounds, the
        // exact counts of the C++ recursion
        assert_eq!(plain.borrow().counter, 2);
    }
}
