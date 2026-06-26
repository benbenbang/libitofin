//! Owning box with deep-copy value semantics.
//!
//! Port of `ql/utilities/clone.hpp`. QuantLib's `Clone<T>` is a `unique_ptr`
//! wrapper that deep-copies its pointee (via the pointee's `clone()`) on copy,
//! giving value semantics to a polymorphic, heap-owned object.
//!
//! In Rust this is largely subsumed by `Box<T>`, whose `Clone` impl already
//! deep-copies when `T: Clone`. [`ValueBox`] is kept as a thin owning box so the
//! C++ call sites translate directly and the intent stays explicit. It is named
//! `ValueBox` rather than `Clone` (QuantLib's name) to avoid shadowing the
//! standard `Clone` trait, which it also implements.

/// Owning box with deep-copy value semantics (QuantLib's `Clone<T>`).
pub struct ValueBox<T> {
    ptr: Option<Box<T>>,
}

impl<T> ValueBox<T> {
    /// An empty box holding nothing.
    pub fn empty() -> Self {
        ValueBox { ptr: None }
    }

    /// Wraps an owned value.
    pub fn new(value: T) -> Self {
        ValueBox {
            ptr: Some(Box::new(value)),
        }
    }

    /// Whether there is no underlying object.
    pub fn is_empty(&self) -> bool {
        self.ptr.is_none()
    }

    /// Borrows the underlying object.
    pub fn get(&self) -> Option<&T> {
        self.ptr.as_deref()
    }

    /// Mutably borrows the underlying object.
    pub fn get_mut(&mut self) -> Option<&mut T> {
        self.ptr.as_deref_mut()
    }

    /// Swaps the contents of two boxes.
    pub fn swap(&mut self, other: &mut ValueBox<T>) {
        std::mem::swap(&mut self.ptr, &mut other.ptr);
    }
}

impl<T: Clone> Clone for ValueBox<T> {
    fn clone(&self) -> Self {
        ValueBox {
            ptr: self.ptr.clone(),
        }
    }
}

impl<T> Default for ValueBox<T> {
    fn default() -> Self {
        ValueBox::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_filled() {
        let empty: ValueBox<i32> = ValueBox::empty();
        assert!(empty.is_empty());
        assert!(empty.get().is_none());

        let filled = ValueBox::new(7_i32);
        assert!(!filled.is_empty());
        assert_eq!(filled.get(), Some(&7));
    }

    #[test]
    fn clone_is_a_deep_copy() {
        let original = ValueBox::new(vec![1, 2, 3]);
        let mut copy = original.clone();
        copy.get_mut().unwrap().push(4);

        assert_eq!(original.get().unwrap().len(), 3);
        assert_eq!(copy.get().unwrap().len(), 4);
    }

    #[test]
    fn swap_exchanges_contents() {
        let mut a = ValueBox::new(1_i32);
        let mut b = ValueBox::new(2_i32);
        a.swap(&mut b);
        assert_eq!(a.get(), Some(&2));
        assert_eq!(b.get(), Some(&1));
    }
}
