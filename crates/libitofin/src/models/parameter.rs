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
//! - `NumericalImpl::change` (`parameter.hpp:156`) rewrites the last-set value in
//!   place; it is used only by the generic Brent-fitting `ShortRateTree` ctor
//!   (`onefactormodel.cpp:56`), which is deferred (#463 ports Hull-White's
//!   closed-form `tree()`, which only `reset`s and `set`s). Omitted, not stubbed.
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

use std::cell::RefCell;
use std::rc::Rc;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::array::Array;
use crate::math::optimization::constraint::{Constraint, NoConstraint};
use crate::require;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
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

/// The runtime-settable fitting law behind a [`TermStructureFittingParameter`]
/// (C++'s `TermStructureFittingParameter::NumericalImpl`, `parameter.hpp:147`).
///
/// A short-rate model's numerical `tree()` fits `phi(t)` node by node: it
/// [`reset`](Self::reset)s the law, then [`set`](Self::set)s the fitted value at
/// each grid time as it walks forward. The tree's dynamics read those values
/// back through [`value`](ParameterValue::value) while the fit is still in flight
/// (an earlier grid node's discount feeds the next node's state prices), so the
/// values live behind interior mutability ([`RefCell`]) and the fit must
/// [`set`](Self::set) on the *same* instance the dynamics reads. C++ shares one
/// `NumericalImpl` between the fit and the dynamics via
/// `dynamic_pointer_cast`; here the caller retains a concrete [`Rc<NumericalImpl>`](Rc)
/// and clones it into the [`Parameter`] as an [`Rc<dyn ParameterValue>`](ParameterValue)
/// before type-erasing, so both ends share the one [`RefCell`].
///
/// ## Divergences from QuantLib
///
/// - [`value`](ParameterValue::value) matches C++'s `std::find(times, t)`
///   semantics exactly: an **exact** `Time` equality, not a tolerance compare
///   (`parameter.hpp:164`). The fit `set`s at `grid[i]` and the dynamics read at
///   `timeGrid()[i]` - the same `f64` from the same [`TimeGrid`], so exact
///   equality is the right and faithful lookup.
/// - C++'s `QL_REQUIRE` on a missing time surfaces here as a documented `expect`
///   panic, not a `Result`: [`ParameterValue::value`] is infallible by design
///   (the eval boundary), and a lookup at an unset time is a programming error -
///   the fit always `set`s every grid node before the dynamics read it.
pub struct NumericalImpl {
    nodes: RefCell<Vec<(Time, Real)>>,
    term_structure: Handle<dyn YieldTermStructure>,
}

impl NumericalImpl {
    /// `NumericalImpl(Handle<YieldTermStructure>)` (`parameter.hpp:149`): an empty
    /// law over `term_structure`. Returns an [`Rc`] so the caller can retain a
    /// concrete handle for the fit and clone a type-erased one into a
    /// [`Parameter`] (both sharing the interior [`RefCell`]).
    pub fn new(term_structure: Handle<dyn YieldTermStructure>) -> Rc<Self> {
        Rc::new(NumericalImpl {
            nodes: RefCell::new(Vec::new()),
            term_structure,
        })
    }

    /// `set(Time t, Real x)` (`parameter.hpp:152`): appends the fitted value `x`
    /// at time `t`.
    pub fn set(&self, t: Time, x: Real) {
        self.nodes.borrow_mut().push((t, x));
    }

    /// `reset()` (`parameter.hpp:159`): clears all fitted nodes, readying the law
    /// for a fresh forward fit.
    pub fn reset(&self) {
        self.nodes.borrow_mut().clear();
    }

    /// `termStructure()` (`parameter.hpp:169`): the curve the fit reprices. Read
    /// by the deferred Brent-fitting `ShortRateTree` ctor (`onefactormodel.cpp:69`);
    /// Hull-White's closed-form `tree()` reads the curve off the model instead.
    pub fn term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.term_structure
    }
}

impl ParameterValue for NumericalImpl {
    /// `value(const Array&, Time t)` (`parameter.hpp:163`): the fitted value at
    /// the grid node whose time equals `t` exactly. Panics if `t` was never
    /// `set` (C++'s `QL_REQUIRE(..., "fitting parameter not set!")`).
    fn value(&self, _params: &Array, t: Time) -> Real {
        let nodes = self.nodes.borrow();
        nodes
            .iter()
            .find(|(time, _)| *time == t)
            .map(|(_, x)| *x)
            .expect("fitting parameter not set!")
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

    fn flat_handle() -> Handle<dyn YieldTermStructure> {
        let curve = FlatForward::with_rate(
            Date::new(17, Month::May, 1998),
            0.05,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>)
    }

    #[test]
    fn numerical_impl_set_then_value_round_trips_per_node() {
        // parameter.hpp:152-167: set appends (t, x); value(t) returns the x set at
        // that exact t. reset clears back to empty.
        let impl_ = NumericalImpl::new(flat_handle());
        impl_.set(1.0, 0.031);
        impl_.set(2.0, 0.042);
        impl_.set(3.0, 0.055);
        assert_eq!(impl_.value(&Array::new(), 1.0), 0.031);
        assert_eq!(impl_.value(&Array::new(), 2.0), 0.042);
        assert_eq!(impl_.value(&Array::new(), 3.0), 0.055);
    }

    #[test]
    #[should_panic(expected = "fitting parameter not set!")]
    fn numerical_impl_value_at_an_unset_time_panics_with_the_cpp_message() {
        // parameter.hpp:165: QL_REQUIRE(found, "fitting parameter not set!"). Here
        // the infallible eval boundary surfaces it as a documented panic.
        let impl_ = NumericalImpl::new(flat_handle());
        impl_.set(1.0, 0.03);
        impl_.value(&Array::new(), 2.0);
    }

    #[test]
    #[should_panic(expected = "fitting parameter not set!")]
    fn numerical_impl_lookup_is_exact_not_tolerant() {
        // parameter.hpp:164: std::find is operator==, an EXACT Time match. A time a
        // hair off a set node is "not set", never a nearest-node fallback. This is
        // safe because the fit and the dynamics share the identical grid f64s.
        let impl_ = NumericalImpl::new(flat_handle());
        impl_.set(1.0, 0.03);
        impl_.value(&Array::new(), 1.0 + 1e-12);
    }

    #[test]
    fn numerical_impl_reset_clears_every_node() {
        let impl_ = NumericalImpl::new(flat_handle());
        impl_.set(1.0, 0.03);
        impl_.reset();
        impl_.set(2.0, 0.07);
        assert_eq!(impl_.value(&Array::new(), 2.0), 0.07);
    }

    #[test]
    fn set_through_the_retained_rc_is_visible_through_the_type_erased_parameter() {
        // The gate-amendment same-impl wiring at the parameter level: the fit
        // retains a concrete Rc<NumericalImpl> and clones a type-erased copy into
        // the Parameter; a set() through the retained handle must be seen by
        // Parameter::value (they share the one RefCell).
        let impl_ = NumericalImpl::new(flat_handle());
        let phi = TermStructureFittingParameter::new(impl_.clone() as Rc<dyn ParameterValue>);
        impl_.set(2.5, 0.037);
        assert_eq!(phi.value(2.5), 0.037);
    }
}
