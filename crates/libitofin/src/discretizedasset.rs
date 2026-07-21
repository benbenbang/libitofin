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
use crate::math::array::Array;
use crate::math::comparison::close_enough;
use crate::methods::lattices::lattice::Lattice;
use crate::shared::Shared;
use crate::types::{Real, Size, Time};

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
