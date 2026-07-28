//! Boundary conditions for finite-difference operators.
//!
//! Port of `ql/methods/finitedifferences/boundarycondition.hpp:35`.
//!
//! The header's `NeumannBC` (`:71`) and `DirichletBC` (`:90`) are not ported:
//! both are `BoundaryCondition<TridiagonalOperator>` classes of the old FD
//! framework and carry an upstream `[[deprecated]]` marker as of QuantLib
//! 1.42. They are dropped, not deferred - the FDM path uses the
//! `BoundaryCondition<FdmLinearOp>` family instead.

use crate::math::array::Array;
use crate::methods::finitedifferences::operators::FdmLinearOp;
use crate::types::Time;

/// The grid edge a boundary condition acts on (`boundarycondition.hpp:41`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundarySide {
    /// No side selected.
    None,
    /// The upper end of the direction.
    Upper,
    /// The lower end of the direction.
    Lower,
}

/// A condition the grid values and the operator must satisfy at a boundary.
///
/// C++ templates this on the operator (`boundarycondition.hpp:34`). Of its two
/// instantiations only `BoundaryCondition<FdmLinearOp>` is live - it is what
/// `OperatorTraits<FdmLinearOp>::bc_set` (`operatortraits.hpp:38`) and every
/// `Fdm*Boundary` use - while `BoundaryCondition<TridiagonalOperator>` belongs
/// to the deprecated pair above. The Rust trait therefore fixes the operator to
/// [`FdmLinearOp`] and takes no type parameter, which also keeps it usable as
/// `dyn BoundaryCondition` in [`FdmBoundaryConditionSet`].
///
/// [`FdmBoundaryConditionSet`]: crate::methods::finitedifferences::utilities::FdmBoundaryConditionSet
///
/// C++ declares `setTime` non-const (`:62`); the Rust method takes `&self`
/// because the set holds each condition as a [`Shared`], which yields no
/// `&mut`. Time-dependent conditions keep their state behind interior
/// mutability, as [`FdmInnerValueCalculator`] already does for its cache.
///
/// [`Shared`]: crate::shared::Shared
/// [`FdmInnerValueCalculator`]: crate::methods::finitedifferences::utilities::FdmInnerValueCalculator
pub trait BoundaryCondition {
    /// Modifies the operator `op` before it is applied, so that the result of
    /// applying it satisfies the condition.
    fn apply_before_applying(&self, op: &mut dyn FdmLinearOp);

    /// Modifies the grid values `a` so that they satisfy the condition.
    fn apply_after_applying(&self, a: &mut Array);

    /// Modifies the operator `op` and the right-hand side `rhs` before the
    /// linear system is solved, so that its solution satisfies the condition.
    fn apply_before_solving(&self, op: &mut dyn FdmLinearOp, rhs: &mut Array);

    /// Modifies the solution `a` so that it satisfies the condition.
    fn apply_after_solving(&self, a: &mut Array);

    /// Sets the current time for time-dependent conditions.
    fn set_time(&self, t: Time);
}
