//! The abstract numerical-method (lattice) interface.
//!
//! Port of `ql/numericalmethod.hpp` (the `Lattice` base class). A `Lattice` owns
//! a [`TimeGrid`] and drives a [`DiscretizedAsset`] backward through it. The
//! concrete tree lattice (`TreeLattice`) lands in a follow-up ticket and
//! implements this trait.
//!
//! Divergence from QuantLib, all deliberate:
//! - The C++ methods return `void`/`Real`; here every driver method returns
//!   [`QlResult`] so a lattice can surface a failure (unset state, out-of-range
//!   rollback target) as an error rather than an assertion (D4/D10).
//! - The mutation direction is unchanged: the lattice mutates the asset, so the
//!   asset is passed as `&mut dyn DiscretizedAsset`.

use crate::discretizedasset::DiscretizedAsset;
use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::timegrid::TimeGrid;
use crate::types::{Real, Time};

/// Lattice (tree, finite-differences) base interface (`numericalmethod.hpp:37`).
///
/// Implementors drive a [`DiscretizedAsset`] backward over the owned
/// [`TimeGrid`]. Users are advised to call the corresponding
/// [`DiscretizedAsset`] methods rather than these directly.
pub trait Lattice {
    /// The time grid the lattice rolls over (`numericalmethod.hpp:45`).
    fn time_grid(&self) -> &TimeGrid;

    /// Initialize an asset at the given time (`numericalmethod.hpp:57`).
    fn initialize(&self, asset: &mut dyn DiscretizedAsset, time: Time) -> QlResult<()>;

    /// Roll an asset back to `to`, performing any needed adjustment
    /// (`numericalmethod.hpp:62`).
    fn rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()>;

    /// Roll an asset back to `to` without the final adjustment
    /// (`numericalmethod.hpp:81`).
    fn partial_rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()>;

    /// The present value of an asset (`numericalmethod.hpp:85`).
    fn present_value(&self, asset: &mut dyn DiscretizedAsset) -> QlResult<Real>;

    /// The state grid at the given time (`numericalmethod.hpp:90`).
    fn grid(&self, time: Time) -> QlResult<Array>;
}
