//! Lattice methods (L9).
//!
//! Port of `ql/methods/lattices/`: recombining trees and the tree-based
//! lattice engines. [`Tree`] is the single-factor tree contract;
//! [`TrinomialTree`] is its recombining trinomial realisation;
//! [`Lattice`](lattice::Lattice) is the rollback interface discretized
//! assets price against.

pub mod lattice;
pub mod tree;
pub mod trinomialtree;

pub use tree::Tree;
pub use trinomialtree::TrinomialTree;
