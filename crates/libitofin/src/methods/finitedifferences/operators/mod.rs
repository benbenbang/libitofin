//! Finite-difference operators.
//!
//! Port of `ql/methods/finitedifferences/operators/`. The indexing primitives
//! every operator is built on land first: the grid odometer
//! [`FdmLinearOpIterator`] and the index space [`FdmLinearOpLayout`] it walks.

mod fdmlinearopiterator;
mod fdmlinearoplayout;

pub use fdmlinearopiterator::FdmLinearOpIterator;
pub use fdmlinearoplayout::FdmLinearOpLayout;
