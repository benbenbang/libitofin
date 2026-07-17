//! Constraints on optimization parameters.
//!
//! Port of `ql/math/optimization/constraint.{hpp,cpp}`. QuantLib's `Constraint`
//! is a pimpl handle over a virtual `Impl` hierarchy; here the hierarchy
//! collapses into the [`Constraint`] trait, with the handle's `update` logic
//! and the default (unbounded) bounds as provided methods. Implementations
//! must return bound arrays of the same size as `params`; where QuantLib
//! checks that on the handle, here [`CompositeConstraint`] asserts it before
//! combining its children's bounds.

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::require;
use crate::types::Real;

/// A feasible region for optimization parameters.
pub trait Constraint {
    /// Tests if `params` satisfy the constraint.
    fn test(&self, params: &Array) -> bool;

    /// The upper bound for the given parameters.
    fn upper_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), Real::MAX)
    }

    /// The lower bound for the given parameters.
    fn lower_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), -Real::MAX)
    }

    /// Moves `params` along `direction` by a step of at most `beta`, halving
    /// the step until the new point is feasible, and returns the step taken.
    ///
    /// # Errors
    ///
    /// Fails if no feasible step is found after 200 halvings.
    fn update(&self, params: &mut Array, direction: &Array, beta: Real) -> QlResult<Real> {
        let mut diff = beta;
        let mut new_params = &*params + &(direction * diff);
        let mut icount = 0;
        while !self.test(&new_params) {
            require!(icount <= 200, "can't update parameter vector");
            diff *= 0.5;
            icount += 1;
            new_params = &*params + &(direction * diff);
        }
        *params = new_params;
        Ok(diff)
    }
}

/// No constraint: every point is feasible.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoConstraint;

impl Constraint for NoConstraint {
    fn test(&self, _params: &Array) -> bool {
        true
    }
}

/// Constraint imposing positivity to all arguments.
#[derive(Clone, Copy, Debug, Default)]
pub struct PositiveConstraint;

impl Constraint for PositiveConstraint {
    fn test(&self, params: &Array) -> bool {
        params.iter().all(|&p| p > 0.0)
    }

    fn lower_bound(&self, params: &Array) -> Array {
        Array::with_size(params.size())
    }
}

/// Constraint imposing all arguments to be in `[low, high]`.
#[derive(Clone, Copy, Debug)]
pub struct BoundaryConstraint {
    low: Real,
    high: Real,
}

impl BoundaryConstraint {
    /// A constraint keeping every argument in `[low, high]`.
    pub fn new(low: Real, high: Real) -> Self {
        BoundaryConstraint { low, high }
    }
}

impl Constraint for BoundaryConstraint {
    fn test(&self, params: &Array) -> bool {
        params.iter().all(|&p| self.low <= p && p <= self.high)
    }

    fn upper_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), self.high)
    }

    fn lower_bound(&self, params: &Array) -> Array {
        Array::filled(params.size(), self.low)
    }
}

/// Constraint enforcing both given sub-constraints.
#[derive(Clone, Copy, Debug)]
pub struct CompositeConstraint<C1, C2> {
    c1: C1,
    c2: C2,
}

impl<C1: Constraint, C2: Constraint> CompositeConstraint<C1, C2> {
    /// The intersection of the feasible regions of `c1` and `c2`.
    pub fn new(c1: C1, c2: C2) -> Self {
        CompositeConstraint { c1, c2 }
    }
}

impl<C1: Constraint, C2: Constraint> Constraint for CompositeConstraint<C1, C2> {
    fn test(&self, params: &Array) -> bool {
        self.c1.test(params) && self.c2.test(params)
    }

    fn upper_bound(&self, params: &Array) -> Array {
        let c1ub = self.c1.upper_bound(params);
        let c2ub = self.c2.upper_bound(params);
        assert_bound_size(c1ub.size(), params.size(), "upper");
        assert_bound_size(c2ub.size(), params.size(), "upper");
        c1ub.iter().zip(&*c2ub).map(|(a, b)| a.min(*b)).collect()
    }

    fn lower_bound(&self, params: &Array) -> Array {
        let c1lb = self.c1.lower_bound(params);
        let c2lb = self.c2.lower_bound(params);
        assert_bound_size(c1lb.size(), params.size(), "lower");
        assert_bound_size(c2lb.size(), params.size(), "lower");
        c1lb.iter().zip(&*c2lb).map(|(a, b)| a.max(*b)).collect()
    }
}

impl Constraint for Box<dyn Constraint> {
    fn test(&self, params: &Array) -> bool {
        (**self).test(params)
    }

    fn upper_bound(&self, params: &Array) -> Array {
        (**self).upper_bound(params)
    }

    fn lower_bound(&self, params: &Array) -> Array {
        (**self).lower_bound(params)
    }

    fn update(&self, params: &mut Array, direction: &Array, beta: Real) -> QlResult<Real> {
        (**self).update(params, direction, beta)
    }
}

fn assert_bound_size(bound: usize, params: usize, which: &str) {
    assert_eq!(
        bound, params,
        "{which} bound size ({bound}) not equal to params size ({params})"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_constraint_accepts_everything() {
        let c = NoConstraint;
        assert!(c.test(&Array::from([-1.0, 0.0, 1.0e30])));
        assert_eq!(
            c.upper_bound(&Array::with_size(2)),
            Array::filled(2, Real::MAX)
        );
        assert_eq!(
            c.lower_bound(&Array::with_size(2)),
            Array::filled(2, -Real::MAX)
        );
    }

    #[test]
    fn positive_constraint_requires_strict_positivity() {
        let c = PositiveConstraint;
        assert!(c.test(&Array::from([1.0, 2.0])));
        assert!(!c.test(&Array::from([1.0, 0.0])));
        assert!(!c.test(&Array::from([1.0, -1.0])));
        assert_eq!(c.lower_bound(&Array::with_size(3)), Array::filled(3, 0.0));
    }

    #[test]
    fn boundary_constraint_bounds_all_arguments() {
        let c = BoundaryConstraint::new(-1.0, 2.0);
        assert!(c.test(&Array::from([-1.0, 0.0, 2.0])));
        assert!(!c.test(&Array::from([-1.5])));
        assert!(!c.test(&Array::from([2.5])));
        assert_eq!(c.upper_bound(&Array::with_size(2)), Array::filled(2, 2.0));
        assert_eq!(c.lower_bound(&Array::with_size(2)), Array::filled(2, -1.0));
    }

    #[test]
    fn composite_constraint_intersects_regions_and_bounds() {
        let c = CompositeConstraint::new(PositiveConstraint, BoundaryConstraint::new(-1.0, 2.0));
        assert!(c.test(&Array::from([0.5, 1.0])));
        assert!(!c.test(&Array::from([-0.5])));
        assert!(!c.test(&Array::from([2.5])));
        assert_eq!(c.upper_bound(&Array::with_size(2)), Array::filled(2, 2.0));
        assert_eq!(c.lower_bound(&Array::with_size(2)), Array::filled(2, 0.0));
    }

    #[test]
    #[should_panic(expected = "upper bound size (1) not equal to params size (2)")]
    fn composite_constraint_rejects_wrong_sized_child_bounds() {
        struct WrongSize;
        impl Constraint for WrongSize {
            fn test(&self, _params: &Array) -> bool {
                true
            }
            fn upper_bound(&self, _params: &Array) -> Array {
                Array::filled(1, 1.0)
            }
        }
        let c = CompositeConstraint::new(WrongSize, NoConstraint);
        let _ = c.upper_bound(&Array::with_size(2));
    }

    #[test]
    fn boxed_constraint_delegates_through_deref() {
        let c: Box<dyn Constraint> = Box::new(PositiveConstraint);
        assert!(c.test(&Array::from([1.0, 2.0])));
        assert!(!c.test(&Array::from([1.0, 0.0])));
        assert_eq!(c.lower_bound(&Array::with_size(2)), Array::filled(2, 0.0));
        assert_eq!(
            c.upper_bound(&Array::with_size(2)),
            Array::filled(2, Real::MAX)
        );
        let composite = CompositeConstraint::new(
            Box::new(PositiveConstraint) as Box<dyn Constraint>,
            BoundaryConstraint::new(-1.0, 2.0),
        );
        assert!(composite.test(&Array::from([0.5, 1.0])));
        assert!(!composite.test(&Array::from([-0.5])));
    }

    #[test]
    fn update_halves_step_until_feasible() {
        let c = PositiveConstraint;
        let mut params = Array::from([1.0]);
        let direction = Array::from([-1.0]);
        let step = c.update(&mut params, &direction, 1.5).unwrap();
        assert_eq!(step, 0.75);
        assert_eq!(params, Array::from([0.25]));
    }

    #[test]
    fn update_fails_when_no_feasible_step_exists() {
        let c = PositiveConstraint;
        let mut params = Array::from([-1.0]);
        let direction = Array::from([0.0]);
        let err = c.update(&mut params, &direction, 1.0).unwrap_err();
        assert_eq!(err.message(), "can't update parameter vector");
    }
}
