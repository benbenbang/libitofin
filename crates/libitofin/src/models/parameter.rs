//! Model parameters.
//!
//! Port of `ql/models/parameter.hpp`. A model stores each of its arguments as a
//! [`Parameter`] and reads it back at a time with [`value`](Parameter::value)
//! (C++'s `operator()(Time)`). The C++ base is a value type carrying a shared
//! polymorphic `Impl` that computes the value; its subclasses slice to the base
//! in the model's `arguments_` vector and differ only in that `Impl` plus their
//! constraint. Here the `Impl` collapses to a small [`ParameterImpl`] enum and
//! the subclasses become named constructors ([`ConstantParameter`],
//! [`NullParameter`]) that return a `Parameter`.
//!
//! ## Deferred
//!
//! - `PiecewiseConstantParameter` (`parameter.hpp:119`) and
//!   `TermStructureFittingParameter` (`parameter.hpp:145`) are time-dependent /
//!   tree-fitting only and unused by the closed-form `discountBond` path; both
//!   land with the numerical (tree/lattice) tickets.
//!
//! ## Divergences from QuantLib
//!
//! - The value-validating `ConstantParameter(Real, const Constraint&)`
//!   constructor surfaces its `QL_REQUIRE` as an `Err` on construction (D4): a
//!   value that violates the constraint is user input, not a programming error.
//! - A default (placeholder) parameter carries the [`ParameterImpl::Null`] law,
//!   so reading it returns `0.0`; C++'s default `Parameter` holds a null `Impl`
//!   whose `operator()` would dereference null. Both are only ever placeholders
//!   overwritten before use, so the observable behaviour never diverges.
//! - The `ConstantParameter`/`NullParameter` constructors deliberately return
//!   the base [`Parameter`] (C++'s subclasses slice to it), so
//!   `clippy::new_ret_no_self` is allowed here as it is for the calendar and
//!   interpolation factories.

#![allow(clippy::new_ret_no_self)]

use std::rc::Rc;

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::constraint::{Constraint, NoConstraint};
use crate::require;
use crate::types::{Real, Size, Time};

/// The value law of a [`Parameter`] (C++'s `Parameter::Impl` hierarchy).
#[derive(Clone, Copy)]
enum ParameterImpl {
    /// `ConstantParameter::Impl` (`parameter.hpp:73`): returns `params[0]`.
    Constant,
    /// `NullParameter::Impl` (`parameter.hpp:101`): always `0.0`.
    Null,
}

impl ParameterImpl {
    fn value(self, params: &Array, _t: Time) -> Real {
        match self {
            ParameterImpl::Constant => params[0],
            ParameterImpl::Null => 0.0,
        }
    }
}

/// Base class for model arguments (`parameter.hpp:38`).
#[derive(Clone)]
pub struct Parameter {
    params: Array,
    constraint: Rc<dyn Constraint>,
    impl_: ParameterImpl,
}

impl Parameter {
    /// The stored parameter values.
    pub fn params(&self) -> &Array {
        &self.params
    }

    /// Sets the `i`-th stored value.
    pub fn set_param(&mut self, i: Size, x: Real) {
        self.params[i] = x;
    }

    /// Whether `params` satisfy this parameter's constraint.
    pub fn test_params(&self, params: &Array) -> bool {
        self.constraint.test(params)
    }

    /// The number of stored values.
    pub fn size(&self) -> Size {
        self.params.size()
    }

    /// The parameter's value at time `t` (C++'s `operator()(Time)`).
    pub fn value(&self, t: Time) -> Real {
        self.impl_.value(&self.params, t)
    }

    /// The constraint on this parameter's values.
    pub fn constraint(&self) -> &dyn Constraint {
        &*self.constraint
    }
}

impl Default for Parameter {
    /// The default (placeholder) parameter (`parameter.hpp:48`): no values,
    /// no constraint, always `0.0`. A model's `arguments_` start as these and
    /// the concrete model overwrites them.
    fn default() -> Self {
        Parameter {
            params: Array::new(),
            constraint: Rc::new(NoConstraint),
            impl_: ParameterImpl::Null,
        }
    }
}

/// Standard constant parameter `a(t) = a` (`parameter.hpp:71`).
pub struct ConstantParameter;

impl ConstantParameter {
    /// `ConstantParameter(const Constraint&)` (`parameter.hpp:78`): one slot,
    /// value left at `0.0`.
    pub fn unset(constraint: Rc<dyn Constraint>) -> Parameter {
        Parameter {
            params: Array::with_size(1),
            constraint,
            impl_: ParameterImpl::Constant,
        }
    }

    /// `ConstantParameter(Real, const Constraint&)` (`parameter.hpp:85`): one
    /// slot set to `value`.
    ///
    /// # Errors
    ///
    /// Fails if `value` violates `constraint`
    /// (C++'s `QL_REQUIRE(testParams(params_), ...)`).
    pub fn new(value: Real, constraint: Rc<dyn Constraint>) -> QlResult<Parameter> {
        let mut params = Array::with_size(1);
        params[0] = value;
        let parameter = Parameter {
            params,
            constraint,
            impl_: ParameterImpl::Constant,
        };
        require!(
            parameter.test_params(parameter.params()),
            "{value}: invalid value"
        );
        Ok(parameter)
    }
}

/// Parameter which is always zero `a(t) = 0` (`parameter.hpp:99`).
pub struct NullParameter;

impl NullParameter {
    /// `NullParameter()` (`parameter.hpp:106`): no slots, value always `0.0`.
    pub fn new() -> Parameter {
        Parameter {
            params: Array::new(),
            constraint: Rc::new(NoConstraint),
            impl_: ParameterImpl::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::optimization::constraint::PositiveConstraint;

    #[test]
    fn constant_parameter_returns_its_value_at_any_time() {
        let p = ConstantParameter::new(0.42, Rc::new(NoConstraint)).unwrap();
        assert_eq!(p.size(), 1);
        assert_eq!(p.value(0.0), 0.42);
        assert_eq!(p.value(10.0), 0.42);
        assert_eq!(p.params()[0], 0.42);
    }

    #[test]
    fn constant_parameter_rejects_a_value_the_constraint_forbids() {
        let err = ConstantParameter::new(-1.0, Rc::new(PositiveConstraint))
            .err()
            .unwrap();
        assert_eq!(err.message(), "-1: invalid value");
    }

    #[test]
    fn unset_constant_parameter_defaults_to_zero() {
        let p = ConstantParameter::unset(Rc::new(NoConstraint));
        assert_eq!(p.size(), 1);
        assert_eq!(p.value(0.0), 0.0);
    }

    #[test]
    fn null_parameter_is_always_zero_and_sizeless() {
        let p = NullParameter::new();
        assert_eq!(p.size(), 0);
        assert_eq!(p.value(0.0), 0.0);
        assert_eq!(p.value(5.0), 0.0);
    }

    #[test]
    fn set_param_updates_the_stored_value() {
        let mut p = ConstantParameter::unset(Rc::new(NoConstraint));
        p.set_param(0, 0.03);
        assert_eq!(p.value(0.0), 0.03);
    }

    #[test]
    fn test_params_delegates_to_the_constraint() {
        let p = ConstantParameter::unset(Rc::new(PositiveConstraint));
        assert!(p.test_params(&Array::from([1.0])));
        assert!(!p.test_params(&Array::from([-1.0])));
    }
}
