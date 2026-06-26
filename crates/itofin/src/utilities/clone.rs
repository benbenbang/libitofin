//! Cloning proxy to an owned object.
//!
//! Port of `ql/utilities/clone.hpp`. QuantLib's `Clone<T>` is a `unique_ptr`
//! wrapper that deep-copies its pointee (via the pointee's `clone()`) on copy,
//! giving value semantics to a polymorphic, heap-owned object.
//!
//! In Rust this is largely subsumed by `Box<T>`, whose `Clone` impl already
//! deep-copies when `T: Clone`. [`Clone`] is kept as a thin owning box so the
//! C++ call sites translate directly and the intent stays explicit.

/// Owning box with deep-copy value semantics.
pub struct Clone<T> {
    ptr: Option<Box<T>>,
}

impl<T> Clone<T> {
    /// An empty clone holding nothing.
    pub fn empty() -> Self {
        Clone { ptr: None }
    }

    /// Wraps an owned value.
    pub fn new(value: T) -> Self {
        Clone {
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

    /// Swaps the contents of two clones.
    pub fn swap(&mut self, other: &mut Clone<T>) {
        std::mem::swap(&mut self.ptr, &mut other.ptr);
    }
}

impl<T: std::clone::Clone> std::clone::Clone for Clone<T> {
    fn clone(&self) -> Self {
        Clone {
            ptr: self.ptr.clone(),
        }
    }
}

impl<T> Default for Clone<T> {
    fn default() -> Self {
        Clone::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_filled() {
        let empty: Clone<i32> = Clone::empty();
        assert!(empty.is_empty());
        assert!(empty.get().is_none());

        let filled = Clone::new(7_i32);
        assert!(!filled.is_empty());
        assert_eq!(filled.get(), Some(&7));
    }

    #[test]
    fn clone_is_a_deep_copy() {
        let original = Clone::new(vec![1, 2, 3]);
        let mut copy = original.clone();
        copy.get_mut().unwrap().push(4);

        assert_eq!(original.get().unwrap().len(), 3);
        assert_eq!(copy.get().unwrap().len(), 4);
    }

    #[test]
    fn swap_exchanges_contents() {
        let mut a = Clone::new(1_i32);
        let mut b = Clone::new(2_i32);
        a.swap(&mut b);
        assert_eq!(a.get(), Some(&2));
        assert_eq!(b.get(), Some(&1));
    }
}
