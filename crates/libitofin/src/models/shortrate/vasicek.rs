//! The Vasicek short-rate model.
//!
//! Port of `ql/models/shortrate/onefactormodels/vasicek.{hpp,cpp}`: the
//! mean-reverting short rate `dr_t = a(b - r_t) dt + sigma dW_t` with an
//! optional risk premium `lambda`, and its closed-form `A(t,T)` / `B(t,T)` that
//! feed the affine [`discount_bond`](OneFactorAffineModel::discount_bond).
//! [`Vasicek`] embeds a [`CalibratedModel`] holding its four
//! [`ConstantParameter`] arguments (`a`, `b`, `sigma`, `lambda`), matching
//! C++'s `OneFactorAffineModel(4)`; `r0` is a plain field.
//!
//! ## Deferred (omitted, not stubbed)
//!
//! C++'s `Vasicek` is a concrete class only because it also implements
//! `discountBondOption` and a `Dynamics`; the Rust trait surface (#377) excludes
//! both, so this slice ports neither:
//!
//! - `discountBondOption` (`vasicek.hpp:47`, `vasicek.cpp:60`) needs only
//!   `B`/`discountBond`/`blackFormula`, but no oracle in this batch exercises
//!   it; porting it now ships unpinned code. It lands with its own oracle later.
//! - `Vasicek::Dynamics` (`vasicek.hpp:78`), an Ornstein-Uhlenbeck
//!   `ShortRateDynamics`, is deferred with the short-rate dynamics in #377.
//!
//! ## Divergences from QuantLib
//!
//! - C++ holds `Parameter&` alias members `a_/b_/sigma_/lambda_`
//!   (`vasicek.hpp:63-66`) aliasing `arguments_[0..3]`, a self-referential
//!   borrow Rust cannot hold. The alias fields are not ported; the scalar
//!   inspectors [`a`](Vasicek::a)/[`b`](Vasicek::b)/[`sigma`](Vasicek::sigma)/
//!   [`lambda`](Vasicek::lambda) read `arguments()[i].value(0.0)` by index.
//! - C++'s value-defaulted constructor `Vasicek(0.05, 0.1, 0.05, 0.01, 0.0)` is
//!   surfaced as [`Default`]; [`new`](Vasicek::new) is fallible (D4) because the
//!   `PositiveConstraint` on `a`/`sigma` validates on construction, but the
//!   default values are statically feasible, so `Default` uses a documented
//!   `expect`.

use std::rc::Rc;

use crate::errors::QlResult;
use crate::math::optimization::constraint::{NoConstraint, PositiveConstraint};
use crate::models::model::{CalibratedModel, CalibratedModelHolder};
use crate::models::parameter::ConstantParameter;
use crate::models::shortrate::onefactormodel::OneFactorAffineModel;
use crate::types::{Rate, Real, Time};

/// The Vasicek model (`vasicek.hpp:42`).
pub struct Vasicek {
    model: CalibratedModel,
    r0: Rate,
}

impl Vasicek {
    /// `Vasicek(Rate r0, Real a, Real b, Real sigma, Real lambda)`
    /// (`vasicek.cpp:26`): binds `a`/`sigma` under a `PositiveConstraint` and
    /// `b`/`lambda` under `NoConstraint` (`vasicek.cpp:30-33`).
    ///
    /// # Errors
    ///
    /// Fails if `a` or `sigma` is not strictly positive.
    pub fn new(r0: Rate, a: Real, b: Real, sigma: Real, lambda: Real) -> QlResult<Vasicek> {
        let mut model = CalibratedModel::new(4);
        model.arguments_mut()[0] = ConstantParameter::new(a, Rc::new(PositiveConstraint))?;
        model.arguments_mut()[1] = ConstantParameter::new(b, Rc::new(NoConstraint))?;
        model.arguments_mut()[2] = ConstantParameter::new(sigma, Rc::new(PositiveConstraint))?;
        model.arguments_mut()[3] = ConstantParameter::new(lambda, Rc::new(NoConstraint))?;
        Ok(Vasicek { model, r0 })
    }

    /// The mean-reversion speed `a` (`vasicek.hpp:54`).
    pub fn a(&self) -> Real {
        self.model.arguments()[0].value(0.0)
    }

    /// The mean-reversion level `b` (`vasicek.hpp:55`).
    pub fn b(&self) -> Real {
        self.model.arguments()[1].value(0.0)
    }

    /// The volatility `sigma` (`vasicek.hpp:57`).
    pub fn sigma(&self) -> Real {
        self.model.arguments()[2].value(0.0)
    }

    /// The risk premium `lambda` (`vasicek.hpp:56`).
    pub fn lambda(&self) -> Real {
        self.model.arguments()[3].value(0.0)
    }

    /// The initial short rate `r0` (`vasicek.hpp:58`).
    pub fn r0(&self) -> Rate {
        self.r0
    }

    /// Refreshes the cached `r0` field (C++'s protected `r0_`, `vasicek.hpp:69`).
    ///
    /// The base model never rewrites `r0` (it is a constructor input), so this is
    /// the seam a term-structure-fitted subclass ([`HullWhite`], whose
    /// `generateArguments` sets `r0_ = zeroRate(0)`, `hullwhite.cpp:86`) uses to
    /// keep the inherited `r0()` reporting the current curve.
    ///
    /// [`HullWhite`]: super::hullwhite::HullWhite
    pub(crate) fn set_r0(&mut self, r0: Rate) {
        self.r0 = r0;
    }
}

/// Base Vasicek's `generateArguments()` is the no-op default (C++ does not
/// override it); the holder is implemented so a composing model
/// ([`HullWhite`](super::hullwhite::HullWhite)) reaches the embedded
/// [`CalibratedModel`] through the #381 seam - both to install its
/// `NullParameter` arguments and to read them back.
impl CalibratedModelHolder for Vasicek {
    fn calibrated_model(&self) -> &CalibratedModel {
        &self.model
    }

    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        &mut self.model
    }
}

impl Default for Vasicek {
    /// C++'s value-defaulted `Vasicek()` = `Vasicek(0.05, 0.1, 0.05, 0.01, 0.0)`
    /// (`vasicek.hpp:44`).
    fn default() -> Vasicek {
        Vasicek::new(0.05, 0.1, 0.05, 0.01, 0.0)
            .expect("QuantLib's default Vasicek parameters satisfy the positivity constraints")
    }
}

impl OneFactorAffineModel for Vasicek {
    /// `A(t, T)` (`vasicek.cpp:36`), with the small-mean-reversion branch when
    /// `a < sqrt(QL_EPSILON)`.
    fn a(&self, t: Time, maturity: Time) -> Real {
        let a = self.a();
        let sigma = self.sigma();
        if a < Real::EPSILON.sqrt() {
            let sigma2 = sigma * sigma;
            let tau = maturity - t;
            (-0.5 * self.lambda() * sigma * tau * tau + sigma2 * tau * tau * tau / 6.0).exp()
        } else {
            let sigma2 = sigma * sigma;
            let bt = OneFactorAffineModel::b(self, t, maturity);
            ((self.b() + self.lambda() * sigma / a - 0.5 * sigma2 / (a * a))
                * (bt - (maturity - t))
                - 0.25 * sigma2 * bt * bt / a)
                .exp()
        }
    }

    /// `B(t, T)` (`vasicek.cpp:52`), with the small-mean-reversion branch when
    /// `a < sqrt(QL_EPSILON)`.
    fn b(&self, t: Time, maturity: Time) -> Real {
        let a = self.a();
        if a < Real::EPSILON.sqrt() {
            maturity - t
        } else {
            (1.0 - (-a * (maturity - t)).exp()) / a
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_mean_reversion_discount_factor_matches_closed_form() {
        // testVasicekDiscountFactorForSmallMeanReversion (shortratemodels.cpp:469).
        let r0 = 0.05;
        let sigma = 0.01;
        let maturity = 1.0;
        let model = Vasicek::new(r0, 1e-12, 0.05, sigma, 0.0).unwrap();

        let expected =
            (-r0 * maturity + sigma * sigma * maturity * maturity * maturity / 6.0).exp();
        let calculated = model.discount_bond(0.0, maturity, r0);

        assert!((expected - calculated).abs() < 1e-12);
    }

    #[test]
    fn default_matches_the_cpp_value_defaulted_constructor() {
        let model = Vasicek::default();
        assert_eq!(model.a(), 0.1);
        assert_eq!(model.b(), 0.05);
        assert_eq!(model.sigma(), 0.01);
        assert_eq!(model.lambda(), 0.0);
        assert_eq!(model.r0(), 0.05);
    }

    #[test]
    fn large_mean_reversion_discount_factor_matches_closed_form() {
        // The a = 0.1 default drives the large-a A/B branch; value computed
        // independently from the same closed form.
        let model = Vasicek::default();
        let calculated = model.discount_bond(0.0, 1.0, 0.05);
        assert!((calculated - 0.951_244_142_965_253_6).abs() < 1e-15);
    }

    #[test]
    fn new_rejects_non_positive_mean_reversion() {
        assert!(Vasicek::new(0.05, -0.1, 0.05, 0.01, 0.0).is_err());
    }

    #[test]
    fn new_rejects_non_positive_volatility() {
        assert!(Vasicek::new(0.05, 0.1, 0.05, -0.01, 0.0).is_err());
    }
}
