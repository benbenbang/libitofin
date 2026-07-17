//! The Extended (term-structure-fitted) Cox-Ingersoll-Ross model.
//!
//! Port of `ql/models/shortrate/onefactormodels/extendedcoxingersollross.{hpp,
//! cpp}`: the short rate `r_t = phi(t) + y_t`, where `y_t` is a standard CIR
//! process and `phi(t)` is the deterministic fitting parameter chosen so the
//! model reprices the input [`YieldTermStructure`] exactly.
//! [`ExtendedCoxIngersollRoss`] composes a base [`CoxIngersollRoss`] (C++
//! subclasses it) plus a [`TermStructureConsistentModel`] (the fitted curve
//! handle) and rebuilds `phi_` through the #381
//! [`CalibratedModelHolder`](crate::models::CalibratedModelHolder) seam.
//!
//! ## Trait re-wire (the composition-loses-dispatch trap)
//!
//! C++ overrides only `A(t,s)`; `B` and `discountBond` are inherited unchanged.
//! Rust cannot subclass, so [`ExtendedCoxIngersollRoss`] gets its own
//! [`OneFactorAffineModel`] impl: [`a`](OneFactorAffineModel::a) is the fitting
//! override, [`b`](OneFactorAffineModel::b) delegates to the embedded base CIR,
//! and [`discount_bond`](OneFactorAffineModel::discount_bond) is the inherited
//! trait default (which then dispatches to *this* type's `a`/`b`). Delegating
//! `a` to the base instead would make `discount_bond` return the base CIR price
//! rather than the forward-discount ratio; a dedicated test
//! (`a_override_reprices_the_curve_where_base_cir_does_not`) pins the difference.
//!
//! ## Deferred (omitted, not stubbed)
//!
//! - `discountBondOption` (`extendedcoxingersollross.cpp:62`) needs the
//!   non-central chi-square surface and has no oracle in this batch (#262 rule).
//! - `ExtendedCoxIngersollRoss::Dynamics` (`extendedcoxingersollross.hpp:90`),
//!   `tree` (`extendedcoxingersollross.cpp:36`) and
//!   `FittingParameter::NumericalImpl` (the tree-fitting law) are the
//!   simulation/lattice path, deferred with the short-rate dynamics per #377.
//!
//! ## Divergences from QuantLib
//!
//! - C++'s `FittingParameter` is a `TermStructureFittingParameter` subclass
//!   whose `Impl` holds `(Handle, theta, k, sigma, x0)`. Here it is a
//!   [`ParameterValue`] ([`FittingParameterValue`]) wrapped by
//!   [`TermStructureFittingParameter`], matching the #381 seam. Its `value`
//!   reads the curve's fallible `forward_rate` and collapses the `Result` with a
//!   documented `expect` (the crate's eval-infallible convention).
//! - C++ multiply-inherits `CoxIngersollRoss` and `TermStructureConsistentModel`;
//!   here both are composed as fields. Per the base model, this class does *not*
//!   `registerWith(termStructure)` (unlike Hull-White / G2): the fitting law
//!   reads the handle live on every evaluation.

use std::rc::Rc;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::models::model::{CalibratedModel, CalibratedModelHolder, TermStructureConsistentModel};
use crate::models::parameter::{
    NullParameter, Parameter, ParameterValue, TermStructureFittingParameter,
};
use crate::models::shortrate::coxingersollross::CoxIngersollRoss;
use crate::models::shortrate::onefactormodel::OneFactorAffineModel;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// The analytical fitting law `phi(t)` (`extendedcoxingersollross.hpp:113`).
///
/// `phi(t) = f(0,t) - 2 k theta (e^{th}-1)/D - x0 4 h^2 e^{th}/D^2`, with
/// `D = 2h + (k+h)(e^{th}-1)`, `h = sqrt(k^2 + 2 sigma^2)` and `f(0,t)` the
/// instantaneous forward read live from the captured curve. Wrapped by
/// [`TermStructureFittingParameter`] into the model's `phi_`.
struct FittingParameterValue {
    term_structure: Handle<dyn YieldTermStructure>,
    theta: Real,
    k: Real,
    sigma: Real,
    x0: Real,
}

impl ParameterValue for FittingParameterValue {
    fn value(&self, _params: &Array, t: Time) -> Real {
        let curve = self
            .term_structure
            .current_link()
            .expect("the extended CIR fitting law requires a non-empty term-structure handle");
        let forward = curve
            .forward_rate(t, t, Compounding::Continuous, Frequency::NoFrequency, false)
            .expect("the extended CIR fitting law's forward rate is well-defined on its curve")
            .rate();
        let h = (self.k * self.k + 2.0 * self.sigma * self.sigma).sqrt();
        let expth = (t * h).exp();
        let temp = 2.0 * h + (self.k + h) * (expth - 1.0);
        forward
            - 2.0 * self.k * self.theta * (expth - 1.0) / temp
            - self.x0 * 4.0 * h * h * expth / (temp * temp)
    }
}

/// The extended Cox-Ingersoll-Ross model (`extendedcoxingersollross.hpp:47`).
pub struct ExtendedCoxIngersollRoss {
    base: CoxIngersollRoss,
    ts_model: TermStructureConsistentModel,
    phi: Parameter,
}

impl ExtendedCoxIngersollRoss {
    /// `ExtendedCoxIngersollRoss(const Handle<YieldTermStructure>&, Real theta,
    /// Real k, Real sigma, Real x0, bool withFellerConstraint)`
    /// (`extendedcoxingersollross.cpp:27`): chains the base as
    /// `CoxIngersollRoss(x0, theta, k, sigma, withFellerConstraint)` (base `r0`
    /// is `x0`), then builds `phi_` via `generateArguments()`.
    ///
    /// # Errors
    ///
    /// Fails if a base-CIR argument violates its constraint (see
    /// [`CoxIngersollRoss::new`]).
    pub fn new(
        term_structure: Handle<dyn YieldTermStructure>,
        theta: Real,
        k: Real,
        sigma: Real,
        x0: Rate,
        with_feller_constraint: bool,
    ) -> QlResult<ExtendedCoxIngersollRoss> {
        let base = CoxIngersollRoss::new(x0, theta, k, sigma, with_feller_constraint)?;
        let ts_model = TermStructureConsistentModel::new(term_structure);
        let mut model = ExtendedCoxIngersollRoss {
            base,
            ts_model,
            phi: NullParameter::new(),
        };
        model.generate_arguments();
        Ok(model)
    }
}

/// The fitted model rebuilds `phi_` from its arguments and the curve, both at
/// construction and on any later `set_params` (`extendedcoxingersollross.hpp:154`).
impl CalibratedModelHolder for ExtendedCoxIngersollRoss {
    fn calibrated_model(&self) -> &CalibratedModel {
        self.base.calibrated_model()
    }

    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        self.base.calibrated_model_mut()
    }

    fn generate_arguments(&mut self) {
        let law = FittingParameterValue {
            term_structure: self.ts_model.term_structure().clone(),
            theta: self.base.theta(),
            k: self.base.k(),
            sigma: self.base.sigma(),
            x0: self.base.x0(),
        };
        self.phi = TermStructureFittingParameter::new(Rc::new(law));
    }
}

impl OneFactorAffineModel for ExtendedCoxIngersollRoss {
    /// `A(t, s)` (`extendedcoxingersollross.cpp:53`): the base CIR `A` scaled so
    /// the model reprices the curve, using `P(.)`, the base `A`/`B`, `phi(t)`
    /// and `x0`.
    fn a(&self, t: Time, s: Time) -> Real {
        let curve = self
            .ts_model
            .term_structure()
            .current_link()
            .expect("the extended CIR model requires a non-empty term-structure handle");
        let pt = curve
            .discount(t, false)
            .expect("the extended CIR model's discount is well-defined on its curve");
        let ps = curve
            .discount(s, false)
            .expect("the extended CIR model's discount is well-defined on its curve");
        let x0 = self.base.x0();
        let base_a_ts = self.base.a(t, s);
        let b_ts = self.base.b(t, s);
        let phi_t = self.phi.value(t);
        let base_a_0t = self.base.a(0.0, t);
        let base_a_0s = self.base.a(0.0, s);
        let b_0t = self.base.b(0.0, t);
        let b_0s = self.base.b(0.0, s);
        base_a_ts * (b_ts * phi_t).exp() * (ps * base_a_0t * (-b_0t * x0).exp())
            / (pt * base_a_0s * (-b_0s * x0).exp())
    }

    /// `B(t, s)` inherited from the base CIR (C++ does not override it).
    fn b(&self, t: Time, maturity: Time) -> Real {
        self.base.b(t, maturity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::array::Array;
    use crate::math::interpolations::linear::Linear;
    use crate::shared::{Shared, shared};
    use crate::termstructures::yields::{FlatForward, ZeroCurve};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    fn flat_curve(rate: Rate) -> Handle<dyn YieldTermStructure> {
        let curve = FlatForward::with_rate(
            Date::new(15, Month::January, 2026),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        );
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>)
    }

    /// A sloped `ZeroCurve` (linear interpolation of continuous zero rates):
    /// reference 15 Jan 2026, Actual365Fixed, nodes at t = 0, 1, 2, 3 with
    /// continuous zero rates 0.03, 0.04, 0.05, 0.055 (so f(0) = 0.03). This is
    /// the exact curve the C++ oracle below was generated on.
    fn sloped_curve() -> Handle<dyn YieldTermStructure> {
        let reference = Date::new(15, Month::January, 2026);
        let dates = vec![
            reference,
            reference + 365,
            reference + 730,
            reference + 1095,
        ];
        let zeros = vec![0.03, 0.04, 0.05, 0.055];
        let curve = ZeroCurve::new(dates, zeros, Actual365Fixed::new(), Linear).unwrap();
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>)
    }

    #[test]
    fn extended_cir_reproduces_the_curve_forward_discount() {
        // testExtendedCoxIngersollRossDiscountFactor (shortratemodels.cpp:439):
        // ctor order (rTS, theta, k, sigma, x0) with theta=0.1, k=1.0, sigma=1e-4,
        // x0 = rate = 0.1; discountBond(1.5, 2.5, 0.1) reprices P(2.5)/P(1.5).
        let rate = 0.1;
        let handle = flat_curve(rate);
        let model =
            ExtendedCoxIngersollRoss::new(handle.clone(), rate, 1.0, 1e-4, rate, true).unwrap();

        let curve = handle.current_link().unwrap();
        let expected = curve.discount(2.5, false).unwrap() / curve.discount(1.5, false).unwrap();
        let calculated = model.discount_bond(1.5, 2.5, rate);

        assert!((expected - calculated).abs() < 1e-6);
    }

    #[test]
    fn discount_bond_matches_cpp_on_a_sloped_curve() {
        // Discriminating oracle for the term-structure fitting (#385): the #382
        // testExtendedCoxIngersollRossDiscountFactor oracle is degenerate (flat
        // curve, sigma=1e-4, x0=rate) and a mis-wire that returns base-CIR
        // discountBond still passes it. Here the numbers come from C++ QuantLib
        // 1.43.0 ExtendedCoxIngersollRoss on the sloped ZeroCurve above, at
        // non-degenerate params theta=0.05, k=0.5, sigma=0.03, x0=0.05 (x0 !=
        // f(0)=0.03). Generator params and full-precision output are cached below;
        // the base-vs-fitted gap at these points is O(1e-2), so a mis-wire fails
        // the 1e-8 oracle by six orders of magnitude.
        let handle = sloped_curve();
        let curve = handle.current_link().unwrap();
        let model =
            ExtendedCoxIngersollRoss::new(handle.clone(), 0.05, 0.5, 0.03, 0.05, true).unwrap();
        let base = CoxIngersollRoss::new(0.05, 0.05, 0.5, 0.03, true).unwrap();

        // C++ forwardRate(0,0,Continuous,NoFrequency): the dt=0.0001 shift makes
        // this 0.030001..., not exactly 0.03; the fitting reprices at r = f(0).
        let f0_cpp = 0.030_001_000_000_790_656;
        let f0 = curve
            .forward_rate(
                0.0,
                0.0,
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap()
            .rate();
        assert!((f0 - f0_cpp).abs() < 1e-12);

        // (maturity, C++ P(maturity)) - the reprice targets. discountBond(0,T,f0)
        // == P(T) for the fitted model but not for base CIR.
        let reprice = [
            (0.5, 0.982_652_235_665_073_2),
            (1.0, 0.960_789_439_152_323_2),
            (1.5, 0.934_727_720_616_027_5),
            (2.0, 0.904_837_418_035_959_5),
            (3.0, 0.847_893_704_087_915_9),
        ];
        for (t, p_cpp) in reprice {
            // Fixture-parity guard: the Rust curve must be the C++ curve.
            assert!((curve.discount(t, false).unwrap() - p_cpp).abs() < 1e-14);
            // Oracle: the fitted model reprices the curve.
            assert!((model.discount_bond(0.0, t, f0) - p_cpp).abs() < 1e-8);
            // A base-CIR mis-wire would land here instead - O(1e-2) off.
            assert!((base.discount_bond(0.0, t, f0) - p_cpp).abs() > 1e-3);
        }

        // A t>0 point pins the phi(t) term-structure interaction that a t=0-only
        // reprice leaves free. C++ discountBond(0.5, 2.5, f0).
        let ext_t_cpp = 0.903_817_675_549_668_5;
        assert!((model.discount_bond(0.5, 2.5, f0) - ext_t_cpp).abs() < 1e-8);
        assert!((base.discount_bond(0.5, 2.5, f0) - ext_t_cpp).abs() > 1e-3);
    }

    #[test]
    fn a_override_reprices_the_curve_where_base_cir_does_not() {
        // With x0 != the flat rate the base CIR no longer reprices the curve, so
        // this discriminates the A override from a mis-wire that delegates a() to
        // the base (the oracle's own params do not: there x0 = theta = rate makes
        // base and extended agree to ~1e-8).
        let rate = 0.08;
        let handle = flat_curve(rate);
        let extended =
            ExtendedCoxIngersollRoss::new(handle.clone(), 0.04, 0.3, 0.02, 0.05, true).unwrap();
        let base = CoxIngersollRoss::new(0.05, 0.04, 0.3, 0.02, true).unwrap();

        let p2 = handle.current_link().unwrap().discount(2.0, false).unwrap();
        let extended_db = extended.discount_bond(0.0, 2.0, rate);
        let base_db = base.discount_bond(0.0, 2.0, rate);

        assert!((extended_db - p2).abs() < 1e-9);
        assert!((base_db - p2).abs() > 1e-3);
    }

    #[test]
    fn set_params_rebuilds_phi_and_changes_the_price() {
        // The holder set_params must re-run generate_arguments, rebuilding phi_
        // from the new arguments; the repriced discount bond therefore moves.
        let rate = 0.08;
        let handle = flat_curve(rate);
        let mut model =
            ExtendedCoxIngersollRoss::new(handle.clone(), 0.04, 0.3, 0.02, 0.05, true).unwrap();
        let before = model.discount_bond(1.5, 2.5, rate);

        // arguments are (theta, k, sigma, r0); bump k and r0 so phi_ shifts.
        model
            .set_params(&Array::from([0.04, 0.6, 0.02, 0.07]))
            .unwrap();
        let after = model.discount_bond(1.5, 2.5, rate);

        assert!((before - after).abs() > 1e-6);
    }
}
