//! Lattice methods (L9).
//!
//! Port of `ql/methods/lattices/`: recombining trees and the tree-based
//! lattice engines. [`Tree`] is the single-factor tree contract;
//! [`TrinomialTree`] is its recombining trinomial realisation;
//! [`Lattice`](lattice::Lattice) is the rollback interface discretized
//! assets price against.

pub mod lattice;
pub mod tree;
pub mod treelattice;
pub mod trinomialtree;

pub use lattice::Lattice;
pub use tree::Tree;
pub use treelattice::{TreeLattice, TreeLattice1D, TreeLatticeImpl};
pub use trinomialtree::TrinomialTree;
