//! The set of boundary conditions a finite-difference scheme applies.
//!
//! Port of `ql/methods/finitedifferences/utilities/fdmboundaryconditionset.hpp:32`.
//!
//! The alias is what `OperatorTraits<FdmLinearOp>::bc_set`
//! (`operatortraits.hpp:38`) expands to. The rest of that C++ type bundle
//! collapses to concrete Rust types and needs no aliases of its own:
//! `operator_type` is [`FdmLinearOp`], `array_type` is [`Array`], and
//! `condition_type` is [`StepCondition`], which already fixes the array type.
//!
//! [`FdmLinearOp`]: crate::methods::finitedifferences::operators::FdmLinearOp
//! [`Array`]: crate::math::array::Array
//! [`StepCondition`]: crate::methods::finitedifferences::StepCondition

use crate::methods::finitedifferences::BoundaryCondition;
use crate::shared::Shared;

/// The boundary conditions applied over one step, in order.
///
/// Empty on the plain European path, where the grid needs no boundary
/// treatment.
pub type FdmBoundaryConditionSet = Vec<Shared<dyn BoundaryCondition>>;
