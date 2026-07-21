//! Lattice methods (L9).
//!
//! Port of `ql/methods/lattices/`: recombining trees and the tree-based
//! lattice engines. [`Tree`] is the single-factor tree contract;
//! [`TrinomialTree`] is its recombining trinomial realisation.

pub mod tree;
pub mod trinomialtree;

pub use tree::Tree;
pub use trinomialtree::TrinomialTree;
