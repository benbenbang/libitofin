//! The split-operator contract an operator-splitting scheme steps.
//!
//! Port of `ql/methods/finitedifferences/operators/fdmlinearopcomposite.hpp:35`.
//!
//! `toMatrixDecomp` (`:48`) and the `toMatrix` it accumulates (`:52`) are
//! omitted with the rest of the sparse-matrix work in #636, rather than
//! accepted and left failing: both return `SparseMatrix`, which is not ported.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::types::{Real, Size, Time};

use super::fdmlinearop::FdmLinearOp;

/// An [`FdmLinearOp`] that also exposes the pieces of its own splitting, so a
/// scheme can treat one direction implicitly while the others stay explicit.
///
/// The whole operator is time dependent: [`set_time`](Self::set_time) rebuilds
/// it for the step about to be taken, and every other method reads the
/// operator that call left behind.
pub trait FdmLinearOpComposite: FdmLinearOp {
    /// The number of directions the operator splits into
    /// (`fdmlinearopcomposite.hpp:37`).
    ///
    /// This counts the pieces of the splitting, not the grid points - the grid
    /// is as large as the mesher's layout, whatever this returns.
    fn size(&self) -> Size;

    /// Rebuilds the operator for the step from `t1` to `t2`, which are
    /// required to satisfy `t1 <= t2` (`fdmlinearopcomposite.hpp:40`).
    ///
    /// C++ returns `void`. Implementations here read term structures, which
    /// report their failures as errors (D4), so the result is carried out
    /// rather than swallowed.
    fn set_time(&mut self, t1: Time, t2: Time) -> QlResult<()>;

    /// Applies the part of the operator that no single direction owns - the
    /// mixed derivatives (`fdmlinearopcomposite.hpp:42`).
    fn apply_mixed(&self, r: &Array) -> Array;

    /// Applies the part of the operator belonging to `direction`
    /// (`fdmlinearopcomposite.hpp:44`).
    fn apply_direction(&self, direction: Size, r: &Array) -> Array;

    /// Solves the implicit step along `direction`, that is
    /// `(I + s * A_direction) x = r` for `x` (`fdmlinearopcomposite.hpp:45`).
    fn solve_splitting(&self, direction: Size, r: &Array, s: Real) -> QlResult<Array>;

    /// Applies the preconditioner an iterative scheme solves against
    /// (`fdmlinearopcomposite.hpp:46`).
    fn preconditioner(&self, r: &Array, s: Real) -> QlResult<Array>;
}
