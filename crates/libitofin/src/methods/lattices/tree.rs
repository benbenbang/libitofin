//! Tree interface for single-factor lattices.
//!
//! Port of `ql/methods/lattices/tree.hpp`. In C++ `Tree<T>` is a CRTP base
//! (`tree.hpp:50`) that stores only `columns_`; the node interface
//! (`underlying`/`size`/`descendant`/`probability` plus the `branches`
//! enumeration) is a *documentation* contract (`tree.hpp:34-46`), realised by
//! each derived class rather than by virtuals. Here that contract is a real
//! [`Tree`] trait, so `TreeLattice` (#462) can consume any tree generically.
//!
//! [`Tree::BRANCHES`] is an associated const (`2` binomial, `3` trinomial),
//! mirroring the C++ `enum { branches = N }`. An associated const makes the
//! trait not `dyn`-compatible; consumers take `T: Tree` generically, matching
//! the C++ template `TreeLattice<Impl>`.

use crate::types::{Real, Size};

/// A tree approximating a single-factor diffusion.
///
/// The lattice has [`columns`](Tree::columns) time slices; slice `i` holds
/// [`size(i)`](Tree::size) nodes. Each node has [`BRANCHES`](Tree::BRANCHES)
/// descendants in the next slice, reached through
/// [`descendant`](Tree::descendant) with the transition weights given by
/// [`probability`](Tree::probability).
pub trait Tree {
    /// Number of branches leaving each node (`2` binomial, `3` trinomial).
    const BRANCHES: Size;

    /// The number of time slices in the tree (`tree.hpp:54`).
    fn columns(&self) -> Size;

    /// The number of nodes on time slice `i`.
    fn size(&self, i: Size) -> Size;

    /// The state value of node `index` on time slice `i`.
    fn underlying(&self, i: Size, index: Size) -> Real;

    /// The index, on slice `i + 1`, of the node reached from node `index` on
    /// slice `i` along `branch` (`0..BRANCHES`).
    fn descendant(&self, i: Size, index: Size, branch: Size) -> Size;

    /// The transition probability from node `index` on slice `i` along
    /// `branch` (`0..BRANCHES`).
    fn probability(&self, i: Size, index: Size, branch: Size) -> Real;
}
