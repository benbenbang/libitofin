//! Tree-based lattice method: backward induction plus forward state prices.
//!
//! Port of `ql/methods/lattices/lattice.hpp` ([`TreeLattice`]) and
//! `ql/methods/lattices/lattice1d.hpp` ([`TreeLattice1D`]). A tree lattice rolls
//! a [`DiscretizedAsset`] backward through a [`TimeGrid`] (discounting at each
//! step) and, in the forward direction, accumulates the Arrow-Debreu state
//! prices used to value against the tree.
//!
//! # Shape: CRTP -> an impl trait
//! C++ `TreeLattice<Impl>` is CRTP: the base supplies the induction machinery
//! and calls back into `Impl` for `size`/`discount`/`descendant`/`probability`
//! (`lattice.hpp:39-52`); `TreeLattice1D<Impl>` adds `underlying`/`grid`
//! (`lattice1d.hpp:43-52`). Here the callback surface is the [`TreeLatticeImpl`]
//! trait. All but `discount` come from a [`Tree`]: `size`/`descendant`/
//! `probability`/`underlying` are already the [`Tree`] contract, so the impl
//! only exposes its [`tree`](TreeLatticeImpl::tree) plus the model-specific
//! [`discount`](TreeLatticeImpl::discount). #463's `ShortRateTree` supplies a
//! `TrinomialTree` and a discount read off the short-rate dynamics.
//!
//! [`TreeLattice`] carries the induction; [`TreeLattice1D`] newtype-wraps it,
//! implements the object-safe [`Lattice`] trait (so an asset can hold a
//! `Shared<dyn Lattice>`), and adds `grid`/`underlying`. It [`Deref`]s to the
//! base so state prices and rollback are reachable on the concrete 1D type.
//!
//! Divergences from QuantLib, all deliberate:
//! - Every driver returns [`QlResult`] (D4/D10) rather than `void`/`Real`.
//! - [`statePrices`](TreeLattice::state_prices) returns a cloned [`Array`], not
//!   a `const Array&`. The C++ reference invites a use-after-recompute alias
//!   (holding one slice, then requesting a further one runs the cached
//!   `computeStatePrices`); cloning the small per-slice array sidesteps it and
//!   keeps the borrow discipline simple for #463's per-node fit.
//! - Interior mutability is explicit: the cached slices live in a
//!   `RefCell<Vec<Array>>` and the computed-up-to cursor in a `Cell<Size>`,
//!   mirroring C++'s two `mutable` members (`lattice.hpp:87,91`).

use std::cell::{Cell, RefCell};

use crate::discretizedasset::DiscretizedAsset;
use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::comparison::close;
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::lattice::Lattice;
use crate::methods::lattices::tree::Tree;
use crate::require;
use crate::types::{Real, Size, Time};

/// The model-specific callback surface a [`TreeLattice`] induces over
/// (`lattice.hpp:39-52`).
///
/// `size`/`descendant`/`probability`/`underlying` come from the exposed
/// [`Tree`]; the lattice only asks the impl for the per-node
/// [`discount`](TreeLatticeImpl::discount), which a short-rate model derives
/// from its dynamics.
pub trait TreeLatticeImpl {
    /// The tree the lattice steps over.
    type Tree: Tree;

    /// The tree supplying `size`/`descendant`/`probability`/`underlying`.
    fn tree(&self) -> &Self::Tree;

    /// The one-step discount factor at node `index` on slice `i`
    /// (`lattice.hpp:42`). The `index` is load-bearing for a state-dependent
    /// short rate even though a flat rate ignores it.
    fn discount(&self, i: Size, index: Size) -> Real;
}

/// Tree-based lattice base: backward induction and forward state prices
/// (`lattice.hpp:56`). Abstract in C++ (no `grid`); here the concrete,
/// [`Lattice`]-implementing type is [`TreeLattice1D`], which wraps this.
pub struct TreeLattice<I: TreeLatticeImpl> {
    implementation: I,
    time_grid: TimeGrid,
    state_prices: RefCell<Vec<Array>>,
    state_prices_limit: Cell<Size>,
}

impl<I: TreeLatticeImpl> TreeLattice<I> {
    /// Builds a lattice over `time_grid` driven by `implementation`
    /// (`lattice.hpp:60`). The state prices seed with the single root node worth
    /// `1.0` and a limit of `0`.
    ///
    /// # Errors
    /// Returns `Err` if the tree has no branches ("there is no zeronomial
    /// lattice!", `lattice.hpp:63`).
    pub fn new(implementation: I, time_grid: TimeGrid) -> QlResult<Self> {
        require!(
            <I::Tree as Tree>::BRANCHES > 0,
            "there is no zeronomial lattice!"
        );
        Ok(TreeLattice {
            implementation,
            time_grid,
            state_prices: RefCell::new(vec![Array::filled(1, 1.0)]),
            state_prices_limit: Cell::new(0),
        })
    }

    /// The impl the lattice induces over.
    pub fn implementation(&self) -> &I {
        &self.implementation
    }

    /// The time grid the lattice rolls over (`lattice.hpp` `Lattice::t_`).
    pub fn time_grid(&self) -> &TimeGrid {
        &self.time_grid
    }

    fn size(&self, i: Size) -> Size {
        self.implementation.tree().size(i)
    }

    fn discount(&self, i: Size, index: Size) -> Real {
        self.implementation.discount(i, index)
    }

    fn descendant(&self, i: Size, index: Size, branch: Size) -> Size {
        self.implementation.tree().descendant(i, index, branch)
    }

    fn probability(&self, i: Size, index: Size, branch: Size) -> Real {
        self.implementation.tree().probability(i, index, branch)
    }

    /// The Arrow-Debreu state prices on slice `i` (`lattice.hpp:113`), computing
    /// forward as needed. Returns a clone of the cached slice (see module docs).
    pub fn state_prices(&self, i: Size) -> Array {
        if i > self.state_prices_limit.get() {
            self.compute_state_prices(i);
        }
        self.state_prices.borrow()[i].clone()
    }

    /// Accumulates the forward state prices up to slice `until`
    /// (`lattice.hpp:97`): each node feeds its descendants
    /// `statePrice * discount * probability`.
    fn compute_state_prices(&self, until: Size) {
        let branches = <I::Tree as Tree>::BRANCHES;
        let mut state_prices = self.state_prices.borrow_mut();
        for i in self.state_prices_limit.get()..until {
            state_prices.push(Array::filled(self.size(i + 1), 0.0));
            for j in 0..self.size(i) {
                let discount = self.discount(i, j);
                let state_price = state_prices[i][j];
                for branch in 0..branches {
                    let destination = self.descendant(i, j, branch);
                    let probability = self.probability(i, j, branch);
                    state_prices[i + 1][destination] += state_price * discount * probability;
                }
            }
        }
        self.state_prices_limit.set(until);
    }

    /// One backward induction step (`lattice.hpp:166`):
    /// `new_values[j] = discount(i,j) * sum_l probability(i,j,l) * values[descendant(i,j,l)]`.
    fn stepback(&self, i: Size, values: &Array, new_values: &mut Array) {
        let branches = <I::Tree as Tree>::BRANCHES;
        for j in 0..self.size(i) {
            let mut value = 0.0;
            for branch in 0..branches {
                value += self.probability(i, j, branch) * values[self.descendant(i, j, branch)];
            }
            value *= self.discount(i, j);
            new_values[j] = value;
        }
    }

    /// Initializes `asset` at time `t` (`lattice.hpp:127`).
    ///
    /// # Errors
    /// Returns `Err` if `t` is not a grid node or `reset` fails.
    pub fn initialize(&self, asset: &mut dyn DiscretizedAsset, t: Time) -> QlResult<()> {
        let i = self.time_grid.index(t)?;
        asset.set_time(t);
        asset.reset(self.size(i))
    }

    /// Rolls `asset` back to `to`, then applies the final adjustment
    /// (`lattice.hpp:133`).
    ///
    /// # Errors
    /// Propagates [`partial_rollback`](TreeLattice::partial_rollback) and
    /// adjustment failures.
    pub fn rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
        self.partial_rollback(asset, to)?;
        asset.adjust_values()
    }

    /// Rolls `asset` back to `to` without the final adjustment
    /// (`lattice.hpp:139`): step back slice by slice, adjusting at every
    /// intermediate node but skipping the destination (the caller, or
    /// [`rollback`](TreeLattice::rollback), performs it).
    ///
    /// # Errors
    /// Returns `Err` if `to` is later than the asset's current time, if either
    /// time is off-grid, or if an intermediate adjustment fails.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn partial_rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
        let from = asset.time();
        if close(from, to) {
            return Ok(());
        }
        require!(
            from > to,
            "cannot roll the asset back to {to} (it is already at t = {from})"
        );
        let i_from = self.time_grid.index(from)?;
        let i_to = self.time_grid.index(to)?;
        for i in (i_to..i_from).rev() {
            let mut new_values = Array::filled(self.size(i), 0.0);
            self.stepback(i, asset.values(), &mut new_values);
            asset.set_time(self.time_grid[i]);
            *asset.values_mut() = new_values;
            if i != i_to {
                asset.adjust_values()?;
            }
        }
        Ok(())
    }

    /// The present value of `asset`: the Arrow-Debreu dot product of its values
    /// with the state prices at its current time (`lattice.hpp:120`).
    ///
    /// # Errors
    /// Returns `Err` if the asset's time is not a grid node.
    pub fn present_value(&self, asset: &mut dyn DiscretizedAsset) -> QlResult<Real> {
        let i = self.time_grid.index(asset.time())?;
        let state_prices = self.state_prices(i);
        Ok(asset.values().dot(&state_prices))
    }
}

/// One-dimensional tree-based lattice (`lattice1d.hpp:38`): a [`TreeLattice`]
/// that also exposes the underlying state grid. This is the concrete type an
/// asset holds as a `Shared<dyn Lattice>`; #463's short-rate model wraps it.
pub struct TreeLattice1D<I: TreeLatticeImpl> {
    base: TreeLattice<I>,
}

impl<I: TreeLatticeImpl> TreeLattice1D<I> {
    /// Builds a 1-D lattice over `time_grid` driven by `implementation`
    /// (`lattice1d.hpp:41`).
    ///
    /// # Errors
    /// Propagates [`TreeLattice::new`].
    pub fn new(implementation: I, time_grid: TimeGrid) -> QlResult<Self> {
        Ok(TreeLattice1D {
            base: TreeLattice::new(implementation, time_grid)?,
        })
    }

    /// The state value of node `index` on slice `i` (`lattice1d.hpp:50`).
    pub fn underlying(&self, i: Size, index: Size) -> Real {
        self.base.implementation.tree().underlying(i, index)
    }
}

impl<I: TreeLatticeImpl> std::ops::Deref for TreeLattice1D<I> {
    type Target = TreeLattice<I>;

    /// Exposes the base induction surface (state prices, rollback) on the
    /// concrete 1-D type, modelling the C++ `TreeLattice1D : TreeLattice`.
    fn deref(&self) -> &TreeLattice<I> {
        &self.base
    }
}

impl<I: TreeLatticeImpl> Lattice for TreeLattice1D<I> {
    fn time_grid(&self) -> &TimeGrid {
        self.base.time_grid()
    }

    fn initialize(&self, asset: &mut dyn DiscretizedAsset, time: Time) -> QlResult<()> {
        self.base.initialize(asset, time)
    }

    fn rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
        self.base.rollback(asset, to)
    }

    fn partial_rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
        self.base.partial_rollback(asset, to)
    }

    fn present_value(&self, asset: &mut dyn DiscretizedAsset) -> QlResult<Real> {
        self.base.present_value(asset)
    }

    /// The underlying state grid at time `t` (`lattice1d.hpp:43`).
    fn grid(&self, t: Time) -> QlResult<Array> {
        let i = self.base.time_grid.index(t)?;
        let size = self.base.size(i);
        let mut grid = Array::filled(size, 0.0);
        for (j, value) in grid.iter_mut().enumerate() {
            *value = self.underlying(i, j);
        }
        Ok(grid)
    }
}
