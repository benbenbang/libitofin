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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::discretizedasset::{DiscretizedAssetBase, DiscretizedDiscountBond};
    use crate::methods::lattices::trinomialtree::TrinomialTree;
    use crate::processes::OrnsteinUhlenbeckProcess;
    use crate::shared::{Shared, shared};
    use crate::stochasticprocess::StochasticProcess1D;
    use std::cell::Cell;

    // Ornstein-Uhlenbeck fixture with x0 != level so the drift is nonzero and
    // the per-node probabilities are asymmetric (p0 != p2) - required for the
    // descendant-swap perturbation to actually shift the distribution.
    const SPEED: Real = 0.1;
    const VOL: Real = 0.01;
    const X0: Real = 0.10;
    const LEVEL: Real = 0.05;
    const R: Real = 0.05;

    fn process() -> Shared<dyn StochasticProcess1D> {
        shared(OrnsteinUhlenbeckProcess::new(SPEED, VOL, X0, LEVEL).unwrap())
    }

    fn trinomial(steps: Size, end: Time) -> (TrinomialTree, TimeGrid) {
        let grid = TimeGrid::new(end, steps).unwrap();
        let tree = TrinomialTree::new(process(), grid.clone(), false).unwrap();
        (tree, grid)
    }

    /// Flat constant-short-rate lattice impl: `discount(i, index) = exp(-r*dt_i)`,
    /// independent of the node (the trivial oracle the ticket prescribes).
    struct FlatRate<T: Tree> {
        tree: T,
        grid: TimeGrid,
        rate: Real,
    }

    impl<T: Tree> TreeLatticeImpl for FlatRate<T> {
        type Tree = T;
        fn tree(&self) -> &T {
            &self.tree
        }
        fn discount(&self, i: Size, _index: Size) -> Real {
            (-self.rate * self.grid.dt(i)).exp()
        }
    }

    fn flat_lattice(steps: Size, end: Time) -> TreeLattice1D<FlatRate<TrinomialTree>> {
        let (tree, grid) = trinomial(steps, end);
        TreeLattice1D::new(
            FlatRate {
                tree,
                grid: grid.clone(),
                rate: R,
            },
            grid,
        )
        .unwrap()
    }

    /// Lattice impl with no discounting (`discount == 1`): a committed proxy for
    /// the "drop the discount" confirm-by-stub. Its state prices sum to `1` at
    /// every slice, so the discount-sum identity fails.
    struct NoDiscount<T: Tree> {
        tree: T,
    }

    impl<T: Tree> TreeLatticeImpl for NoDiscount<T> {
        type Tree = T;
        fn tree(&self) -> &T {
            &self.tree
        }
        fn discount(&self, _i: Size, _index: Size) -> Real {
            1.0
        }
    }

    /// A tree that swaps the branch-0 and branch-2 destinations at one node,
    /// leaving every probability, size, and underlying untouched. The total
    /// probability leaving the node is conserved (so the discount-sum identity
    /// is blind to it), but the per-node Arrow-Debreu distribution shifts.
    struct SwapDescTree {
        inner: TrinomialTree,
        swap_slice: Size,
        swap_node: Size,
    }

    impl Tree for SwapDescTree {
        const BRANCHES: Size = 3;
        fn columns(&self) -> Size {
            self.inner.columns()
        }
        fn size(&self, i: Size) -> Size {
            self.inner.size(i)
        }
        fn underlying(&self, i: Size, index: Size) -> Real {
            self.inner.underlying(i, index)
        }
        fn probability(&self, i: Size, index: Size, branch: Size) -> Real {
            self.inner.probability(i, index, branch)
        }
        fn descendant(&self, i: Size, index: Size, branch: Size) -> Size {
            if i == self.swap_slice && index == self.swap_node {
                let swapped = match branch {
                    0 => 2,
                    2 => 0,
                    b => b,
                };
                self.inner.descendant(i, index, swapped)
            } else {
                self.inner.descendant(i, index, branch)
            }
        }
    }

    /// Counts how often `post_adjust_values_impl` actually fires, to pin the
    /// partial-rollback skip-then-final-adjust order.
    #[derive(Default)]
    struct AdjustCountingAsset {
        base: DiscretizedAssetBase,
        post_adjusts: Cell<u32>,
    }

    impl DiscretizedAsset for AdjustCountingAsset {
        fn base(&self) -> &DiscretizedAssetBase {
            &self.base
        }
        fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
            &mut self.base
        }
        fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
            self
        }
        fn reset(&mut self, size: Size) -> QlResult<()> {
            *self.values_mut() = Array::filled(size, 0.0);
            Ok(())
        }
        fn mandatory_times(&self) -> Vec<Time> {
            Vec::new()
        }
        fn post_adjust_values_impl(&mut self) -> QlResult<()> {
            self.post_adjusts.set(self.post_adjusts.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn zeronomial_lattice_is_rejected() {
        // lattice.hpp:63: a tree with no branches has no lattice.
        struct NoBranch;
        impl Tree for NoBranch {
            const BRANCHES: Size = 0;
            fn columns(&self) -> Size {
                1
            }
            fn size(&self, _i: Size) -> Size {
                1
            }
            fn underlying(&self, _i: Size, _index: Size) -> Real {
                0.0
            }
            fn descendant(&self, _i: Size, _index: Size, _branch: Size) -> Size {
                0
            }
            fn probability(&self, _i: Size, _index: Size, _branch: Size) -> Real {
                0.0
            }
        }
        let grid = TimeGrid::new(1.0, 2).unwrap();
        let err = TreeLattice::new(NoDiscount { tree: NoBranch }, grid)
            .err()
            .expect("a zeronomial lattice must be rejected");
        assert_eq!(err.message(), "there is no zeronomial lattice!");
    }

    #[test]
    fn state_prices_discount_sum_to_the_zero_bond() {
        // lattice.hpp:97: sum_j state_prices(i)[j] == exp(-r*t_i), the
        // Arrow-Debreu discount-sum. Catches discount/probability-magnitude bugs
        // (but NOT descendant routing - see the perturbation test).
        let steps = 5;
        let lattice = flat_lattice(steps, 1.0);
        let grid = lattice.time_grid().clone();
        for i in 0..=steps {
            let sum: Real = lattice.state_prices(i).iter().sum();
            let bond = (-R * grid[i]).exp();
            assert!((sum - bond).abs() < 1e-13, "sum(sp[{i}]) = {sum} != {bond}");
        }
    }

    #[test]
    fn present_value_of_a_unit_bond_is_the_discount_sum() {
        // lattice.hpp:120: presentValue = DotProduct(values, statePrices(i)).
        // A discount bond initialized at t_i has values all 1, so its present
        // value is exactly the discount-sum. Exercises the ported Lattice method.
        let steps = 5;
        let i = 3;
        let lattice: Shared<dyn Lattice> = shared(flat_lattice(steps, 1.0));
        let grid = lattice.time_grid().clone();
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(Shared::clone(&lattice), grid[i]).unwrap();
        let pv = bond.present_value().unwrap();
        assert!(
            (pv - (-R * grid[i]).exp()).abs() < 1e-13,
            "pv = {pv} != {}",
            (-R * grid[i]).exp()
        );
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn state_prices_match_an_independent_forward_recursion_elementwise() {
        // GATE AMENDMENT (distribution-sensitive pin): assert the state prices
        // ELEMENT-WISE against an independent Arrow-Debreu recursion written out
        // here from the tree's OWN descendants/probabilities plus the flat
        // discount. This re-derivation routes through tree.descendant, so a
        // routing bug in compute_state_prices (which the sum-identity cannot see)
        // makes the two diverge.
        let steps = 4;
        let (tree, grid) = trinomial(steps, 1.0);
        let disc: Vec<Real> = (0..steps).map(|i| (-R * grid.dt(i)).exp()).collect();

        let mut expected: Vec<Vec<Real>> = vec![vec![1.0]];
        for i in 0..steps {
            let mut next = vec![0.0; tree.size(i + 1)];
            for j in 0..tree.size(i) {
                let sp = expected[i][j];
                for l in 0..3 {
                    let d = tree.descendant(i, j, l);
                    next[d] += sp * disc[i] * tree.probability(i, j, l);
                }
            }
            expected.push(next);
        }

        let lattice = TreeLattice1D::new(
            FlatRate {
                tree,
                grid: grid.clone(),
                rate: R,
            },
            grid,
        )
        .unwrap();
        for i in 0..=steps {
            let sp = lattice.state_prices(i);
            assert_eq!(sp.size(), expected[i].len(), "slice {i} size mismatch");
            for j in 0..sp.size() {
                assert!(
                    (sp[j] - expected[i][j]).abs() < 1e-14,
                    "sp[{i}][{j}] = {} != {}",
                    sp[j],
                    expected[i][j]
                );
            }
        }
    }

    #[test]
    fn descendant_perturbation_evades_sum_identity_but_the_elementwise_pin_catches_it() {
        // GATE AMENDMENT (confirm-by-stub, both outcomes recorded): a descendant
        // swap that conserves total transition probability is INVISIBLE to the
        // discount-sum identity yet shifts the per-node distribution. This test
        // demonstrates BOTH: the sum-identity still passes on the perturbed
        // lattice, while the element-wise pin and the exact fit quantity #463
        // consumes both catch it. This is why the element-wise pin earns its keep.
        let steps = 3;
        let end = 1.0;
        let i = 2;

        let (correct_tree, grid) = trinomial(steps, end);
        let correct = TreeLattice1D::new(
            FlatRate {
                tree: correct_tree,
                grid: grid.clone(),
                rate: R,
            },
            grid.clone(),
        )
        .unwrap();

        let (inner, _g) = trinomial(steps, end);
        let perturbed = TreeLattice1D::new(
            FlatRate {
                tree: SwapDescTree {
                    inner,
                    swap_slice: 1,
                    swap_node: 0,
                },
                grid: grid.clone(),
                rate: R,
            },
            grid.clone(),
        )
        .unwrap();

        let sp_correct = correct.state_prices(i);
        let sp_perturbed = perturbed.state_prices(i);
        let bond = (-R * grid[i]).exp();

        // (a) The discount-sum identity is BLIND to the swap: both still hit the bond.
        let sum_correct: Real = sp_correct.iter().sum();
        let sum_perturbed: Real = sp_perturbed.iter().sum();
        assert!(
            (sum_correct - bond).abs() < 1e-13,
            "correct sum {sum_correct} != {bond}"
        );
        assert!(
            (sum_perturbed - bond).abs() < 1e-13,
            "perturbed sum {sum_perturbed} != {bond}: the sum-identity must stay blind"
        );

        // (b) The element-wise pin CATCHES it: some node differs materially.
        let max_elt_diff = (0..sp_correct.size())
            .map(|j| (sp_correct[j] - sp_perturbed[j]).abs())
            .fold(0.0_f64, Real::max);
        assert!(
            max_elt_diff > 1e-6,
            "element-wise pin failed to distinguish the perturbation (max diff {max_elt_diff})"
        );

        // (c) The fit quantity #463 consumes (sum_j sp[j]*exp(-underlying(i,j)*dt))
        // also differs; underlying is unchanged by the swap, so the gap isolates
        // the distribution shift.
        let dt = grid.dt(i);
        let fit_correct: Real = (0..sp_correct.size())
            .map(|j| sp_correct[j] * (-correct.underlying(i, j) * dt).exp())
            .sum();
        let fit_perturbed: Real = (0..sp_perturbed.size())
            .map(|j| sp_perturbed[j] * (-perturbed.underlying(i, j) * dt).exp())
            .sum();
        assert!(
            (fit_correct - fit_perturbed).abs() > 1e-6,
            "fit quantity failed to distinguish the perturbation: {fit_correct} vs {fit_perturbed}"
        );
    }

    #[test]
    fn dropping_the_discount_breaks_the_sum_identity() {
        // GATE AMENDMENT (confirm-by-stub, discount direction): with discount==1
        // the state prices sum to 1 at every slice, NOT exp(-r*t_i), so the
        // discount-sum identity fails - a committed proxy proving the pin is
        // sensitive to the discount factor.
        let steps = 3;
        let (tree, grid) = trinomial(steps, 1.0);
        let lattice = TreeLattice1D::new(NoDiscount { tree }, grid.clone()).unwrap();
        for i in 1..=steps {
            let sum: Real = lattice.state_prices(i).iter().sum();
            assert!((sum - 1.0).abs() < 1e-13, "undiscounted sum {sum} != 1");
            let bond = (-R * grid[i]).exp();
            assert!(
                (sum - bond).abs() > 1e-6,
                "the sum-identity must FAIL without the discount (sum {sum} vs bond {bond})"
            );
        }
    }

    #[test]
    fn constant_payoff_rolls_back_to_payoff_times_bond() {
        // lattice.hpp:166 (stepback): a constant terminal payoff k rolls back to
        // k * exp(-r*T) (== k * the zero bond), pinning the discount*probability
        // magnitude in the backward step.
        let steps = 4;
        let end = 1.0;
        let k = 5.0;
        let lattice: Shared<dyn Lattice> = shared(flat_lattice(steps, end));
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(Shared::clone(&lattice), end).unwrap();
        let size = bond.values().size();
        *bond.values_mut() = Array::filled(size, k);
        bond.rollback(0.0).unwrap();
        let expected = k * (-R * end).exp();
        assert!(
            (bond.values()[0] - expected).abs() < 1e-13,
            "rolled back to {} != {expected}",
            bond.values()[0]
        );
    }

    #[test]
    fn partial_rollback_skips_the_destination_adjust_and_rollback_supplies_it() {
        // lattice.hpp:139-163: partialRollback adjusts every intermediate node but
        // skips the destination ("skip the very last adjustment"); rollback then
        // supplies exactly that one final adjust.
        let steps = 4;
        let end = 1.0;
        let lattice: Shared<dyn Lattice> = shared(flat_lattice(steps, end));

        let mut full_asset = AdjustCountingAsset::default();
        full_asset.initialize(Shared::clone(&lattice), end).unwrap();
        full_asset.rollback(0.0).unwrap();
        let full = full_asset.post_adjusts.get();

        let mut partial_asset = AdjustCountingAsset::default();
        partial_asset
            .initialize(Shared::clone(&lattice), end)
            .unwrap();
        partial_asset.partial_rollback(0.0).unwrap();
        let partial = partial_asset.post_adjusts.get();

        assert_eq!(
            partial,
            steps as u32 - 1,
            "partial_rollback should adjust every intermediate node, skipping the destination"
        );
        assert_eq!(
            full, steps as u32,
            "rollback should add exactly the destination adjust"
        );
        assert_eq!(
            full,
            partial + 1,
            "the skipped destination adjust is supplied once by rollback"
        );
    }

    #[test]
    fn stepback_gathers_each_branch_from_its_descendant() {
        // lattice.hpp:166: pin the BACKWARD gather routing. A constant payoff is
        // routing-invariant (sum_l prob == 1), so roll a DISTINCT-per-node payoff
        // back one step and assert element-wise against a hand-gather off the
        // tree's own descendants - the backward analog of the forward
        // element-wise pin, covering the separate stepback (gather) code path.
        let steps = 4;
        let end = 1.0;
        let i = 3;
        let (tree, grid) = trinomial(steps, end);
        let lattice: Shared<dyn Lattice> = shared(flat_lattice(steps, end));

        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(Shared::clone(&lattice), grid[i]).unwrap();
        let terminal: Vec<Real> = (0..tree.size(i)).map(|j| 1.0 + j as Real).collect();
        *bond.values_mut() = Array::from(terminal.clone());
        bond.rollback(grid[i - 1]).unwrap();

        let disc = (-R * grid.dt(i - 1)).exp();
        for j in 0..tree.size(i - 1) {
            let mut expected = 0.0;
            for l in 0..3 {
                expected += tree.probability(i - 1, j, l) * terminal[tree.descendant(i - 1, j, l)];
            }
            expected *= disc;
            assert!(
                (bond.values()[j] - expected).abs() < 1e-13,
                "stepback[{j}] = {} != {expected}",
                bond.values()[j]
            );
        }
    }

    #[test]
    fn grid_returns_the_underlying_state_nodes() {
        // lattice1d.hpp:43: grid(t) is the underlying value at each node of slice
        // index(t).
        let steps = 3;
        let end = 1.0;
        let i = 2;
        let (tree, grid) = trinomial(steps, end);
        let lattice = flat_lattice(steps, end);
        let g = lattice.grid(grid[i]).unwrap();
        assert_eq!(g.size(), tree.size(i));
        for j in 0..tree.size(i) {
            assert!(
                (g[j] - tree.underlying(i, j)).abs() < 1e-15,
                "grid[{j}] = {} != {}",
                g[j],
                tree.underlying(i, j)
            );
        }
    }
}
