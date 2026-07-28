//! Finite-difference operators.
//!
//! Port of `ql/methods/finitedifferences/operators/`. The indexing primitives
//! every operator is built on land first, starting with the grid odometer
//! [`FdmLinearOpIterator`].

mod fdmlinearopiterator;

pub use fdmlinearopiterator::FdmLinearOpIterator;
