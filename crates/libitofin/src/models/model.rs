//! Calibrated model base.
//!
//! Port of the [`CalibratedModel`] parts of `ql/models/model.{hpp,cpp}` needed
//! by the closed-form affine path: the [`Parameter`] storage, `params()` /
//! `set_params()`, and the [`PrivateConstraint`] over the arguments. A concrete
//! model embeds a `CalibratedModel` the way a curve embeds a
//! [`TermStructureBase`](crate::termstructures::TermStructureBase), delegates
//! its parameter machinery, and exposes the shared [`Observable`] through
//! [`AsObservable`].
//!
//! ## Deferred
//!
//! - `calibrate()` against the `CalibrationHelper` family (`model.hpp:99`) and
//!   the `CalibrationFunction` cost function (`model.cpp:35`): calibration needs
//!   the (not-yet-ported) helper family; the affine `discountBond` oracle
//!   constructs a model with explicit parameters and never calibrates. The
//!   optimization stack it would drive (`Constraint`, `Problem`,
//!   `LevenbergMarquardt`) is already on main, so this is a later ticket, not a
//!   missing dependency.
//! - Observer-driven regeneration when observed market data changes. C++'s
//!   `update()` = `generateArguments()` + `notifyObservers()` (`model.hpp:88`)
//!   runs when a registered observable notifies; wiring an observer half that
//!   re-runs [`generate_arguments`](CalibratedModelHolder::generate_arguments)
//!   needs the concrete model to register itself, which the affine oracle does
//!   not exercise (ExtendedCoxIngersollRoss reads its curve live on every
//!   evaluation and does not `registerWith(termStructure)`, unlike Hull-White /
//!   G2). The [`set_params`](CalibratedModelHolder::set_params) regeneration
//!   path is ported; the observer-driven path is deferred to the model that
//!   first needs it.
//!
//! ## Divergences from QuantLib
//!
//! - C++'s `PrivateConstraint::Impl` holds `const std::vector<Parameter>&`
//!   aliasing the model's own `arguments_` (`model.hpp:168,223`), a
//!   self-referential borrow Rust cannot hold. [`CalibratedModel::constraint`]
//!   instead builds the [`PrivateConstraint`] on demand from a clone of the
//!   current arguments. It is consumed only by the deferred `calibrate()`, so
//!   it is unexercised by the affine oracle.
//! - C++'s `setParams` always calls the virtual `generateArguments()`
//!   (`model.cpp:146`); a subclass override is dispatched through the base. An
//!   embedded [`CalibratedModel`] cannot dispatch up into the concrete type, so
//!   the regenerating `setParams` lives on the [`CalibratedModelHolder`] trait
//!   the concrete model implements, and [`CalibratedModel::set_params`] itself
//!   only writes and notifies (the base's `generateArguments()` is a no-op).
//!   Writing through the embedded model directly therefore bypasses
//!   regeneration - a fitted model must route parameter changes through its
//!   holder [`set_params`](CalibratedModelHolder::set_params).

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::array::Array;
use crate::math::optimization::constraint::Constraint;
use crate::models::parameter::Parameter;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::require;
use crate::shared::{Shared, SharedMut, shared};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::Size;

/// Calibrated model class (`model.hpp:86`).
///
/// Both [`Observable`] (pricing engines observe the model) and, through its
/// [`as_observer`](CalibratedModel::as_observer) handle, an observer half that
/// regenerates and re-broadcasts when observed market data changes (C++'s
/// `update()` = `generateArguments()` + `notifyObservers()`, `model.hpp:90`).
pub struct CalibratedModel {
    arguments: Vec<Parameter>,
    observable: Shared<Observable>,
    updater: SharedMut<ResetThenNotify>,
}

impl CalibratedModel {
    /// `CalibratedModel(Size nArguments)` (`model.cpp:32`): `n_arguments`
    /// placeholder parameters the concrete model overwrites in its constructor.
    pub fn new(n_arguments: Size) -> CalibratedModel {
        let observable = shared(Observable::new());
        let updater = ResetThenNotify::forwarding(Shared::clone(&observable));
        CalibratedModel {
            arguments: vec![Parameter::default(); n_arguments],
            observable,
            updater,
        }
    }

    /// The model's arguments, for a concrete model's inspectors
    /// (C++'s protected `arguments_`).
    pub fn arguments(&self) -> &[Parameter] {
        &self.arguments
    }

    /// The model's arguments for a concrete model's constructor to populate.
    /// The count is fixed at [`new`](CalibratedModel::new), so this is a slice:
    /// a subclass sets each argument but cannot resize the vector out from
    /// under `params()` or the constraint. Public because Rust has no
    /// "protected": this is the model-building surface C++ exposes to subclasses.
    pub fn arguments_mut(&mut self) -> &mut [Parameter] {
        &mut self.arguments
    }

    /// `params()` (`model.cpp:126`): the arguments' values flattened in order.
    pub fn params(&self) -> Array {
        let size = self.arguments.iter().map(Parameter::size).sum();
        let mut params = Array::with_size(size);
        let mut k = 0;
        for argument in &self.arguments {
            for j in 0..argument.size() {
                params[k] = argument.params()[j];
                k += 1;
            }
        }
        params
    }

    /// Re-slices `params` back into the arguments in order, without notifying:
    /// the `setParams` re-slice loop (`model.cpp:139-146`) split out so a
    /// [`CalibratedModelHolder`] can regenerate derived state before it notifies
    /// (D1: no observer runs until every mutation is done).
    ///
    /// # Errors
    ///
    /// Fails if `params` has too few values for the arguments
    /// (`"parameter array too small"`) or too many
    /// (`"parameter array too big!"`).
    pub fn write_params(&mut self, params: &Array) -> QlResult<()> {
        let mut p = 0;
        for argument in &mut self.arguments {
            for j in 0..argument.size() {
                require!(p < params.size(), "parameter array too small");
                argument.set_param(j, params[p]);
                p += 1;
            }
        }
        require!(p == params.size(), "parameter array too big!");
        Ok(())
    }

    /// `setParams(const Array&)` (`model.cpp:138`): re-slices `params` back into
    /// the arguments in order, then notifies. The C++ base also calls the
    /// virtual `generateArguments()` (a no-op for the base and for Vasicek); a
    /// model whose `generateArguments()` is not a no-op must instead route
    /// through [`CalibratedModelHolder::set_params`], which regenerates before
    /// notifying.
    ///
    /// # Errors
    ///
    /// Fails if `params` has too few or too many values for the arguments.
    pub fn set_params(&mut self, params: &Array) -> QlResult<()> {
        self.write_params(params)?;
        self.observable.notify_observers();
        Ok(())
    }

    /// `constraint()` (`model.hpp:159`): the [`PrivateConstraint`] over the
    /// current arguments, rebuilt from a snapshot on each call.
    pub fn constraint(&self) -> PrivateConstraint {
        PrivateConstraint {
            arguments: self.arguments.clone(),
        }
    }

    /// This model's observer half, wired to its observable (C++'s
    /// `CalibratedModel` publicly *is* an `Observer`). Register it with an
    /// observed observable so a change regenerates and re-broadcasts; the
    /// term-structure-consistent models are the first to do so.
    pub fn as_observer(&self) -> SharedMut<dyn Observer> {
        self.updater.clone() as SharedMut<dyn Observer>
    }
}

impl AsObservable for CalibratedModel {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

/// The regeneration seam C++ puts on [`CalibratedModel`]: the virtual
/// `generateArguments()` and the `setParams` path that invokes it
/// (`model.hpp:88`, `model.cpp:138`).
///
/// A concrete model embeds a [`CalibratedModel`], which - unlike the C++ base -
/// cannot virtual-dispatch up into the concrete type. The overridable
/// `generateArguments()` hook therefore lives here, on a trait the concrete
/// model implements over its embedded model. A model whose C++
/// `generateArguments()` is the base no-op (Vasicek) can leave the defaults; a
/// term-structure-fitted model overrides
/// [`generate_arguments`](Self::generate_arguments) to rebuild the parameters
/// derived from its arguments, and both its constructor and any parameter change
/// go through this seam.
pub trait CalibratedModelHolder {
    /// The embedded [`CalibratedModel`].
    fn calibrated_model(&self) -> &CalibratedModel;

    /// The embedded [`CalibratedModel`], mutably.
    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel;

    /// `generateArguments()` (`model.hpp:154`): rebuild the parameters derived
    /// from the model's arguments. The base is a no-op.
    fn generate_arguments(&mut self) {}

    /// `setParams(const Array&)` (`model.cpp:138`): re-slice `params` into the
    /// arguments, regenerate derived state, then notify - in that order, so no
    /// observer runs until every mutation is done (D1).
    ///
    /// # Errors
    ///
    /// Fails if `params` has too few or too many values for the arguments.
    fn set_params(&mut self, params: &Array) -> QlResult<()> {
        self.calibrated_model_mut().write_params(params)?;
        self.generate_arguments();
        self.calibrated_model().observable().notify_observers();
        Ok(())
    }
}

/// Term-structure consistent model base (`model.hpp:73`): holds the
/// [`YieldTermStructure`] handle a fitted model reprices exactly, and exposes it.
///
/// C++'s class is `public virtual Observable`; here it is a plain holder. A
/// concrete fitted model embeds both this and a [`CalibratedModel`] (C++
/// multiply-inherits them). Whether the model *observes* the handle is a
/// per-model choice made in the concrete constructor: Hull-White / G2 / GSR
/// `registerWith(termStructure)`, but ExtendedCoxIngersollRoss (the affine
/// fitting oracle) deliberately does not, because its fitting law reads the
/// handle live on every evaluation.
pub struct TermStructureConsistentModel {
    term_structure: Handle<dyn YieldTermStructure>,
}

impl TermStructureConsistentModel {
    /// `TermStructureConsistentModel(Handle<YieldTermStructure>)`
    /// (`model.hpp:75`).
    pub fn new(term_structure: Handle<dyn YieldTermStructure>) -> TermStructureConsistentModel {
        TermStructureConsistentModel { term_structure }
    }

    /// `termStructure()` (`model.hpp:77`): the fitted curve handle.
    pub fn term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.term_structure
    }
}

/// Constraint imposed on a [`CalibratedModel`]'s arguments (`model.hpp:164`).
///
/// Owns a snapshot of the arguments (see the module divergence note) and, for
/// each, slices the flat parameter array into that argument's share, in order,
/// delegating to the argument's own constraint (`model.hpp:171-220`).
pub struct PrivateConstraint {
    arguments: Vec<Parameter>,
}

impl PrivateConstraint {
    fn bound(&self, params: &Array, f: impl Fn(&dyn Constraint, &Array) -> Array) -> Array {
        let total: Size = self.arguments.iter().map(Parameter::size).sum();
        let mut result = Array::with_size(total);
        let mut k = 0;
        let mut k2 = 0;
        for argument in &self.arguments {
            let size = argument.size();
            let mut partial = Array::with_size(size);
            for j in 0..size {
                partial[j] = params[k];
                k += 1;
            }
            let tmp = f(argument.constraint(), &partial);
            for j in 0..size {
                result[k2] = tmp[j];
                k2 += 1;
            }
        }
        result
    }
}

impl Constraint for PrivateConstraint {
    fn test(&self, params: &Array) -> bool {
        let mut k = 0;
        for argument in &self.arguments {
            let size = argument.size();
            let mut test_params = Array::with_size(size);
            for j in 0..size {
                test_params[j] = params[k];
                k += 1;
            }
            if !argument.test_params(&test_params) {
                return false;
            }
        }
        true
    }

    fn upper_bound(&self, params: &Array) -> Array {
        self.bound(params, |constraint, partial| {
            constraint.upper_bound(partial)
        })
    }

    fn lower_bound(&self, params: &Array) -> Array {
        self.bound(params, |constraint, partial| {
            constraint.lower_bound(partial)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interestrate::Compounding;
    use crate::math::optimization::constraint::{NoConstraint, PositiveConstraint};
    use crate::models::parameter::ConstantParameter;
    use crate::patterns::observable::Observer;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use std::rc::Rc;

    struct UpdateCounter {
        count: usize,
    }

    impl Observer for UpdateCounter {
        fn update(&mut self) {
            self.count += 1;
        }
    }

    fn two_argument_model() -> CalibratedModel {
        let mut model = CalibratedModel::new(2);
        model.arguments_mut()[0] =
            ConstantParameter::new(0.1, Rc::new(PositiveConstraint)).unwrap();
        model.arguments_mut()[1] = ConstantParameter::new(0.2, Rc::new(NoConstraint)).unwrap();
        model
    }

    #[test]
    fn params_flattens_arguments_in_order() {
        let model = two_argument_model();
        assert_eq!(model.params(), Array::from([0.1, 0.2]));
    }

    #[test]
    fn set_params_round_trips_through_params() {
        let mut model = two_argument_model();
        model.set_params(&Array::from([0.3, 0.4])).unwrap();
        assert_eq!(model.params(), Array::from([0.3, 0.4]));
        assert_eq!(model.arguments()[0].value(0.0), 0.3);
        assert_eq!(model.arguments()[1].value(0.0), 0.4);
    }

    #[test]
    fn set_params_rejects_too_few_values() {
        let mut model = two_argument_model();
        let err = model.set_params(&Array::from([0.3])).unwrap_err();
        assert_eq!(err.message(), "parameter array too small");
    }

    #[test]
    fn set_params_rejects_too_many_values() {
        let mut model = two_argument_model();
        let err = model.set_params(&Array::from([0.3, 0.4, 0.5])).unwrap_err();
        assert_eq!(err.message(), "parameter array too big!");
    }

    #[test]
    fn private_constraint_tests_each_argument_on_its_own_slice() {
        let constraint = two_argument_model().constraint();
        // argument 0 is positive-constrained, argument 1 is unconstrained
        assert!(constraint.test(&Array::from([0.5, -9.0])));
        assert!(!constraint.test(&Array::from([-0.5, 0.0])));
    }

    #[test]
    fn private_constraint_slices_bounds_per_argument() {
        let constraint = two_argument_model().constraint();
        // PositiveConstraint lower bound is 0.0; NoConstraint lower bound is -MAX
        let lower = constraint.lower_bound(&Array::from([0.5, 0.5]));
        assert_eq!(lower[0], 0.0);
        assert_eq!(lower[1], -crate::types::Real::MAX);
    }

    #[test]
    fn set_params_notifies_registered_observers() {
        let mut model = two_argument_model();
        let counter = shared_mut(UpdateCounter { count: 0 });
        model
            .observable()
            .register_observer(&(counter.clone() as SharedMut<dyn Observer>));

        model.set_params(&Array::from([0.3, 0.4])).unwrap();
        assert_eq!(counter.borrow().count, 1);
    }

    #[test]
    fn updater_regenerates_and_rebroadcasts_to_the_models_observers() {
        let model = CalibratedModel::new(0);
        let counter = shared_mut(UpdateCounter { count: 0 });
        model
            .observable()
            .register_observer(&(counter.clone() as SharedMut<dyn Observer>));

        // driving the observer half (as an observed observable would) forwards
        // the notification to the model's own observers
        model.as_observer().borrow_mut().update();
        assert_eq!(counter.borrow().count, 1);
    }

    #[test]
    fn write_params_updates_the_arguments_without_notifying() {
        let mut model = two_argument_model();
        let counter = shared_mut(UpdateCounter { count: 0 });
        model
            .observable()
            .register_observer(&(counter.clone() as SharedMut<dyn Observer>));

        model.write_params(&Array::from([0.3, 0.4])).unwrap();

        assert_eq!(model.params(), Array::from([0.3, 0.4]));
        assert_eq!(counter.borrow().count, 0);
    }

    /// A minimal fitted model: it counts every regeneration so a test can pin
    /// that the holder's `set_params` fires `generate_arguments`.
    struct FittedModel {
        model: CalibratedModel,
        regenerations: usize,
    }

    impl CalibratedModelHolder for FittedModel {
        fn calibrated_model(&self) -> &CalibratedModel {
            &self.model
        }
        fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
            &mut self.model
        }
        fn generate_arguments(&mut self) {
            self.regenerations += 1;
        }
    }

    #[test]
    fn holder_set_params_writes_regenerates_then_notifies() {
        let mut fitted = FittedModel {
            model: two_argument_model(),
            regenerations: 0,
        };
        let counter = shared_mut(UpdateCounter { count: 0 });
        fitted
            .calibrated_model()
            .observable()
            .register_observer(&(counter.clone() as SharedMut<dyn Observer>));

        fitted.set_params(&Array::from([0.3, 0.4])).unwrap();

        assert_eq!(fitted.calibrated_model().params(), Array::from([0.3, 0.4]));
        assert_eq!(fitted.regenerations, 1);
        assert_eq!(counter.borrow().count, 1);
    }

    #[test]
    fn holder_set_params_propagates_the_size_error() {
        let mut fitted = FittedModel {
            model: two_argument_model(),
            regenerations: 0,
        };
        let err = fitted.set_params(&Array::from([0.3])).unwrap_err();
        assert_eq!(err.message(), "parameter array too small");
        assert_eq!(fitted.regenerations, 0);
    }

    #[test]
    fn term_structure_consistent_model_exposes_its_handle() {
        let curve = FlatForward::with_rate(
            Date::new(17, Month::May, 1998),
            0.05,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        let handle: Handle<dyn YieldTermStructure> =
            Handle::new(shared(curve) as Shared<dyn YieldTermStructure>);
        let consistent = TermStructureConsistentModel::new(handle.clone());
        assert!(consistent.term_structure().points_to_same_link(&handle));
    }
}
