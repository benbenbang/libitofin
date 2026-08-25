//! Year-on-year inflation optionlet stripping.
//!
//! Port of `ql/experimental/inflation/yoyoptionletstripper.hpp` (the
//! [`YoYOptionletStripper`] interface, `hpp:37-60`) and
//! `interpolatedyoyoptionletstripper.hpp` (the interpolated implementation,
//! `hpp:43-298`): from a [`YoYCapFloorTermPriceSurface`] of quoted cap/floor
//! prices, strip one [`PiecewiseYoYOptionletVolatilityCurve`] per strike, so
//! that [`slice`](YoYOptionletStripper::slice) can answer the K-profile of
//! optionlet volatilities at any date.
//!
//! ## The engine repointing divergence
//!
//! C++ `initialize` takes the shared `YoYInflationCapFloorEngine` and the
//! solver's objective calls `p_->setVolatility(hCurve)` on *every* evaluation
//! (`interpolatedyoyoptionletstripper.hpp:154-155`), overwriting the engine's
//! handle member. The Rust engine's handle is immutable, so the caller hands
//! `initialize` the *retained* [`RelinkableHandle`] the engine was built over,
//! and the objective re-links it to each freshly built
//! [`InterpolatedYoYOptionletVolatilityCurve`] instead - which notifies the
//! engine and, through it, invalidates the cap/floor being repriced. The
//! bootstrap phase then shares the same link through each
//! [`YoYOptionletHelper`], whose `set_term_structure` re-points it at the
//! curve under construction.
//!
//! ## Fidelity pins (ported faithfully, not fixed)
//!
//! - The objective's volatility curves are built with
//!   `indexIsInterpolated = false`: the C++ `ObjectiveFunction` declares the
//!   member with that default and no constructor ever assigns it (`hpp:81`,
//!   `:97-136`), even though the surface's own flag feeds everything else.
//! - The `fixingDays` the C++ `ObjectiveFunction` takes (`hpp:71`, `:102`) is
//!   never stored or read; the port's objective simply takes none. The
//!   *helpers* do read the surface's fixing days (`hpp:239`), and so do they
//!   here.
//! - Two lags travel separately: the `lag` parameter goes to
//!   `MakeYoYInflationCapFloor` (`hpp:116`) while the member `lag_` is
//!   overwritten from `surf_->observationLag()` (`hpp:112`) and feeds the vol
//!   curves. `initialize` passes its own `lag_` - also the surface's
//!   observation lag - so the two coincide in every reachable call, and the
//!   port keeps both locals.
//! - The objective's curve hardcodes `TARGET`, `ModifiedFollowing` and
//!   `Actual/365 (Fixed)` (`hpp:149-150`), not the surface's conventions; and
//!   each helper is handed a throwaway flat surface at the solved volatility
//!   (`hpp:243-254`) that the bootstrap immediately replaces.
//!
//! [`YoYCapFloorTermPriceSurface`]: crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurface

use crate::errors::QlResult;
use crate::handle::RelinkableHandle;
use crate::pricingengines::inflation::YoYInflationCapFloorEngine;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::inflation::yoycapfloortermpricesurface::YoYCapFloorTermPriceSurface;
use crate::time::date::Date;
use crate::types::{Rate, Real, Volatility};

use super::YoYOptionletVolatilitySurface;

/// Interface for inflation cap stripping from price surfaces
/// (`YoYOptionletStripper`, `yoyoptionletstripper.hpp:37-60`). Strippers
/// return K slices of the volatility surface at a given T; `initialize` does
/// the actual stripping along each K.
pub trait YoYOptionletStripper {
    /// Strips `surface` with `pricer`, whose volatility must be read through
    /// the retained `vol_handle` (see the module docs); `slope` is the assumed
    /// proportional change of the unobserved initial caplet volatility.
    ///
    /// # Errors
    ///
    /// A strike whose initial-point solve fails (C++'s `QL_FAIL` wrap,
    /// `interpolatedyoyoptionletstripper.hpp:214-216`), an unbuildable
    /// instrument or helper, or a failed per-strike bootstrap.
    fn initialize(
        &self,
        surface: &Shared<dyn YoYCapFloorTermPriceSurface>,
        pricer: &SharedMut<YoYInflationCapFloorEngine>,
        vol_handle: &RelinkableHandle<dyn YoYOptionletVolatilitySurface>,
        slope: Real,
    ) -> QlResult<()>;

    /// The lowest quoted strike (`minStrike`).
    ///
    /// # Errors
    ///
    /// Before `initialize` has run, where C++ dereferences null.
    fn min_strike(&self) -> QlResult<Rate>;

    /// The highest quoted strike (`maxStrike`).
    ///
    /// # Errors
    ///
    /// As [`min_strike`](Self::min_strike).
    fn max_strike(&self) -> QlResult<Rate>;

    /// The quoted strike union (`strikes`).
    ///
    /// # Errors
    ///
    /// As [`min_strike`](Self::min_strike).
    fn strikes(&self) -> QlResult<Vec<Rate>>;

    /// The (strikes, volatilities) profile at `d` (`slice`), one entry per
    /// quoted strike, each read off that strike's stripped curve.
    ///
    /// # Errors
    ///
    /// As [`min_strike`](Self::min_strike), plus a date the stripped curves
    /// refuse.
    fn slice(&self, d: Date) -> QlResult<(Vec<Rate>, Vec<Volatility>)>;
}
