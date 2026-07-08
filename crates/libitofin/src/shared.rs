//! Centralized smart-pointer aliases for the core.
//!
//! QuantLib uses `ext::shared_ptr<T>` pervasively. Per design decision D3 the
//! core is single-threaded-mutable, so we map it to [`Rc`] now; a future switch
//! to `Arc` (for `rayon`/PyO3) is a localized change to this module rather than
//! a scattered rewrite. Everything downstream must go through these aliases.

use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// Shared ownership of an immutable pointee (`ext::shared_ptr<T>`).
pub type Shared<T> = Rc<T>;

/// Shared ownership of a mutable pointee (`ext::shared_ptr<T>` over mutable state).
pub type SharedMut<T> = Rc<RefCell<T>>;

/// Non-owning back-reference used to break reference cycles (observer graph).
pub type WeakMut<T> = Weak<RefCell<T>>;

/// Constructs a [`Shared`] from a value.
pub fn shared<T>(value: T) -> Shared<T> {
    Rc::new(value)
}

/// Constructs a [`SharedMut`] from a value.
pub fn shared_mut<T>(value: T) -> SharedMut<T> {
    Rc::new(RefCell::new(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_clones_share_one_pointee() {
        let a = shared(7_i32);
        let b = Shared::clone(&a);
        assert!(Shared::ptr_eq(&a, &b));
        assert_eq!(Shared::strong_count(&a), 2);
    }

    #[test]
    fn shared_mut_mutation_is_visible_through_clones() {
        let a = shared_mut(1_i32);
        let b = SharedMut::clone(&a);
        *b.borrow_mut() = 42;
        assert_eq!(*a.borrow(), 42);
    }

    #[test]
    fn weak_does_not_keep_pointee_alive() {
        let weak: WeakMut<i32> = {
            let strong = shared_mut(3_i32);
            SharedMut::downgrade(&strong)
        };
        assert!(weak.upgrade().is_none());
    }
}
