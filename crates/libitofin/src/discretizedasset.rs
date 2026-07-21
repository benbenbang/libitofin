//! Discretized assets: the rollback protagonists driven by a [`Lattice`].
//!
//! Port of `ql/discretizedasset.{hpp,cpp}`: the [`DiscretizedAsset`] trait
//! (base contract), [`DiscretizedDiscountBond`] and [`DiscretizedOption`].
//!
//! # The asset <-> lattice ownership cycle (the central design decision)
//! In QuantLib a `DiscretizedAsset` holds an `ext::shared_ptr<Lattice>`
//! (`method_`) and every high-level call (`rollback`, `partialRollback`,
//! `presentValue`, `initialize`) hands `*this` back to that lattice, which then
//! mutates the asset. A naive Rust translation would borrow the asset while
//! also reaching through the lattice it owns - an alias. We resolve it by
//! CLONING the `Shared<dyn Lattice>` out of the asset before the delegating
//! call, so the owned handle and the `&mut` asset never alias:
//! ```ignore
//! let method = self.require_method()?;      // Rc clone, borrow of self ends
//! method.rollback(self.as_asset_mut(), to)  // fresh &mut, no alias
//! ```
//! The same discipline extends to [`DiscretizedOption`]'s underlying: its
//! `post_adjust_values_impl` calls `underlying.partial_rollback(time)` (an
//! asset-to-asset coupling), so the underlying's [`SharedMut`] handle is cloned
//! out before it is borrowed (see [`DiscretizedOption::post_adjust_values_impl`]).
//!
//! ## `as_asset_mut`
//! The high-level methods are provided (default) trait methods. Rust cannot
//! unsize-coerce `self: &mut Self` (`Self: ?Sized`) to `&mut dyn
//! DiscretizedAsset` inside a default body, so each concrete asset implements
//! the required [`DiscretizedAsset::as_asset_mut`] returning `{ self }` (there
//! `Self: Sized`, so the coercion is legal). The default methods route through
//! it to obtain the trait object the lattice expects.
//!
//! Divergences from QuantLib, all deliberate:
//! - Every delegating / adjusting method returns [`QlResult`] (D4/D10) rather
//!   than `void`.
//! - [`DiscretizedAsset::is_on_time`] cannot use `TimeGrid::index` (not ported);
//!   see its doc comment for the exact behavioural consequence.

use crate::errors::QlResult;
use crate::exercise::ExerciseType;
use crate::math::array::Array;
use crate::math::comparison::close_enough;
use crate::methods::lattices::lattice::Lattice;
use crate::shared::{Shared, SharedMut};
use crate::types::{Real, Size, Time};

/// Whether a coupon is applied in the pre- or post-adjustment pass
/// (`discretizedasset.hpp:128`, `enum class CouponAdjustment { pre, post }`).
///
/// Used by [`DiscretizedSwap`](crate::pricingengines::swaption::DiscretizedSwap)
/// to route each leg's coupons: a normal coupon adjusts in the pre pass, while a
/// coupon whose reset is already in the past flips to the post pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CouponAdjustment {
    /// Applied in `pre_adjust_values` (`CouponAdjustment::pre`).
    Pre,
    /// Applied in `post_adjust_values` (`CouponAdjustment::post`).
    Post,
}

/// The state every [`DiscretizedAsset`] embeds (QuantLib's protected/private
/// data members of `DiscretizedAsset`).
///
/// A concrete asset holds one of these and exposes it through
/// [`DiscretizedAsset::base`] / [`DiscretizedAsset::base_mut`], mirroring the
/// `BlackCalibrationHelperBase` precedent.
pub struct DiscretizedAssetBase {
    time: Time,
    latest_pre_adjustment: Time,
    latest_post_adjustment: Time,
    values: Array,
    method: Option<Shared<dyn Lattice>>,
}

impl Default for DiscretizedAssetBase {
    /// The `QL_MAX_REAL` sentinels of `DiscretizedAsset()` (`discretizedasset.hpp:38`):
    /// the latest-adjustment marks start "never", so the first adjustment fires.
    fn default() -> Self {
        DiscretizedAssetBase {
            time: 0.0,
            latest_pre_adjustment: Real::MAX,
            latest_post_adjustment: Real::MAX,
            values: Array::new(),
            method: None,
        }
    }
}

/// A discretized asset used by numerical methods (`discretizedasset.hpp:36`).
///
/// A concrete asset provides its embedded [`DiscretizedAssetBase`] plus the
/// low-level `reset` / `mandatory_times`; the high-level rollback interface and
/// the pre/post-adjustment guards are provided here as defaults.
pub trait DiscretizedAsset {
    /// The embedded base state.
    fn base(&self) -> &DiscretizedAssetBase;
    /// The embedded base state, mutably.
    fn base_mut(&mut self) -> &mut DiscretizedAssetBase;
    /// Upcast to a trait object (each impl writes `{ self }`); see the module
    /// docs for why the delegating defaults need it.
    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset;

    /// Initialize the asset values to an `Array` of the given size with
    /// asset-specific values (`discretizedasset.hpp:81`).
    fn reset(&mut self, size: Size) -> QlResult<()>;

    /// The times at which the numerical method must stop while rolling back
    /// (`discretizedasset.hpp:120`). Not guaranteed sorted.
    fn mandatory_times(&self) -> Vec<Time>;

    /// The actual pre-adjustment (`discretizedasset.hpp:135`). Default: no-op.
    fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
        Ok(())
    }

    /// The actual post-adjustment (`discretizedasset.hpp:137`). Default: no-op.
    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        Ok(())
    }

    /// The current rollback time (`discretizedasset.hpp:42`).
    fn time(&self) -> Time {
        self.base().time
    }

    /// Sets the current time (C++ `Time& time()`).
    fn set_time(&mut self, time: Time) {
        self.base_mut().time = time;
    }

    /// The current asset values (`discretizedasset.hpp:45`).
    fn values(&self) -> &Array {
        &self.base().values
    }

    /// The current asset values, mutably (C++ `Array& values()`).
    fn values_mut(&mut self) -> &mut Array {
        &mut self.base_mut().values
    }

    /// The lattice the asset was initialized on (`discretizedasset.hpp:48`), or
    /// `None` before [`initialize`](DiscretizedAsset::initialize).
    fn method(&self) -> Option<&Shared<dyn Lattice>> {
        self.base().method.as_ref()
    }

    /// Initialize on a method at time `t` (`discretizedasset.hpp:182`): store the
    /// lattice, then let it initialize this asset.
    fn initialize(&mut self, method: Shared<dyn Lattice>, t: Time) -> QlResult<()> {
        self.base_mut().method = Some(Shared::clone(&method));
        method.initialize(self.as_asset_mut(), t)
    }

    /// Roll the asset back to `to` (`discretizedasset.hpp:188`).
    fn rollback(&mut self, to: Time) -> QlResult<()> {
        let method = self.require_method()?;
        method.rollback(self.as_asset_mut(), to)
    }

    /// Roll the asset back to `to` without the final adjustment
    /// (`discretizedasset.hpp:192`).
    fn partial_rollback(&mut self, to: Time) -> QlResult<()> {
        let method = self.require_method()?;
        method.partial_rollback(self.as_asset_mut(), to)
    }

    /// The present value of the asset (`discretizedasset.hpp:196`).
    fn present_value(&mut self) -> QlResult<Real> {
        let method = self.require_method()?;
        method.present_value(self.as_asset_mut())
    }

    /// Pre-adjustment guarded against a double-fire at the same time
    /// (`discretizedasset.hpp:200`): runs [`pre_adjust_values_impl`] once per
    /// distinct `time()`, deduped with `close_enough`.
    fn pre_adjust_values(&mut self) -> QlResult<()> {
        if !close_enough(self.time(), self.base().latest_pre_adjustment) {
            self.pre_adjust_values_impl()?;
            self.base_mut().latest_pre_adjustment = self.base().time;
        }
        Ok(())
    }

    /// Post-adjustment, guarded like [`pre_adjust_values`]
    /// (`discretizedasset.hpp:207`).
    fn post_adjust_values(&mut self) -> QlResult<()> {
        if !close_enough(self.time(), self.base().latest_post_adjustment) {
            self.post_adjust_values_impl()?;
            self.base_mut().latest_post_adjustment = self.base().time;
        }
        Ok(())
    }

    /// Both pre- and post-adjustment (`discretizedasset.hpp:110`).
    fn adjust_values(&mut self) -> QlResult<()> {
        self.pre_adjust_values()?;
        self.post_adjust_values()
    }

    /// Whether the asset was rolled to `t`'s grid node (`discretizedasset.hpp:211`).
    ///
    /// # Divergence
    /// QuantLib evaluates `close_enough(grid[grid.index(t)], time())`.
    /// `TimeGrid::index` is not ported, so this finds the grid node nearest `t`
    /// (an inline `close_enough`/argmin search) and compares it to `time()`.
    /// The observable difference: QuantLib's `index` throws when `t` is far from
    /// every node; this returns the nearest node regardless, so a match is only
    /// meaningful when `t` is grid-aligned - which holds for the sole caller
    /// ([`DiscretizedOption`] exercise times, placed on the grid by construction).
    ///
    /// # Panics
    /// Panics (via `expect`) if the asset was never initialized on a method,
    /// mirroring C++'s dereference of a null `method()`.
    fn is_on_time(&self, t: Time) -> bool {
        let grid = self
            .method()
            .expect("asset is not initialized on any method")
            .time_grid();
        let times = grid.times();
        let mut best = 0usize;
        for i in 1..times.len() {
            if (times[i] - t).abs() < (times[best] - t).abs() {
                best = i;
            }
        }
        close_enough(times[best], self.time())
    }

    /// Clones the lattice handle out of the asset, or errors if unset. The clone
    /// is what lets the delegating call avoid aliasing `self`.
    fn require_method(&self) -> QlResult<Shared<dyn Lattice>> {
        match self.method() {
            Some(m) => Ok(Shared::clone(m)),
            None => crate::fail!("asset is not initialized on any method"),
        }
    }
}

/// A discretized zero-coupon discount bond (`discretizedasset.hpp:147`): worth
/// 1 at maturity, rolled back to today by the lattice to yield the discount.
#[derive(Default)]
pub struct DiscretizedDiscountBond {
    base: DiscretizedAssetBase,
}

impl DiscretizedDiscountBond {
    /// A fresh discount bond, uninitialized until placed on a method.
    pub fn new() -> Self {
        Self::default()
    }
}

impl DiscretizedAsset for DiscretizedDiscountBond {
    fn base(&self) -> &DiscretizedAssetBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
        &mut self.base
    }

    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
        self
    }

    /// `values_ = Array(size, 1.0)` (`discretizedasset.hpp:150`).
    fn reset(&mut self, size: Size) -> QlResult<()> {
        self.base.values = Array::filled(size, 1.0);
        Ok(())
    }

    /// Empty (`discretizedasset.hpp:151`).
    fn mandatory_times(&self) -> Vec<Time> {
        Vec::new()
    }
}
/// A discretized option on an underlying asset (`discretizedasset.hpp:160`).
///
/// Holds the underlying as a [`SharedMut`] so an exercise can roll it back and
/// read its values (the asset-to-asset coupling flagged in the module docs).
/// The clone-out-before-borrow discipline applies to that handle too.
pub struct DiscretizedOption {
    base: DiscretizedAssetBase,
    underlying: SharedMut<dyn DiscretizedAsset>,
    exercise_type: ExerciseType,
    exercise_times: Vec<Time>,
}

impl DiscretizedOption {
    /// An option on `underlying` exercisable in `exercise_type` style at
    /// `exercise_times` (`discretizedasset.hpp:164`).
    pub fn new(
        underlying: SharedMut<dyn DiscretizedAsset>,
        exercise_type: ExerciseType,
        exercise_times: Vec<Time>,
    ) -> Self {
        DiscretizedOption {
            base: DiscretizedAssetBase::default(),
            underlying,
            exercise_type,
            exercise_times,
        }
    }

    /// The underlying asset handle.
    pub fn underlying(&self) -> &SharedMut<dyn DiscretizedAsset> {
        &self.underlying
    }

    /// `values_[i] = max(underlying_->values()[i], values_[i])`
    /// (`discretizedasset.hpp:225`).
    fn apply_exercise_condition(&mut self) {
        let underlying = self.underlying.borrow();
        for i in 0..self.base.values.size() {
            self.base.values[i] = self.base.values[i].max(underlying.values()[i]);
        }
    }
}

impl DiscretizedAsset for DiscretizedOption {
    fn base(&self) -> &DiscretizedAssetBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
        &mut self.base
    }

    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
        self
    }

    /// `reset` (`discretizedasset.hpp:218`): require the option and its
    /// underlying share one method, zero the values, then adjust.
    fn reset(&mut self, size: Size) -> QlResult<()> {
        let same = match (self.method(), self.underlying.borrow().method()) {
            (Some(a), Some(b)) => Shared::ptr_eq(a, b),
            _ => false,
        };
        crate::require!(
            same,
            "option and underlying were initialized on different methods"
        );
        self.base.values = Array::filled(size, 0.0);
        self.adjust_values()
    }

    /// `mandatoryTimes` (`discretizedasset.hpp:231`): the underlying's times plus
    /// the exercise times from the first non-negative one onward.
    fn mandatory_times(&self) -> Vec<Time> {
        let mut times = self.underlying.borrow().mandatory_times();
        if let Some(start) = self.exercise_times.iter().position(|&t| t >= 0.0) {
            times.extend_from_slice(&self.exercise_times[start..]);
        }
        times
    }

    /// `postAdjustValuesImpl` (`discretizedasset.cpp:25`). Time flows backward,
    /// so the underlying is rolled to this time and pre-adjusted, the exercise
    /// condition is applied, then the underlying is post-adjusted. The exact
    /// ordering matters and mirrors the C++ line-for-line.
    ///
    /// # Panics
    /// The American branch reads `exercise_times[0]` and `[1]` as the exercise
    /// window bounds, faithful to C++ (`discretizedasset.cpp:38`), so it panics
    /// if fewer than two exercise times were supplied. QuantLib's unchecked
    /// `std::vector::operator[]` is equally undefined there; callers building an
    /// American option must pass the two-element window.
    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        let underlying = SharedMut::clone(&self.underlying);
        let t = self.time();
        underlying.borrow_mut().partial_rollback(t)?;
        underlying.borrow_mut().pre_adjust_values()?;
        match self.exercise_type {
            ExerciseType::American => {
                if self.base.time >= self.exercise_times[0]
                    && self.base.time <= self.exercise_times[1]
                {
                    self.apply_exercise_condition();
                }
            }
            ExerciseType::Bermudan | ExerciseType::European => {
                for i in 0..self.exercise_times.len() {
                    let et = self.exercise_times[i];
                    if et >= 0.0 && self.is_on_time(et) {
                        self.apply_exercise_condition();
                    }
                }
            }
        }
        underlying.borrow_mut().post_adjust_values()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::timegrid::TimeGrid;
    use crate::shared::{shared, shared_mut};
    use std::cell::{Cell, RefCell};

    /// A single-state test lattice: each backward step scales the asset values
    /// by a constant `discount`, threading values through the asset (never
    /// short-circuiting) so rollback delegation is genuinely exercised. It logs
    /// each step so a test can observe the underlying being rolled before an
    /// exercise. The real tree lattice lands in a follow-up ticket.
    struct FlatLattice {
        grid: TimeGrid,
        discount: Real,
        log: RefCell<Vec<Time>>,
    }

    impl FlatLattice {
        fn new(end: Time, steps: Size, discount: Real) -> Self {
            FlatLattice {
                grid: TimeGrid::new(end, steps).unwrap(),
                discount,
                log: RefCell::new(Vec::new()),
            }
        }

        fn index_of(&self, t: Time) -> Size {
            let times = self.grid.times();
            let mut best = 0;
            for i in 1..times.len() {
                if (times[i] - t).abs() < (times[best] - t).abs() {
                    best = i;
                }
            }
            best
        }
    }

    impl Lattice for FlatLattice {
        fn time_grid(&self) -> &TimeGrid {
            &self.grid
        }

        fn initialize(&self, asset: &mut dyn DiscretizedAsset, time: Time) -> QlResult<()> {
            asset.set_time(time);
            asset.reset(1)
        }

        fn partial_rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
            let target = self.index_of(to);
            let mut i = self.index_of(asset.time());
            while i > target {
                i -= 1;
                asset.set_time(self.grid[i]);
                for v in asset.values_mut().iter_mut() {
                    *v *= self.discount;
                }
                self.log.borrow_mut().push(self.grid[i]);
                if i > target {
                    asset.adjust_values()?;
                }
            }
            Ok(())
        }

        fn rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
            self.partial_rollback(asset, to)?;
            asset.adjust_values()
        }

        fn present_value(&self, asset: &mut dyn DiscretizedAsset) -> QlResult<Real> {
            self.rollback(asset, self.grid[0])?;
            Ok(asset.values()[0])
        }

        fn grid(&self, _time: Time) -> QlResult<Array> {
            Ok(Array::filled(1, 0.0))
        }
    }

    /// Counts how often `pre_adjust_values_impl` actually fires, to pin the
    /// `close_enough` dedup guard.
    #[derive(Default)]
    struct CountingAsset {
        base: DiscretizedAssetBase,
        pre_calls: Cell<u32>,
    }

    impl DiscretizedAsset for CountingAsset {
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
            self.base.values = Array::filled(size, 0.0);
            Ok(())
        }
        fn mandatory_times(&self) -> Vec<Time> {
            Vec::new()
        }
        fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
            self.pre_calls.set(self.pre_calls.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn discount_bond_resets_to_ones() {
        // discretizedasset.hpp:150: reset(size) -> Array(size, 1.0).
        let mut bond = DiscretizedDiscountBond::new();
        bond.reset(3).unwrap();
        assert_eq!(bond.values().to_vec(), vec![1.0, 1.0, 1.0]);
        assert!(bond.mandatory_times().is_empty());
    }

    #[test]
    fn discount_bond_present_value_is_product_of_step_discounts() {
        // Oracle: a bond worth 1 at T=1.0, rolled back over 4 steps at d=0.9,
        // is worth 0.9^4. Exercises rollback delegation through the lattice.
        let lattice: Shared<dyn Lattice> = shared(FlatLattice::new(1.0, 4, 0.9));
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(Shared::clone(&lattice), 1.0).unwrap();
        assert_eq!(bond.values().to_vec(), vec![1.0]);
        let pv = bond.present_value().unwrap();
        assert!((pv - 0.9_f64.powi(4)).abs() < 1e-12, "pv = {pv}");
    }

    #[test]
    fn pre_adjust_guard_fires_impl_once_per_time() {
        // discretizedasset.hpp:200: the close_enough dedup fires the impl once
        // per distinct time_, and a new time_ re-arms it.
        let mut asset = CountingAsset::default();
        asset.pre_adjust_values().unwrap();
        asset.pre_adjust_values().unwrap();
        assert_eq!(asset.pre_calls.get(), 1, "same time must dedup");
        asset.set_time(1.0);
        asset.pre_adjust_values().unwrap();
        assert_eq!(asset.pre_calls.get(), 2, "new time re-arms");
    }

    #[test]
    fn is_on_time_matches_grid_node() {
        // discretizedasset.hpp:211: true iff the asset sits on t's grid node.
        let lattice: Shared<dyn Lattice> = shared(FlatLattice::new(1.0, 4, 0.9));
        let mut asset = CountingAsset::default();
        asset.initialize(Shared::clone(&lattice), 1.0).unwrap();
        asset.set_time(0.5);
        assert!(asset.is_on_time(0.5));
        assert!(!asset.is_on_time(0.75));
    }

    #[test]
    fn apply_exercise_condition_is_elementwise_max() {
        // discretizedasset.hpp:225: values[i] = max(underlying[i], values[i]).
        // Both directions live in one array: idx0 continuation wins, idx1
        // underlying wins, idx2 ties - so a min() stub is detectable.
        let bond = shared_mut(DiscretizedDiscountBond::new());
        *bond.borrow_mut().values_mut() = Array::from([1.0, 5.0, 2.0]);
        let underlying: SharedMut<dyn DiscretizedAsset> = bond;
        let mut option = DiscretizedOption::new(
            SharedMut::clone(&underlying),
            ExerciseType::Bermudan,
            vec![],
        );
        *option.values_mut() = Array::from([3.0, 4.0, 2.0]);
        option.apply_exercise_condition();
        assert_eq!(option.values().to_vec(), vec![3.0, 5.0, 2.0]);
    }

    #[test]
    fn option_post_adjust_rolls_underlying_back_before_exercising() {
        // Bermudan exercise at the on-grid time 0.5: the continuation value is 0
        // but the underlying, rolled back to 0.5 first, is worth 0.9^2, so the
        // exercise condition lifts the option to the underlying value.
        let lattice: Shared<dyn Lattice> = shared(FlatLattice::new(1.0, 4, 0.9));
        let bond = shared_mut(DiscretizedDiscountBond::new());
        bond.borrow_mut()
            .initialize(Shared::clone(&lattice), 1.0)
            .unwrap();
        let underlying: SharedMut<dyn DiscretizedAsset> = bond;
        let mut option = DiscretizedOption::new(
            SharedMut::clone(&underlying),
            ExerciseType::Bermudan,
            vec![0.5],
        );
        option.initialize(Shared::clone(&lattice), 1.0).unwrap();
        assert_eq!(option.values().to_vec(), vec![0.0]);

        option.rollback(0.5).unwrap();

        let expected = 0.9_f64.powi(2);
        assert!(
            (option.values()[0] - expected).abs() < 1e-12,
            "option = {}",
            option.values()[0]
        );
        assert!((underlying.borrow().time() - 0.5).abs() < 1e-12);
        assert!((underlying.borrow().values()[0] - expected).abs() < 1e-12);
    }
}
