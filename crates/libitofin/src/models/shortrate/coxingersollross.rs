//! The Cox-Ingersoll-Ross short-rate model.
//!
//! Port of `ql/models/shortrate/onefactormodels/coxingersollross.{hpp,cpp}`: the
//! square-root diffusion `dr_t = k(theta - r_t) dt + sigma sqrt(r_t) dW_t` and
//! its closed-form `A(t,T)` / `B(t,T)` feeding the affine
//! [`discount_bond`](OneFactorAffineModel::discount_bond). [`CoxIngersollRoss`]
//! embeds a [`CalibratedModel`] holding its four [`ConstantParameter`] arguments
//! (`theta`, `k`, `sigma`, `r0`), matching C++'s `OneFactorAffineModel(4)`.
//!
//! The volatility argument carries a [`VolatilityConstraint`] (the Feller
//! condition) when built with `with_feller_constraint`; like the model's other
//! constraints it is validated at construction and, in this closed-form slice,
//! never re-tested on the `discount_bond` path.
//!
//! ## Deferred (omitted, not stubbed)
//!
//! C++'s `CoxIngersollRoss` is a concrete class only because it also implements
//! the numerical-tree and option surface the Rust trait (#377) excludes:
//!
//! - `discountBondOption` (`coxingersollross.hpp:52`, `coxingersollross.cpp:83`)
//!   needs `NonCentralCumulativeChiSquareDistribution` (already on main at
//!   `math/distributions`), but no oracle in this batch exercises it; porting it
//!   now ships unpinned code. It lands with its own oracle later (#262 rule).
//! - `CoxIngersollRoss::Dynamics` (`coxingersollross.hpp:86`) and `tree`
//!   (`coxingersollross.cpp:125`) are the simulation/lattice path, deferred with
//!   the short-rate dynamics per #377.
//!
//! ## Divergences from QuantLib
//!
//! - C++ holds `Parameter&` alias members `theta_/k_/sigma_/r0_`
//!   (`coxingersollross.hpp:75-78`) aliasing `arguments_[0..3]`, a
//!   self-referential borrow Rust cannot hold. The alias fields are not ported;
//!   the inspectors [`theta`](CoxIngersollRoss::theta)/[`k`](CoxIngersollRoss::k)/
//!   [`sigma`](CoxIngersollRoss::sigma)/[`x0`](CoxIngersollRoss::x0) read
//!   `arguments()[i].value(0.0)` by index. `r0` is `arguments_[3]`, not a plain
//!   field (unlike Vasicek's `r0`), matching C++'s `x0() = r0_(0.0)`.
//! - C++'s inspectors are `protected`; Rust has no `protected`, so they are `pub`
//!   inherent methods (the Vasicek precedent), letting a composing model
//!   ([`ExtendedCoxIngersollRoss`](super::extendedcoxingersollross)) read them.
//! - C++'s value-defaulted constructor `CoxIngersollRoss(0.05, 0.1, 0.1, 0.1,
//!   true)` is surfaced as [`Default`]; [`new`](CoxIngersollRoss::new) is fallible
//!   (D4) because the constraints validate on construction, but the default
//!   values are statically feasible, so `Default` uses a documented `expect`.

use std::rc::Rc;

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::optimization::constraint::{Constraint, PositiveConstraint};
use crate::models::model::{CalibratedModel, CalibratedModelHolder};
use crate::models::parameter::ConstantParameter;
use crate::models::shortrate::onefactormodel::OneFactorAffineModel;
use crate::types::{Rate, Real, Time};

/// The Feller volatility constraint (`coxingersollross.cpp:26`): `sigma > 0` and
/// `sigma^2 < 2 k theta`, capturing `k` and `theta` at construction.
///
/// C++'s `VolatilityConstraint::Impl` overrides only `test`; its bounds are the
/// base `Constraint::Impl` unbounded defaults, which the [`Constraint`] trait
/// here already provides, so only [`test`](Constraint::test) is implemented.
pub struct VolatilityConstraint {
    k: Real,
    theta: Real,
}

impl VolatilityConstraint {
    /// `VolatilityConstraint(Real k, Real theta)` (`coxingersollross.cpp:38`).
    pub fn new(k: Real, theta: Real) -> VolatilityConstraint {
        VolatilityConstraint { k, theta }
    }
}

impl Constraint for VolatilityConstraint {
    fn test(&self, params: &Array) -> bool {
        let sigma = params[0];
        sigma > 0.0 && sigma * sigma < 2.0 * self.k * self.theta
    }
}

/// The Cox-Ingersoll-Ross model (`coxingersollross.hpp:44`).
pub struct CoxIngersollRoss {
    model: CalibratedModel,
}

impl CoxIngersollRoss {
    /// `CoxIngersollRoss(Rate r0, Real theta, Real k, Real sigma, bool
    /// withFellerConstraint)` (`coxingersollross.cpp:43`): binds `theta`/`k`/`r0`
    /// under a `PositiveConstraint` and `sigma` under a [`VolatilityConstraint`]
    /// (the Feller condition) when `with_feller_constraint`, else a
    /// `PositiveConstraint` (`coxingersollross.cpp:49-55`).
    ///
    /// # Errors
    ///
    /// Fails if `theta`, `k` or `r0` is not strictly positive, or if `sigma`
    /// violates its constraint (with the Feller condition, `sigma^2 >= 2 k
    /// theta`; otherwise `sigma <= 0`).
    pub fn new(
        r0: Rate,
        theta: Real,
        k: Real,
        sigma: Real,
        with_feller_constraint: bool,
    ) -> QlResult<CoxIngersollRoss> {
        let mut model = CalibratedModel::new(4);
        model.arguments_mut()[0] = ConstantParameter::new(theta, Rc::new(PositiveConstraint))?;
        model.arguments_mut()[1] = ConstantParameter::new(k, Rc::new(PositiveConstraint))?;
        let sigma_constraint: Rc<dyn Constraint> = if with_feller_constraint {
            Rc::new(VolatilityConstraint::new(k, theta))
        } else {
            Rc::new(PositiveConstraint)
        };
        model.arguments_mut()[2] = ConstantParameter::new(sigma, sigma_constraint)?;
        model.arguments_mut()[3] = ConstantParameter::new(r0, Rc::new(PositiveConstraint))?;
        Ok(CoxIngersollRoss { model })
    }

    /// The long-run level `theta` (`coxingersollross.hpp:67`).
    pub fn theta(&self) -> Real {
        self.model.arguments()[0].value(0.0)
    }

    /// The mean-reversion speed `k` (`coxingersollross.hpp:68`).
    pub fn k(&self) -> Real {
        self.model.arguments()[1].value(0.0)
    }

    /// The volatility `sigma` (`coxingersollross.hpp:69`).
    pub fn sigma(&self) -> Real {
        self.model.arguments()[2].value(0.0)
    }

    /// The initial short rate `x0 = r0` (`coxingersollross.hpp:70`).
    pub fn x0(&self) -> Real {
        self.model.arguments()[3].value(0.0)
    }
}

impl Default for CoxIngersollRoss {
    /// C++'s value-defaulted `CoxIngersollRoss()` = `CoxIngersollRoss(0.05, 0.1,
    /// 0.1, 0.1, true)` (`coxingersollross.hpp:46`).
    fn default() -> CoxIngersollRoss {
        CoxIngersollRoss::new(0.05, 0.1, 0.1, 0.1, true)
            .expect("QuantLib's default CIR parameters satisfy the Feller constraint")
    }
}

/// Base CIR's `generateArguments()` is the no-op default (C++ does not override
/// it); the holder is implemented so a composing model
/// ([`ExtendedCoxIngersollRoss`](super::extendedcoxingersollross)) reaches the
/// embedded [`CalibratedModel`] through the same #381 seam.
impl CalibratedModelHolder for CoxIngersollRoss {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.model
    }

    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.model
    }
}

impl OneFactorAffineModel for CoxIngersollRoss {
    /// `A(t, T)` (`coxingersollross.cpp:64`).
    fn a(&self, t: Time, maturity: Time) -> Real {
        let k = self.k();
        let theta = self.theta();
        let sigma = self.sigma();
        let sigma2 = sigma * sigma;
        let h = (k * k + 2.0 * sigma2).sqrt();
        let numerator = 2.0 * h * (0.5 * (k + h) * (maturity - t)).exp();
        let denominator = 2.0 * h + (k + h) * (((maturity - t) * h).exp() - 1.0);
        let value = (numerator / denominator).ln() * 2.0 * k * theta / sigma2;
        value.exp()
    }

    /// `B(t, T)` (`coxingersollross.cpp:74`).
    fn b(&self, t: Time, maturity: Time) -> Real {
        let k = self.k();
        let sigma = self.sigma();
        let h = (k * k + 2.0 * sigma * sigma).sqrt();
        let temp = ((maturity - t) * h).exp() - 1.0;
        let numerator = 2.0 * temp;
        let denominator = 2.0 * h + (k + h) * temp;
        numerator / denominator
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_matches_the_cpp_value_defaulted_constructor() {
        let model = CoxIngersollRoss::default();
        assert_eq!(model.theta(), 0.1);
        assert_eq!(model.k(), 0.1);
        assert_eq!(model.sigma(), 0.1);
        assert_eq!(model.x0(), 0.05);
    }

    #[test]
    fn discount_bond_matches_the_closed_form_at_default_params() {
        // Independently computed from A(t,T) e^{-B(t,T) r} at the default params.
        let model = CoxIngersollRoss::default();
        let calculated = model.discount_bond(0.0, 1.0, 0.05);
        assert!((calculated - 0.949_006_558_472_911).abs() < 1e-14);
    }

    #[test]
    fn discount_bond_matches_the_closed_form_with_non_trivial_a_and_b() {
        // theta=0.1, k=0.5, sigma=0.08 keeps A(0,2)=0.929, B(0,2)=1.261 both far
        // from their degenerate values, so this pins the full A and B transcription.
        let model = CoxIngersollRoss::new(0.05, 0.1, 0.5, 0.08, true).unwrap();
        let calculated = model.discount_bond(0.0, 2.0, 0.05);
        assert!((calculated - 0.872_385_885_028_455_4).abs() < 1e-14);
    }

    #[test]
    fn discount_bond_is_one_over_a_zero_length_period() {
        let model = CoxIngersollRoss::default();
        assert!((model.discount_bond(0.0, 0.0, 0.05) - 1.0).abs() < 1e-15);
    }

    #[test]
    fn volatility_constraint_enforces_the_feller_condition() {
        // sigma^2 < 2 k theta with k = 1.0, theta = 0.1 => sigma^2 < 0.2.
        let c = VolatilityConstraint::new(1.0, 0.1);
        assert!(c.test(&Array::from([0.1])));
        assert!(!c.test(&Array::from([0.5])));
        assert!(!c.test(&Array::from([-0.1])));
    }

    #[test]
    fn new_rejects_a_volatility_violating_the_feller_condition() {
        // k = 1.0, theta = 0.1 => the Feller bound is sigma^2 < 0.2; sigma = 0.5
        // gives sigma^2 = 0.25, so the constructor must err.
        assert!(CoxIngersollRoss::new(0.1, 0.1, 1.0, 0.5, true).is_err());
    }

    #[test]
    fn without_the_feller_constraint_a_large_volatility_is_accepted() {
        // The same sigma = 0.5 that the Feller path rejects passes when only
        // positivity is required.
        assert!(CoxIngersollRoss::new(0.1, 0.1, 1.0, 0.5, false).is_ok());
    }

    #[test]
    fn new_rejects_non_positive_theta_k_or_r0() {
        assert!(CoxIngersollRoss::new(0.1, -0.1, 1.0, 0.1, true).is_err());
        assert!(CoxIngersollRoss::new(0.1, 0.1, -1.0, 0.1, true).is_err());
        assert!(CoxIngersollRoss::new(-0.1, 0.1, 1.0, 0.1, true).is_err());
    }
}
