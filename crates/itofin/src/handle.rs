//! Relinkable shared handle to an observable.
//!
//! Port of `ql/handle.hpp` (design decision D2). All copies of a [`Handle`]
//! share one inner `Link`; relinking through a [`RelinkableHandle`] swaps the
//! pointee for every copy and notifies the link's observers.
//!
//! QuantLib's `Link` also registers as an *observer of its pointee* so that
//! changes inside the pointee propagate through the handle. That wiring needs an
//! observable pointee type (e.g. `Quote`, EPIC-3) and is added once such types
//! exist; the EPIC-0 port covers the shared-link relink-and-notify behavior.

use crate::errors::QlResult;
use crate::patterns::observable::{Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut, shared_mut};

/// Inner shared cell of a [`Handle`]: the current pointee plus the link's own
/// observable, through which relinks are broadcast.
pub struct Link<T> {
    current: Option<Shared<T>>,
    observable: Shared<Observable>,
}

impl<T> Link<T> {
    fn new(pointee: Option<Shared<T>>) -> Self {
        Link {
            current: pointee,
            observable: Shared::new(Observable::new()),
        }
    }

    /// Repoints the link and returns its observable so the caller can notify
    /// *after* dropping the link borrow - observers commonly read or relink the
    /// handle from `update()`, which would otherwise re-borrow this cell.
    fn link_to(&mut self, pointee: Option<Shared<T>>) -> Shared<Observable> {
        self.current = pointee;
        self.observable.clone()
    }

    fn is_empty(&self) -> bool {
        self.current.is_none()
    }
}

/// Shared handle to an observable pointee.
///
/// Cloning a `Handle` shares the same underlying [`Link`]; see
/// [`RelinkableHandle`] to relink it.
pub struct Handle<T> {
    link: SharedMut<Link<T>>,
}

impl<T> Handle<T> {
    /// Creates an empty handle.
    pub fn empty() -> Self {
        Handle {
            link: shared_mut(Link::new(None)),
        }
    }

    /// Creates a handle pointing at `pointee`.
    pub fn new(pointee: Shared<T>) -> Self {
        Handle {
            link: shared_mut(Link::new(Some(pointee))),
        }
    }

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
    /// The observer is notified whenever the handle is relinked.
    pub fn register_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.link.borrow().observable.register_observer(observer)
    }

    /// Two handles are equal when they share the same underlying link.
    pub fn points_to_same_link(&self, other: &Handle<T>) -> bool {
        SharedMut::ptr_eq(&self.link, &other.link)
    }
}

impl<T> Clone for Handle<T> {
    fn clone(&self) -> Self {
        Handle {
            link: SharedMut::clone(&self.link),
        }
    }
}

/// A [`Handle`] that can be relinked, propagating the change to all its copies.
pub struct RelinkableHandle<T> {
    handle: Handle<T>,
}

impl<T> RelinkableHandle<T> {
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

    /// Points the shared link at `pointee`, notifying observers.
    pub fn link_to(&self, pointee: Shared<T>) {
        let observable = self.handle.link.borrow_mut().link_to(Some(pointee));
        observable.notify_observers();
    }

    /// Clears the shared link, notifying observers.
    pub fn reset(&self) {
        let observable = self.handle.link.borrow_mut().link_to(None);
        observable.notify_observers();
    }

    /// Borrows this as a plain [`Handle`] (e.g. to hand out non-relinkable copies).
    pub fn handle(&self) -> Handle<T> {
        self.handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    #[test]
    fn empty_handle_cannot_be_dereferenced() {
        let h: Handle<i32> = Handle::empty();
        assert!(h.is_empty());
        assert!(h.current_link().is_err());
    }

    #[test]
    fn handle_dereferences_to_pointee() {
        let h = Handle::new(shared(42_i32));
        assert!(!h.is_empty());
        assert_eq!(*h.current_link().unwrap(), 42);
    }

    #[test]
    fn copies_share_one_link() {
        let h = Handle::new(shared(1_i32));
        let copy = h.clone();
        assert!(h.points_to_same_link(&copy));
    }

    #[test]
    fn relink_propagates_notification_to_observers() {
        let rh = RelinkableHandle::new(shared(1_i32));
        let observed = rh.handle();

        let flag = shared_mut(Flag::default());
        observed.register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        rh.link_to(shared(2_i32));

        assert!(flag.borrow().up, "relink should notify observers");
        // the change is visible through the copy that shares the link
        assert_eq!(*observed.current_link().unwrap(), 2);
    }

    #[test]
    fn reset_empties_and_notifies() {
        let rh = RelinkableHandle::new(shared(1_i32));
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
        handle: Handle<i32>,
        seen: Option<i32>,
    }

    impl Observer for Reader {
        fn update(&mut self) {
            self.seen = self.handle.current_link().ok().map(|v| *v);
        }
    }

    #[test]
    fn observer_may_read_handle_during_relink() {
        let rh = RelinkableHandle::new(shared(1_i32));
        let reader = shared_mut(Reader {
            handle: rh.handle(),
            seen: None,
        });
        rh.handle()
            .register_observer(&(reader.clone() as SharedMut<dyn Observer>));

        rh.link_to(shared(2_i32));

        // the relink borrow is released before observers run, so the observer
        // can dereference the handle and sees the freshly-linked value
        assert_eq!(reader.borrow().seen, Some(2));
    }
}
