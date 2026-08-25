//! Piecewise-bootstrapped year-on-year optionlet volatility curve.
//!
//! Port of `ql/experimental/inflation/piecewiseyoyoptionletvolatility.hpp`:
//! [`YoYInflationVolatilityTraits`] (`hpp:36-96`) drives the bootstrap's seed,
//! guesses and brackets, and [`PiecewiseYoYOptionletVolatilityCurve`]
//! (`hpp:105-175`) is the curve, one per strike of the stripper, fitted to
//! [`YoYOptionletVolatilityHelper`]s by the same [`IterativeBootstrap`] every
//! other piecewise curve runs. "We use a flat smile for bootstrapping at
//! constant K" (`hpp:100-103`): the smile is flat, the strike bounds a thin
//! band around the quoted K.
//!
//! It mirrors [`PiecewiseYoYInflationCurve`] field for field; C++ derives from
//! `InterpolatedYoYOptionletVolatilityCurve<Interpolator>` *and* `LazyObject`
//! (`hpp:108-110`), and Rust has no inheritance, so the node storage lives
//! here and the volatility lookup reads it directly.
//!
//! ## Node zero carries the base level
//!
//! `YoYInflationVolatilityTraits::initialDate` is the curve's own base date
//! and `initialValue` its own base level (`hpp:41-51` - "REALLLYYYY important
//! because generally don't have a clue what this should be"), both read off
//! the curve being bootstrapped. They land here as the
//! [`PiecewiseCurve::initial_date`]/[`PiecewiseCurve::initial_value`] hook
//! overrides, the same seam [`PiecewiseYoYInflationCurve`] uses for its base
//! rate; `updateGuess` writes only node `i` (`hpp:89-93`), so the seeded level
//! survives the whole solve.
//!
//! ## Divergences from QuantLib
//!
//! - The moving reference date takes the shared [`Settings`] handle (D5).
//! - The `accuracy` constructor argument (`hpp:133`) is not exposed, matching
//!   the sibling curves; the field carries the C++ default `1.0e-12`.
//! - Only [`Linear`] is constructible, the impls staying generic, exactly as
//!   on [`PiecewiseYoYInflationCurve`].
//!
//! [`PiecewiseYoYInflationCurve`]: crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve
//! [`IterativeBootstrap`]: crate::termstructures::iterativebootstrap::IterativeBootstrap
//! [`PiecewiseCurve::initial_date`]: crate::termstructures::iterativebootstrap::PiecewiseCurve::initial_date
//! [`PiecewiseCurve::initial_value`]: crate::termstructures::iterativebootstrap::PiecewiseCurve::initial_value

use crate::termstructures::bootstraptraits::BootstrapTraits;
use crate::types::{Real, Size, Time};

/// Traits for the inflation-volatility bootstrap
/// (`YoYInflationVolatilityTraits`, `hpp:36-96`).
pub struct YoYInflationVolatilityTraits;

impl BootstrapTraits for YoYInflationVolatilityTraits {
    /// C++ has no constant here: `initialValue` reads the bootstrapped curve's
    /// own `baseLevel()` (`hpp:45-51`), which lands as
    /// [`PiecewiseYoYOptionletVolatilityCurve`]'s `initial_value` hook
    /// override, so this static is never the seeding path. A zero volatility
    /// stands in for the total function the trait requires.
    fn initial_value() -> Real {
        0.0
    }

    /// The per-node guess (`hpp:54-68`): the stored node on a seeded pass,
    /// `0.005` at the first pillar, `0.002` after it.
    fn guess(i: Size, _times: &[Time], data: &[Real], valid_data: bool) -> Real {
        if valid_data {
            return data[i];
        }
        if i == 1 {
            return 0.005;
        }
        0.002
    }

    /// The lower bracket (`hpp:71-78`): two vol points under the previous
    /// node, floored at zero - "vol cannot be negative".
    fn min_value_after(i: Size, _times: &[Time], data: &[Real], _valid_data: bool) -> Real {
        (data[i - 1] - 0.02).max(0.0)
    }

    /// The upper bracket (`hpp:79-86`): two vol points over the previous node.
    fn max_value_after(i: Size, _times: &[Time], data: &[Real], _valid_data: bool) -> Real {
        data[i - 1] + 0.02
    }

    /// Writes a solved level back (`hpp:89-93`): node `i` takes it and nothing
    /// else moves, so the seeded base level is never overwritten.
    fn update_guess(data: &mut [Real], value: Real, i: Size) {
        data[i] = value;
    }

    /// The convergence-loop cap (`hpp:95`).
    fn max_iterations() -> Size {
        25
    }
}
