//! A [`Constraint`] lifted onto the free subspace of a [`Projection`].
//!
//! Port of `ql/math/optimization/projectedconstraint.hpp`. `ProjectedConstraint`
//! wraps a base constraint and a projection so the optimizer can test and bound
//! the free parameters alone: each operation first [`Projection::include`]s the
//! free vector into the full parameter set before delegating to the base
//! constraint. The bounds additionally [`Projection::project`] the base result
//! back onto the free subspace, mirroring `projectedconstraint.hpp:49,52`.
//!
//! The base constraint is stored as `Box<dyn Constraint>` rather than a generic
//! parameter so that `CalibratedModel::calibrate` can hand it either a private
//! or a composite constraint chosen at run time.

use crate::math::array::Array;
use crate::math::optimization::constraint::Constraint;
use crate::math::optimization::projection::Projection;

/// A base constraint evaluated over the free parameters of a projection.
pub struct ProjectedConstraint {
    constraint: Box<dyn Constraint>,
    projection: Projection,
}

impl ProjectedConstraint {
    /// A constraint that applies `constraint` to the full parameter vector
    /// reconstructed from the free parameters via `projection`.
    pub fn new(constraint: Box<dyn Constraint>, projection: Projection) -> Self {
        ProjectedConstraint {
            constraint,
            projection,
        }
    }
}

impl Constraint for ProjectedConstraint {
    fn test(&self, params: &Array) -> bool {
        self.constraint.test(&self.projection.include(params))
    }

    fn upper_bound(&self, params: &Array) -> Array {
        self.projection.project(
            &self
                .constraint
                .upper_bound(&self.projection.include(params)),
        )
    }

    fn lower_bound(&self, params: &Array) -> Array {
        self.projection.project(
            &self
                .constraint
                .lower_bound(&self.projection.include(params)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::optimization::constraint::BoundaryConstraint;
    use crate::types::Real;

    struct SecondSlotBelow {
        threshold: Real,
    }

    impl Constraint for SecondSlotBelow {
        fn test(&self, params: &Array) -> bool {
            params[1] < self.threshold
        }
    }

    #[test]
    fn test_delegates_to_base_on_the_reinstated_full_vector() {
        let projection =
            Projection::new(&Array::from([0.0, 5.0, 0.0]), vec![false, true, false]).unwrap();
        let base = Box::new(SecondSlotBelow { threshold: 10.0 });
        let pc = ProjectedConstraint::new(base, projection);
        assert!(pc.test(&Array::from([1.0, 2.0])));

        let rejecting =
            Projection::new(&Array::from([0.0, 20.0, 0.0]), vec![false, true, false]).unwrap();
        let pc_reject =
            ProjectedConstraint::new(Box::new(SecondSlotBelow { threshold: 10.0 }), rejecting);
        assert!(!pc_reject.test(&Array::from([1.0, 2.0])));
    }

    #[test]
    fn bounds_include_the_input_then_project_the_base_result() {
        let projection =
            Projection::new(&Array::from([0.0, 5.0, 0.0]), vec![false, true, false]).unwrap();
        let base = Box::new(BoundaryConstraint::new(-1.0, 2.0));
        let pc = ProjectedConstraint::new(base, projection);
        let free = Array::from([0.5, 0.5]);
        assert_eq!(pc.upper_bound(&free), Array::from([2.0, 2.0]));
        assert_eq!(pc.lower_bound(&free), Array::from([-1.0, -1.0]));
    }
}
