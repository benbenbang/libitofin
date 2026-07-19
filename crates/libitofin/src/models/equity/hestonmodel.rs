//! The Heston stochastic-volatility calibrated model.
//!
//! Port of `ql/models/equity/hestonmodel.{hpp,cpp}`: the 5-parameter
//! [`CalibratedModel`] wrapper over a [`HestonProcess`]. Its arguments are
//! `(theta, kappa, sigma, rho, v0)` (`hestonmodel.cpp:28-37`) and its
//! [`process`](HestonModel::process) accessor returns a [`HestonProcess`] rebuilt
//! from those arguments, so the (next-batch) `AnalyticHestonEngine` always reads
//! the current parameters.
//!
//! ## The composition-loses-dispatch point
//!
//! C++ `HestonModel::generateArguments()` (`hestonmodel.cpp:45`) rebuilds
//! `process_` as a fresh `HestonProcess` on every parameter move. [`HestonModel`]
//! embeds a [`CalibratedModel`] and implements [`CalibratedModelHolder`], whose
//! [`set_params`](CalibratedModelHolder::set_params) fires
//! [`generate_arguments`](CalibratedModelHolder::generate_arguments) - so a fitted
//! model's `process()` reflects the new parameters, not the stale ctor ones.
//!
//! The `arguments_` order `(theta, kappa, sigma, rho, v0)` differs from the
//! [`HestonProcess`] constructor signature `(rf, div, s0, v0, kappa, theta,
//! sigma, rho)`, and `generate_arguments` feeds the getters positionally into
//! that signature (`hestonmodel.cpp:46-50`): `HestonProcess::new(rf, div, s0,
//! v0(), kappa(), theta(), sigma(), rho())`.
//!
//! ## Observation
//!
//! C++ `registerWith`s the process's three market handles (risk-free curve,
//! dividend curve, spot quote; `hestonmodel.cpp:41-43`), so a relink rebuilds
//! `process_`. This port wires the same observation through
//! [`register_with_term_structure`] on the risk-free curve handle, then registers
//! the same observer on the dividend curve and spot-quote handles; a change to
//! any of the three re-runs `generate_arguments` and re-broadcasts.
//! [`HestonModel::new`] returns a [`SharedMut`] so that observer, which holds a
//! weak back-reference to the model, can be stashed after the model is shared (as
//! [`HullWhite::new`](crate::models::HullWhite) does).

use std::rc::Rc;

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::constraint::{BoundaryConstraint, Constraint, PositiveConstraint};
use crate::models::model::{CalibratedModel, CalibratedModelHolder, register_with_term_structure};
use crate::models::parameter::ConstantParameter;
use crate::patterns::observable::Observer;
use crate::processes::HestonProcess;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::types::Real;

/// The Heston model (`hestonmodel.hpp:44`): a 5-parameter [`CalibratedModel`]
/// over a [`HestonProcess`].
pub struct HestonModel {
    model: CalibratedModel,
    process: Shared<HestonProcess>,
    /// Keeps the market-handle observer alive: each handle holds only a weak
    /// back-reference to it (see [`register_with_term_structure`]), so dropping
    /// it here would unregister the model. Never read directly.
    #[allow(dead_code)]
    observer: Option<SharedMut<dyn Observer>>,
}

impl HestonModel {
    /// `HestonModel(const shared_ptr<HestonProcess>&)` (`hestonmodel.cpp:24`):
    /// seeds the five arguments from the process's parameters, runs
    /// `generateArguments()` to rebuild the process from them, then registers as
    /// an observer of the process's three market handles.
    ///
    /// Returns a [`SharedMut`] so the observer, which holds a weak reference back
    /// to the model, can be stashed after the model is shared (C++ registers
    /// `this` from inside the constructor).
    ///
    /// # Errors
    ///
    /// Fails if a process parameter violates its argument's constraint: `theta`,
    /// `kappa`, `sigma` or `v0` not strictly positive, or `rho` outside
    /// `[-1, 1]` (C++'s `ConstantParameter` `QL_REQUIRE`).
    pub fn new(process: Shared<HestonProcess>) -> QlResult<SharedMut<HestonModel>> {
        let mut model = CalibratedModel::new(5);
        model.arguments_mut()[0] =
            ConstantParameter::new(process.theta(), Rc::new(PositiveConstraint))?;
        model.arguments_mut()[1] =
            ConstantParameter::new(process.kappa(), Rc::new(PositiveConstraint))?;
        model.arguments_mut()[2] =
            ConstantParameter::new(process.sigma(), Rc::new(PositiveConstraint))?;
        model.arguments_mut()[3] =
            ConstantParameter::new(process.rho(), Rc::new(BoundaryConstraint::new(-1.0, 1.0)))?;
        model.arguments_mut()[4] =
            ConstantParameter::new(process.v0(), Rc::new(PositiveConstraint))?;

        let risk_free_rate = process.risk_free_rate();
        let dividend_yield = process.dividend_yield();
        let s0 = process.s0();

        let mut heston = HestonModel {
            model,
            process,
            observer: None,
        };
        heston.generate_arguments();

        let shared = shared_mut(heston);
        let observer = register_with_term_structure(&shared, &risk_free_rate);
        dividend_yield.register_observer(&observer);
        s0.register_observer(&observer);
        shared.borrow_mut().observer = Some(observer);
        Ok(shared)
    }

    /// Variance mean version level `theta` = `arguments_[0](0.0)`
    /// (`hestonmodel.hpp:48`).
    pub fn theta(&self) -> Real {
        self.model.arguments()[0].value(0.0)
    }

    /// Variance mean reversion speed `kappa` = `arguments_[1](0.0)`
    /// (`hestonmodel.hpp:50`).
    pub fn kappa(&self) -> Real {
        self.model.arguments()[1].value(0.0)
    }

    /// Volatility of the volatility `sigma` = `arguments_[2](0.0)`
    /// (`hestonmodel.hpp:52`).
    pub fn sigma(&self) -> Real {
        self.model.arguments()[2].value(0.0)
    }

    /// Spot/variance correlation `rho` = `arguments_[3](0.0)`
    /// (`hestonmodel.hpp:54`).
    pub fn rho(&self) -> Real {
        self.model.arguments()[3].value(0.0)
    }

    /// Spot variance `v0` = `arguments_[4](0.0)` (`hestonmodel.hpp:56`).
    pub fn v0(&self) -> Real {
        self.model.arguments()[4].value(0.0)
    }

    /// The underlying process (`hestonmodel.hpp:58`), rebuilt from the current
    /// parameters by every [`generate_arguments`](CalibratedModelHolder::generate_arguments).
    pub fn process(&self) -> Shared<HestonProcess> {
        self.process.clone()
    }
}

/// `HestonModel::generateArguments()` (`hestonmodel.cpp:45`): rebuilds the
/// process from the current parameters, mapping the `(theta, kappa, sigma, rho,
/// v0)` argument getters positionally into the `HestonProcess` constructor
/// signature `(rf, div, s0, v0, kappa, theta, sigma, rho)`.
impl CalibratedModelHolder for HestonModel {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.model
    }

    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.model
    }

    fn generate_arguments(&mut self) {
        self.process = shared(HestonProcess::new(
            self.process.risk_free_rate(),
            self.process.dividend_yield(),
            self.process.s0(),
            self.v0(),
            self.kappa(),
            self.theta(),
            self.sigma(),
            self.rho(),
        ));
    }
}

/// `HestonModel::FellerConstraint` (`hestonmodel.hpp:66`): the Feller condition
/// `2 kappa theta > sigma^2` (with `sigma >= 0`), which keeps the variance
/// strictly positive. A caller passes it to
/// [`calibrate`](crate::models::calibrate) as the additional constraint.
///
/// Reads `theta = params[0]`, `kappa = params[1]`, `sigma = params[2]`
/// (`hestonmodel.hpp:71-73`), the model's argument order.
pub struct FellerConstraint;

impl Constraint for FellerConstraint {
    fn test(&self, params: &Array) -> bool {
        let theta = params[0];
        let kappa = params[1];
        let sigma = params[2];
        sigma >= 0.0 && sigma * sigma < 2.0 * kappa * theta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::{Handle, RelinkableHandle};
    use crate::interestrate::Compounding;
    use crate::quotes::make_quote_handle;
    use crate::shared::Shared;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::Rate;

    const S0: Real = 100.0;
    const V0: Real = 0.04;
    const KAPPA: Real = 1.2;
    const THETA: Real = 0.06;
    const SIGMA: Real = 0.3;
    const RHO: Real = -0.5;
    const R: Rate = 0.05;
    const Q: Rate = 0.02;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn process_on(risk_free: Handle<dyn YieldTermStructure>) -> Shared<HestonProcess> {
        shared(HestonProcess::new(
            risk_free,
            flat_yield(Q),
            make_quote_handle(S0).handle(),
            V0,
            KAPPA,
            THETA,
            SIGMA,
            RHO,
        ))
    }

    fn make_model() -> SharedMut<HestonModel> {
        HestonModel::new(process_on(flat_yield(R))).unwrap()
    }

    /// Round-trips all five parameters through both the model getters and the
    /// rebuilt process. Five distinct values so any transposition of the
    /// argument order or the positional `HestonProcess` constructor mapping in
    /// `generate_arguments` (run once at construction) fails.
    #[test]
    fn ctor_round_trips_params_and_rebuilt_process() {
        let model = make_model();
        let m = model.borrow();

        assert_eq!(m.theta(), THETA);
        assert_eq!(m.kappa(), KAPPA);
        assert_eq!(m.sigma(), SIGMA);
        assert_eq!(m.rho(), RHO);
        assert_eq!(m.v0(), V0);

        let process = m.process();
        assert_eq!(process.theta(), THETA);
        assert_eq!(process.kappa(), KAPPA);
        assert_eq!(process.sigma(), SIGMA);
        assert_eq!(process.rho(), RHO);
        assert_eq!(process.v0(), V0);
    }

    /// The load-bearing pin: the holder `set_params` must fire
    /// `generate_arguments`, which rebuilds the process. Asserting on
    /// `process().X()` for all five params (not `model.X()`) discriminates the
    /// missing-regenerate bug: `write_params` updates the arguments regardless,
    /// so `model.X()` would pass even if the process were never rebuilt - only
    /// the rebuilt process reflects the new values. Distinct new values so a
    /// transposed slot fails.
    #[test]
    fn set_params_rebuilds_the_process() {
        let model = make_model();

        let new_theta = 0.07;
        let new_kappa = 1.5;
        let new_sigma = 0.35;
        let new_rho = -0.6;
        let new_v0 = 0.05;
        model
            .borrow_mut()
            .set_params(&Array::from([
                new_theta, new_kappa, new_sigma, new_rho, new_v0,
            ]))
            .unwrap();

        let m = model.borrow();
        assert_eq!(m.sigma(), new_sigma);

        let process = m.process();
        assert_eq!(process.theta(), new_theta);
        assert_eq!(process.kappa(), new_kappa);
        assert_eq!(process.sigma(), new_sigma);
        assert_eq!(process.rho(), new_rho);
        assert_eq!(process.v0(), new_v0);
    }

    /// A relink of an observed market handle must rebuild the process, the
    /// observation -> `generate_arguments` bridge the `AnalyticHestonEngine` will
    /// depend on. The rebuilt process is a fresh allocation, so the pre-relink
    /// and post-relink `Shared<HestonProcess>` no longer point to the same
    /// object; the parameters are unchanged, so the values still match.
    #[test]
    fn relink_rebuilds_the_process() {
        let rh: RelinkableHandle<dyn YieldTermStructure> =
            RelinkableHandle::new(shared(FlatForward::with_rate(
                reference(),
                R,
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);
        let model = HestonModel::new(process_on(rh.handle())).unwrap();

        let before = model.borrow().process();
        rh.link_to(shared(FlatForward::with_rate(
            reference(),
            0.08,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let after = model.borrow().process();

        assert!(
            !Shared::ptr_eq(&before, &after),
            "a relink must rebuild the process"
        );
        assert_eq!(after.sigma(), SIGMA);
        assert_eq!(after.v0(), V0);
    }

    /// `FellerConstraint::test` (`hestonmodel.hpp:71`): true when `sigma^2 < 2
    /// kappa theta` and `sigma >= 0`, false when the Feller inequality is
    /// violated, false when `sigma < 0`. Reads `theta`/`kappa`/`sigma` from the
    /// first three slots of the model argument order.
    #[test]
    fn feller_constraint_tests_the_variance_positivity_condition() {
        let c = FellerConstraint;

        // sigma^2 = 0.09 < 2 * 1.2 * 0.06 = 0.144
        assert!(c.test(&Array::from([THETA, KAPPA, SIGMA, RHO, V0])));

        // sigma^2 = 0.25 >= 0.144: Feller violated, sign gate still holds
        assert!(!c.test(&Array::from([THETA, KAPPA, 0.5, RHO, V0])));

        // sigma < 0: sign gate fails
        assert!(!c.test(&Array::from([THETA, KAPPA, -0.1, RHO, V0])));
    }
}
