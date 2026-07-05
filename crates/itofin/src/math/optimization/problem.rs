//! Constrained optimization problem.
//!
//! Port of `ql/math/optimization/problem.hpp`. The C++ class stores the cost
//! function and constraint by reference with a lifetime warning in the docs;
//! here the borrows make that contract explicit.

use crate::math::array::Array;
use crate::math::optimization::constraint::Constraint;
use crate::math::optimization::costfunction::CostFunction;
use crate::types::{Integer, Real};
use crate::utilities::null::Null;

/// A cost function to minimize over a constrained region, tracking the
/// current candidate minimum and evaluation counts.
pub struct Problem<'a> {
    cost_function: &'a dyn CostFunction,
    constraint: &'a dyn Constraint,
    current_value: Array,
    function_value: Real,
    squared_norm: Real,
    function_evaluation: Integer,
    gradient_evaluation: Integer,
}

impl<'a> Problem<'a> {
    /// A problem minimizing `cost_function` over `constraint`, starting from
    /// `initial_value`.
    pub fn new(
        cost_function: &'a dyn CostFunction,
        constraint: &'a dyn Constraint,
        initial_value: Array,
    ) -> Self {
        Problem {
            cost_function,
            constraint,
            current_value: initial_value,
            function_value: Real::null(),
            squared_norm: Real::null(),
            function_evaluation: 0,
            gradient_evaluation: 0,
        }
    }

    /// Resets the evaluation counters and stored function/gradient values;
    /// it does not reset the current minimum to any initial value.
    pub fn reset(&mut self) {
        self.function_evaluation = 0;
        self.gradient_evaluation = 0;
        self.function_value = Real::null();
        self.squared_norm = Real::null();
    }

    /// Computes the scalar cost at `x` and increments the evaluation counter.
    pub fn value(&mut self, x: &Array) -> Real {
        self.function_evaluation += 1;
        self.cost_function.value(x)
    }

    /// Computes the cost values at `x` and increments the evaluation counter.
    pub fn values(&mut self, x: &Array) -> Array {
        self.function_evaluation += 1;
        self.cost_function.values(x)
    }

    /// Computes the cost gradient at `x` and increments the gradient counter.
    pub fn gradient(&mut self, grad: &mut Array, x: &Array) {
        self.gradient_evaluation += 1;
        self.cost_function.gradient(grad, x);
    }

    /// Computes cost and gradient at `x`, incrementing both counters.
    pub fn value_and_gradient(&mut self, grad: &mut Array, x: &Array) -> Real {
        self.function_evaluation += 1;
        self.gradient_evaluation += 1;
        self.cost_function.value_and_gradient(grad, x)
    }

    /// The constraint.
    pub fn constraint(&self) -> &dyn Constraint {
        self.constraint
    }

    /// The cost function.
    pub fn cost_function(&self) -> &dyn CostFunction {
        self.cost_function
    }

    /// Sets the current candidate minimum.
    pub fn set_current_value(&mut self, current_value: Array) {
        self.current_value = current_value;
    }

    /// The current candidate minimum.
    pub fn current_value(&self) -> &Array {
        &self.current_value
    }

    /// Sets the cost function value at the current candidate.
    pub fn set_function_value(&mut self, function_value: Real) {
        self.function_value = function_value;
    }

    /// The cost function value at the current candidate.
    pub fn function_value(&self) -> Real {
        self.function_value
    }

    /// Sets the squared norm of the gradient at the current candidate.
    pub fn set_gradient_norm_value(&mut self, squared_norm: Real) {
        self.squared_norm = squared_norm;
    }

    /// The squared norm of the gradient at the current candidate.
    pub fn gradient_norm_value(&self) -> Real {
        self.squared_norm
    }

    /// The number of cost function evaluations.
    pub fn function_evaluation(&self) -> Integer {
        self.function_evaluation
    }

    /// The number of cost function gradient evaluations.
    pub fn gradient_evaluation(&self) -> Integer {
        self.gradient_evaluation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::optimization::constraint::NoConstraint;

    struct Square;

    impl CostFunction for Square {
        fn values(&self, x: &Array) -> Array {
            Array::filled(1, x[0] * x[0])
        }
    }

    #[test]
    fn evaluations_are_counted_and_reset() {
        let cost = Square;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([3.0]));

        assert_eq!(problem.value(&Array::from([3.0])), 9.0);
        assert_eq!(problem.values(&Array::from([2.0])), Array::from([4.0]));
        let mut grad = Array::with_size(1);
        problem.gradient(&mut grad, &Array::from([3.0]));
        assert!((grad[0] - 6.0).abs() < 1e-6);
        problem.value_and_gradient(&mut grad, &Array::from([3.0]));
        assert_eq!(problem.function_evaluation(), 3);
        assert_eq!(problem.gradient_evaluation(), 2);

        problem.reset();
        assert_eq!(problem.function_evaluation(), 0);
        assert_eq!(problem.gradient_evaluation(), 0);
        assert!(problem.function_value().is_null());
        assert!(problem.gradient_norm_value().is_null());
    }

    #[test]
    fn stores_candidate_minimum_state() {
        let cost = Square;
        let constraint = NoConstraint;
        let mut problem = Problem::new(&cost, &constraint, Array::from([1.0]));

        assert_eq!(problem.current_value(), &Array::from([1.0]));
        problem.set_current_value(Array::from([0.5]));
        problem.set_function_value(0.25);
        problem.set_gradient_norm_value(1.0);
        assert_eq!(problem.current_value(), &Array::from([0.5]));
        assert_eq!(problem.function_value(), 0.25);
        assert_eq!(problem.gradient_norm_value(), 1.0);
        assert!(problem.constraint().test(problem.current_value()));
    }
}
