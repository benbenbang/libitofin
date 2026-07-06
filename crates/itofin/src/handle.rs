//! Relinkable shared handle to an observable.
//!
//! Port of `ql/handle.hpp` (design decision D2). All copies of a [`Handle`]
//! share one inner `Link`; relinking through a [`RelinkableHandle`] swaps the
//! pointee for every copy and notifies the link's observers.
//!
//! QuantLib's `Link` is an *observer of its pointee* as well as an observable:
//! changes inside the pointee propagate to observers of the handle, and
//! relinking moves that registration to the new pointee. The port mirrors the
//! `handle.hpp` precondition "class T must inherit from Observable" with the
//! [`AsObservable`] bound on handle pointees and forwards every pointee
//! notification to the link's own observers.

use crate::errors::QlResult;
use crate::patterns::observable::{Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};

/// Pointee contract for [`Handle`]: access to the embedded [`Observable`]
/// through which the pointee broadcasts its changes.
///
/// Mirrors the `handle.hpp` precondition that a handle target "must inherit
/// from `Observable`". The handle's link registers with this observable, so a
/// pointee change reaches every observer of the handle.
pub trait AsObservable {
    /// Access to the embedded observable for registering observers.
    fn observable(&self) -> &Observable;
}

/// Observer half of the link (QuantLib's `Link::update`): forwards every
/// notification of the current pointee to the link's own observers.
struct Forwarder {
    observable: Shared<Observable>,
}

impl Observer for Forwarder {
    fn update(&mut self) {
        self.observable.notify_observers();
    }
}

/// Inner shared cell of a [`Handle`]: the current pointee plus the link's own
/// observable, through which relinks and pointee changes are broadcast.
pub struct Link<T: ?Sized> {
    current: Option<Shared<T>>,
    observable: Shared<Observable>,
    forwarder: SharedMut<Forwarder>,
}

impl<T: ?Sized> Link<T> {
    fn is_empty(&self) -> bool {
        self.current.is_none()
    }
}

impl<T: AsObservable + ?Sized> Link<T> {
    fn new(pointee: Option<Shared<T>>) -> Self {
        let observable = Shared::new(Observable::new());
        let forwarder = shared_mut(Forwarder {
            observable: Shared::clone(&observable),
        });
        if let Some(pointee) = &pointee {
            pointee
                .observable()
                .register_observer(&(forwarder.clone() as SharedMut<dyn Observer>));
        }
        Link {
            current: pointee,
            observable,
            forwarder,
        }
    }

    /// Repoints the link, moving the pointee subscription from the old pointee
    /// to the new one, and returns the link's observable so the caller can
    /// notify *after* dropping the link borrow - observers commonly read or
    /// relink the handle from `update()`, which would otherwise re-borrow this
    /// cell.
    fn link_to(&mut self, pointee: Option<Shared<T>>) -> Shared<Observable> {
        let forwarder = self.forwarder.clone() as SharedMut<dyn Observer>;
        if let Some(old) = &self.current {
            old.observable().unregister_observer(&forwarder);
        }
        if let Some(new) = &pointee {
            new.observable().register_observer(&forwarder);
        }
        self.current = pointee;
        self.observable.clone()
    }
}

/// Shared handle to an observable pointee.
///
/// Cloning a `Handle` shares the same underlying [`Link`]; see
/// [`RelinkableHandle`] to relink it.
pub struct Handle<T: ?Sized> {
    link: SharedMut<Link<T>>,
}

impl<T: AsObservable + ?Sized> Handle<T> {
    /// Creates an empty handle.
    pub fn empty() -> Self {
        Handle {
            link: shared_mut(Link::new(None)),
        }
    }

    /// Creates a handle pointing at `pointee`, registering with its observable.
    pub fn new(pointee: Shared<T>) -> Self {
        Handle {
            link: shared_mut(Link::new(Some(pointee))),
        }
    }
}

impl<T: ?Sized> Handle<T> {
    /// Returns the current pointee, or an error if the handle is empty.
    ///
    /// Mirrors QuantLib's `currentLink`/`operator*`, which require a non-empty
    /// handle.
    pub fn current_link(&self) -> QlResult<Shared<T>> {
        require!(!self.is_empty(), "empty Handle cannot be dereferenced");
        Ok(self.link.borrow().current.clone().unwrap())
    }

    /// Whether the contained pointer points at anything.
    pub fn is_empty(&self) -> bool {
        self.link.borrow().is_empty()
    }

    /// Registers an observer with the underlying link.
    ///
    /// The observer is notified whenever the handle is relinked and whenever
    /// the current pointee notifies a change.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.link.borrow().observable.register_observer(observer)
    }

    /// Two handles are equal when they share the same underlying link.
    pub fn points_to_same_link(&self, other: &Handle<T>) -> bool {
        SharedMut::ptr_eq(&self.link, &other.link)
    }
}

impl<T: ?Sized> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle {
            link: SharedMut::clone(&self.link),
        }
    }
}

/// A [`Handle`] that can be relinked, propagating the change to all its copies.
pub struct RelinkableHandle<T: ?Sized> {
    handle: Handle<T>,
}

impl<T: AsObservable + ?Sized> RelinkableHandle<T> {
    /// Creates an empty relinkable handle.
    pub fn empty() -> Self {
        RelinkableHandle {
            handle: Handle::empty(),
        }
    }

    /// Creates a relinkable handle pointing at `pointee`.
    pub fn new(pointee: Shared<T>) -> Self {
        RelinkableHandle {
            handle: Handle::new(pointee),
        }
    }

    /// Points the shared link at `pointee`, moving the pointee subscription
    /// and notifying observers.
    pub fn link_to(&self, pointee: Shared<T>) {
        let observable = self.handle.link.borrow_mut().link_to(Some(pointee));
        observable.notify_observers();
    }

    /// Clears the shared link, dropping the pointee subscription and notifying
    /// observers.
    pub fn reset(&self) {
        let observable = self.handle.link.borrow_mut().link_to(None);
        observable.notify_observers();
    }
}

impl<T: ?Sized> RelinkableHandle<T> {
    /// Borrows this as a plain [`Handle`] (e.g. to hand out non-relinkable copies).
    pub fn handle(&self) -> Handle<T> {
        self.handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::shared::{shared, shared_mut};

    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    /// Minimal observable pointee for the link-sharing tests, standing in for
    /// the `handle.hpp` precondition that pointees derive from `Observable`.
    struct Pointee {
        value: i32,
        observable: Observable,
    }

    impl Pointee {
        fn new(value: i32) -> Shared<Pointee> {
            shared(Pointee {
                value,
                observable: Observable::new(),
            })
        }
    }

    impl AsObservable for Pointee {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    #[test]
    fn empty_handle_cannot_be_dereferenced() {
        let h: Handle<Pointee> = Handle::empty();
        assert!(h.is_empty());
        assert!(h.current_link().is_err());
    }

    #[test]
    fn handle_dereferences_to_pointee() {
        let h = Handle::new(Pointee::new(42));
        assert!(!h.is_empty());
        assert_eq!(h.current_link().unwrap().value, 42);
    }

    #[test]
    fn copies_share_one_link() {
        let h = Handle::new(Pointee::new(1));
        let copy = h.clone();
        assert!(h.points_to_same_link(&copy));
    }

    #[test]
    fn relink_propagates_notification_to_observers() {
        let rh = RelinkableHandle::new(Pointee::new(1));
        let observed = rh.handle();

        let flag = shared_mut(Flag::default());
        observed.register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        rh.link_to(Pointee::new(2));

        assert!(flag.borrow().up, "relink should notify observers");
        // the change is visible through the copy that shares the link
        assert_eq!(observed.current_link().unwrap().value, 2);
    }

    #[test]
    fn reset_empties_and_notifies() {
        let rh = RelinkableHandle::new(Pointee::new(1));
        let observed = rh.handle();
        let flag = shared_mut(Flag::default());
        observed.register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        rh.reset();

        assert!(flag.borrow().up);
        assert!(observed.is_empty());
    }

    /// Observer that reads its shared handle while being notified - the common
    /// "recompute on relink" pattern. This must not hit a `RefCell` borrow panic.
    struct Reader {
        handle: Handle<Pointee>,
        seen: Option<i32>,
    }

    impl Observer for Reader {
        fn update(&mut self) {
            self.seen = self.handle.current_link().ok().map(|p| p.value);
        }
    }

    #[test]
    fn observer_may_read_handle_during_relink() {
        let rh = RelinkableHandle::new(Pointee::new(1));
        let reader = shared_mut(Reader {
            handle: rh.handle(),
            seen: None,
        });
        rh.handle()
            .register_observer(&(reader.clone() as SharedMut<dyn Observer>));

        rh.link_to(Pointee::new(2));

        // the relink borrow is released before observers run, so the observer
        // can dereference the handle and sees the freshly-linked value
        assert_eq!(reader.borrow().seen, Some(2));
    }

    /// The deferred half of the ticket's oracle: an observer of a plain
    /// `Handle<SimpleQuote>` is notified when the underlying quote changes.
    #[test]
    #[allow(clippy::approx_constant)]
    fn pointee_change_notifies_handle_observers() {
        let quote = shared(SimpleQuote::new(0.0));
        let h: Handle<SimpleQuote> = Handle::new(quote.clone());
        let flag = shared_mut(Flag::default());
        h.register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        quote.set_value(3.14);

        assert!(
            flag.borrow().up,
            "observer was not notified of quote change"
        );
    }

    /// Port of `testObservableHandle` (test-suite/quotes.cpp), extended with
    /// the subscription-move asserts: the detached pointee must stop notifying
    /// and the newly linked one must start.
    #[test]
    #[allow(clippy::approx_constant)]
    fn observable_handle_forwards_pointee_changes_and_relinks() {
        let me1 = shared(SimpleQuote::new(0.0));
        let h: RelinkableHandle<dyn Quote> = RelinkableHandle::new(me1.clone());
        let f = shared_mut(Flag::default());
        h.handle()
            .register_observer(&(f.clone() as SharedMut<dyn Observer>));

        me1.set_value(3.14);
        assert!(f.borrow().up, "observer was not notified of quote change");

        f.borrow_mut().up = false;
        let me2 = shared(SimpleQuote::new(0.0));
        h.link_to(me2.clone());
        assert!(f.borrow().up, "observer was not notified of relink");

        f.borrow_mut().up = false;
        me1.set_value(1.0);
        assert!(
            !f.borrow().up,
            "detached pointee must no longer notify handle observers"
        );

        me2.set_value(2.0);
        assert!(
            f.borrow().up,
            "new pointee change must notify handle observers"
        );
    }

    #[test]
    fn reset_unhooks_the_old_pointee() {
        let quote = shared(SimpleQuote::new(1.0));
        let rh: RelinkableHandle<dyn Quote> = RelinkableHandle::new(quote.clone());
        let flag = shared_mut(Flag::default());
        rh.handle()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        rh.reset();
        flag.borrow_mut().up = false;

        quote.set_value(2.0);
        assert!(
            !flag.borrow().up,
            "reset must drop the pointee subscription"
        );
    }

    #[test]
    fn linking_an_empty_handle_starts_forwarding() {
        let rh: RelinkableHandle<dyn Quote> = RelinkableHandle::empty();
        let flag = shared_mut(Flag::default());
        rh.handle()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        let quote = shared(SimpleQuote::new(0.0));
        rh.link_to(quote.clone());
        flag.borrow_mut().up = false;

        quote.set_value(1.0);
        assert!(
            flag.borrow().up,
            "pointee linked after construction must forward"
        );
    }

    /// Dropping every handle drops the forwarder; the quote must keep working
    /// and silently prune the dead registration on its next notification.
    #[test]
    fn dropped_handle_stops_forwarding() {
        let quote = shared(SimpleQuote::new(0.0));
        {
            let _h: Handle<SimpleQuote> = Handle::new(quote.clone());
        }
        assert_eq!(quote.set_value(1.0), Some(1.0));
    }
}
