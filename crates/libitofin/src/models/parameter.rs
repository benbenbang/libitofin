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
//! - `PiecewiseConstantParameter` (`parameter.hpp:119`) is time-dependent and
//!   used only by the numerical tree/lattice path; it lands with those tickets.
//! - `TermStructureFittingParameter::NumericalImpl` (`parameter.hpp:147`), the
//!   tree-fitting value law read back by `tree()`, stays deferred with the
//!   lattice machinery. The analytic seam (the generic `Impl` constructor,
//!   `parameter.hpp:178`) is ported here as [`TermStructureFittingParameter`];
//!   concrete fitting laws (Extended CIR's `phi(t)`) are supplied by each model.
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

/// A state-capturing, possibly time-dependent value law for a [`Parameter`]
/// (C++'s polymorphic `Parameter::Impl`, `parameter.hpp:40`).
///
/// The stateless [`ConstantParameter`] / [`NullParameter`] laws collapse to
/// [`ParameterImpl`] variants; a law that captures state - a term-structure
/// fitting `phi(t)` holding a `Handle<YieldTermStructure>` - implements this
/// trait instead and is wrapped by [`TermStructureFittingParameter`]. The method
/// returns a plain [`Real`], so a law reading a fallible source (a curve's
/// forward rate) collapses that `Result` internally with a documented `expect`
/// rather than bubbling it through [`Parameter::value`].
pub trait ParameterValue {
    /// The value at time `t` (C++'s `Impl::value(const Array&, Time)`).
    ///
    /// Takes both the stored `params` and `t` for C++ signature parity, though a
    /// fitting law typically ignores `params` and reads only `t`.
    fn value(&self, params: &Array, t: Time) -> Real;
}

/// The value law of a [`Parameter`] (C++'s `Parameter::Impl` hierarchy).
#[derive(Clone)]
enum ParameterImpl {
    /// `ConstantParameter::Impl` (`parameter.hpp:73`): returns `params[0]`.
    Constant,
    /// `NullParameter::Impl` (`parameter.hpp:101`): always `0.0`.
    Null,
    /// A user-supplied [`ParameterValue`] (C++'s generic `Parameter::Impl`): the
    /// state-capturing, time-dependent law behind
    /// [`TermStructureFittingParameter`]. Held via [`Rc`] so [`Parameter`] stays
    /// [`Clone`] (`PrivateConstraint` clones the arguments).
    Custom(Rc<dyn ParameterValue>),
}

impl ParameterImpl {
    fn value(&self, params: &Array, t: Time) -> Real {
        match self {
            ParameterImpl::Constant => params[0],
            ParameterImpl::Null => 0.0,
            ParameterImpl::Custom(impl_) => impl_.value(params, t),
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

/// Deterministic time-dependent parameter used for yield-curve fitting
/// (`parameter.hpp:145`).
///
/// C++'s `TermStructureFittingParameter` is a size-0, `NoConstraint`
/// [`Parameter`] wrapping an arbitrary `Parameter::Impl`
/// (`Parameter(0, impl, NoConstraint())`, `parameter.hpp:178`). The port mirrors
/// the other parameter factories: [`new`](Self::new) returns the base
/// [`Parameter`] carrying the supplied [`ParameterValue`] law. C++'s second
/// constructor (from a `Handle<YieldTermStructure>`, building the tree-fitting
/// `NumericalImpl`) is deferred with the lattice machinery.
pub struct TermStructureFittingParameter;

impl TermStructureFittingParameter {
    /// `TermStructureFittingParameter(const shared_ptr<Parameter::Impl>&)`
    /// (`parameter.hpp:178`): size 0, `NoConstraint`, value computed by `impl_`.
    pub fn new(impl_: Rc<dyn ParameterValue>) -> Parameter {
        Parameter {
            params: Array::new(),
            constraint: Rc::new(NoConstraint),
            impl_: ParameterImpl::Custom(impl_),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Handle;
    use crate::interestrate::Compounding;
    use crate::math::optimization::constraint::PositiveConstraint;
    use crate::shared::{Shared, shared};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;

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

    /// A stateless-but-time-dependent stand-in for a fitting law: it captures a
    /// slope and returns `slope * t`, ignoring `params`.
    struct LinearLaw {
        slope: Real,
    }

    impl ParameterValue for LinearLaw {
        fn value(&self, _params: &Array, t: Time) -> Real {
            self.slope * t
        }
    }

    #[test]
    fn fitting_parameter_wraps_a_time_dependent_state_capturing_law() {
        let p = TermStructureFittingParameter::new(Rc::new(LinearLaw { slope: 0.02 }));
        assert_eq!(p.size(), 0);
        assert_eq!(p.value(0.0), 0.0);
        assert!((p.value(3.0) - 0.06).abs() < 1e-15);
    }

    /// A fitting law that captures a term-structure handle and reads it *live*
    /// on every evaluation, exactly as Extended CIR's `phi(t)` does. The curve's
    /// `forward_rate` is fallible, but [`Parameter::value`] is not, so the law
    /// collapses the `Result` internally with a documented `expect` (the crate's
    /// eval-infallible convention); the production law itself lands in #382.
    struct ForwardPlusSlope {
        term_structure: Handle<dyn YieldTermStructure>,
        slope: Real,
    }

    impl ParameterValue for ForwardPlusSlope {
        fn value(&self, _params: &Array, t: Time) -> Real {
            let curve = self
                .term_structure
                .current_link()
                .expect("a fitting parameter requires a non-empty term-structure handle");
            let forward = curve
                .forward_rate(t, t, Compounding::Continuous, Frequency::NoFrequency, false)
                .expect("a fitting parameter's forward rate is well-defined on its curve")
                .rate();
            forward + self.slope * t
        }
    }

    #[test]
    fn fitting_parameter_reads_its_captured_term_structure_live() {
        let curve = FlatForward::with_rate(
            Date::new(17, Month::May, 1998),
            0.05,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        let handle: Handle<dyn YieldTermStructure> =
            Handle::new(shared(curve) as Shared<dyn YieldTermStructure>);
        let phi = TermStructureFittingParameter::new(Rc::new(ForwardPlusSlope {
            term_structure: handle,
            slope: 0.01,
        }));

        assert_eq!(phi.size(), 0);
        assert!((phi.value(2.0) - (0.05 + 0.01 * 2.0)).abs() < 1e-9);
        assert!((phi.value(0.5) - (0.05 + 0.01 * 0.5)).abs() < 1e-9);
    }
}
