//! Visitor pattern.
//!
//! Port of `ql/patterns/visitor.hpp`. QuantLib's acyclic-visitor machinery
//! (`AcyclicVisitor` base + `Visitor<T>` per type, recovered at runtime via
//! `dynamic_cast`) exists to work around C++ single dispatch. In Rust the same
//! intent is expressed directly with a trait per visited type, so we keep only
//! the generic [`Visitor`] trait and drop the `AcyclicVisitor` base.
//!
//! The curiously-recurring template pattern (`ql/patterns/curiouslyrecurring.hpp`)
//! is not ported as a type: it is a C++ static-dispatch idiom whose equivalent
//! in Rust is provided by generics and trait default methods, so it has no
//! runtime behavior and no test to satisfy.

/// Visitor for a specific visited type `T`.
///
/// Mirrors QuantLib's `Visitor<T>`: a visited type calls `visit` on the
/// concrete visitor. Implement once per `(visitor, T)` pair.
pub trait Visitor<T> {
    /// Visits a value of type `T`.
    fn visit(&mut self, target: &mut T);
}
