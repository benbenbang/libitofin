//! The N-D mesher contract.
//!
//! Port of `ql/methods/finitedifferences/meshers/fdmmesher.hpp:37`.

use crate::math::array::Array;
use crate::methods::finitedifferences::operators::{FdmLinearOpIterator, FdmLinearOpLayout};
use crate::shared::Shared;
use crate::types::{Real, Size};

/// The geometry of a finite-difference grid: where its points sit and how far
/// apart they are.
///
/// C++ makes this an abstract class that *holds* the layout (`layout_`,
/// `fdmmesher.hpp:56`) and hands it out through `layout()`. A Rust trait
/// carries no data, so each implementor owns its own
/// [`Shared<FdmLinearOpLayout>`] and surfaces it through
/// [`layout`](Self::layout); the accessor keeps the `Shared` so an operator can
/// clone the handle rather than the layout.
pub trait FdmMesher {
    /// The index space this mesher is defined over.
    fn layout(&self) -> &Shared<FdmLinearOpLayout>;

    /// The distance from `iter` to its neighbour one step up along `direction`.
    fn dplus(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real;

    /// The distance from `iter` to its neighbour one step down along
    /// `direction`.
    fn dminus(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real;

    /// The coordinate of `iter` along `direction`.
    fn location(&self, iter: &FdmLinearOpIterator, direction: Size) -> Real;

    /// The `direction` coordinate of every grid point, indexed the way the
    /// layout indexes its points: the result has
    /// [`layout().size()`](FdmLinearOpLayout::size) entries, not
    /// `dim()[direction]`.
    fn locations(&self, direction: Size) -> Array;
}
