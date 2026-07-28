//! Finite-difference operators.
//!
//! Port of `ql/methods/finitedifferences/operators/`. This ticket lands the
//! indexing primitives every operator is built on -
//! [`FdmLinearOpIterator`] and [`FdmLinearOpLayout`] - plus the
//! [`FdmLinearOp`] contract the operators themselves implement.

mod fdmlinearop;
mod fdmlinearopiterator;
mod fdmlinearoplayout;
mod triplebandlinearop;

pub use fdmlinearop::FdmLinearOp;
pub use fdmlinearopiterator::FdmLinearOpIterator;
pub use fdmlinearoplayout::FdmLinearOpLayout;
pub use triplebandlinearop::TripleBandLinearOp;
