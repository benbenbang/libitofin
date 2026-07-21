//! The Hull-White one-factor short-rate model.
//!
//! Port of `ql/models/shortrate/onefactormodels/hullwhite.{hpp,cpp}`: the
//! extended-Vasicek short rate `dr_t = (theta(t) - a r_t) dt + sigma dW_t`,
//! whose deterministic drift `theta(t)` is fitted so the model reprices the
//! input [`YieldTermStructure`]
//! exactly. This slice ports the static futures convexity bias, the closed-form
//! curve-fit [`HullWhite`] model (the `A(t,T)` override and the cached, curve-fed
//! `r0`), and their non-engine oracles.
//!
//! ## Trait re-wire (the composition-loses-dispatch trap)
//!
//! C++ `HullWhite : public Vasicek` overrides only `A(t,T)`; `B` and
//! `discountBond` are inherited unchanged. Rust cannot subclass, so [`HullWhite`]
//! composes a base [`Vasicek`] and gets its own [`OneFactorAffineModel`] impl:
//! [`a`](OneFactorAffineModel::a) is the curve-fit override, [`b`](OneFactorAffineModel::b)
//! delegates to the embedded Vasicek's `B`, and
//! [`discount_bond`](OneFactorAffineModel::discount_bond) is the inherited trait
//! default (which then dispatches to *this* type's `a`/`b`). Delegating `a` to the
//! base would price the plain Vasicek bond rather than the curve-fitted one; the
//! `discount_bond_matches_cpp_hull_white` oracle pins the difference at `t > 0`.
//!
//! ## Deferred (omitted, not stubbed)
//!
//! - `HullWhite::FittingParameter` (`hullwhite.hpp:128`) and the `phi_` member it
//!   builds in `generateArguments` (`hullwhite.cpp:87`) are the analytical
//!   fitting law `phi(t) = f(t,t) + 0.5*temp^2`, `temp = a < sqrt(QL_EPSILON) ?
//!   sigma*t : sigma*(1-e^{-at})/a` (`hullwhite.hpp:136-143`; note this is the
//!   `sqrt(QL_EPSILON)` guard, distinct from `convexity_bias`'s plain
//!   `QL_EPSILON`). `phi_` feeds only `dynamics()` (`hullwhite.hpp:162`); `A(t,T)`
//!   reads the curve's forward directly and never touches it, so `phi_` and its
//!   fitting law are deferred with the dynamics/tree path (#377). Consequently
//!   [`generate_arguments`](crate::models::CalibratedModelHolder::generate_arguments)
//!   here refreshes only the cached `r0`; the `phi_` rebuild lands with `dynamics()`.
//! - `HullWhite::Dynamics` (`hullwhite.hpp:107`), `tree` (`hullwhite.cpp:43`) and
//!   `FixedReversion` (`hullwhite.hpp:80`) are the simulation/lattice paths,
//!   deferred with the short-rate dynamics per #377. The analytic
//!   `testCachedHullWhite` calibration oracle (`shortratemodels.cpp:83`) is ported
//!   in `hull_white_calibrates_to_the_cached_swaption_values` below (#399), on the
//!   Jamshidian-engine path (#392); `testCachedHullWhiteFixedReversion`
//!   (`shortratemodels.cpp:155`) and `testCachedHullWhite2`
//!   (`shortratemodels.cpp:229`) are ported below in
//!   `hull_white_calibrates_with_fixed_reversion` and
//!   `hull_white_calibrates_without_start_delay` (#400). The `FixedReversion`
//!   fixed/free mask reaches `calibrate` as the inlined `vec![true, false]`
//!   (the static `hullwhite.hpp:80` helper itself is not ported).
//!
//! ## Divergences from QuantLib
//!
//! - C++'s ctor sets `b_ = NullParameter()` and `lambda_ = NullParameter()`,
//!   replacing `arguments_[1]`/`arguments_[3]` so `params()` flattens to
//!   `[a, sigma]` (HW's calibration surface). Here [`HullWhite::new`] installs the
//!   same [`NullParameter`]s through the embedded Vasicek's
//!   [`CalibratedModelHolder`] seam.
//! - C++ multiply-inherits `Vasicek` and `TermStructureConsistentModel`; here both
//!   are composed as fields. Unlike Extended CIR, Hull-White *does*
//!   [`register_with_term_structure`], so a relink re-runs `generate_arguments`
//!   and the cached `r0` tracks the newly linked curve.
//! - [`HullWhite::new`] returns a [`SharedMut`] because the term-structure
//!   observer holds a weak back-reference to the model and must be stashed after
//!   the model is shared (C++ registers `this` inside the constructor).

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::interestrate::Compounding;
use crate::models::model::{
    CalibratedModel, CalibratedModelHolder, TermStructureConsistentModel,
    register_with_term_structure,
};
use crate::models::parameter::NullParameter;
use crate::models::shortrate::onefactormodel::OneFactorAffineModel;
use crate::models::shortrate::vasicek::Vasicek;
use crate::option::OptionType;
use crate::patterns::observable::Observer;
use crate::pricingengines::blackformula::black_formula;
use crate::require;
use crate::shared::{SharedMut, shared_mut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// `HullWhite::convexityBias(Real futuresPrice, Time t, Time T, Real sigma,
/// Real a)` (`hullwhite.cpp:134`): the futures convexity bias (the difference
/// between the futures-implied rate and the forward rate), computed as in G.
/// Kirikos, D. Novak, "Convexity Conundrums", Risk Magazine, March 1997.
///
/// `t` and `T` are year fractions in the deposit day counter, and `futures_price`
/// is the futures' market price. C++ maps this static member to a namespaced free
/// function here (no instance is needed).
///
/// The small-mean-reversion guard is plain `QL_EPSILON` (`hullwhite.cpp:150`),
/// distinct from the `sqrt(QL_EPSILON)` guard on `Vasicek::B` and on the
/// (deferred) fitting law: `temp(x) = a < QL_EPSILON ? x : (1 - e^{-ax})/a`.
///
/// # Errors
///
/// Mirrors the five `QL_REQUIRE`s (`hullwhite.cpp:139-148`): fails on a negative
/// futures price, a negative `t`, `T < t`, a negative `sigma`, or a negative `a`.
#[allow(clippy::neg_cmp_op_on_partial_ord)]
pub fn convexity_bias(
    futures_price: Real,
    t: Time,
    maturity: Time,
    sigma: Real,
    a: Real,
) -> QlResult<Rate> {
    require!(
        futures_price >= 0.0,
        "negative futures price ({futures_price}) not allowed"
    );
    require!(t >= 0.0, "negative t ({t}) not allowed");
    require!(
        maturity >= t,
        "T ({maturity}) must not be less than t ({t})"
    );
    require!(sigma >= 0.0, "negative sigma ({sigma}) not allowed");
    require!(a >= 0.0, "negative a ({a}) not allowed");

    let temp = |x: Real| {
        if a < Real::EPSILON {
            x
        } else {
            (1.0 - (-a * x).exp()) / a
        }
    };

    let delta_t = maturity - t;
    let temp_delta_t = temp(delta_t);
    let half_sigma_square = sigma * sigma / 2.0;
    let lambda = temp(2.0 * t) * temp_delta_t;
    let temp_t = temp(t);
    let phi = temp_t * temp_t;
    let z = half_sigma_square * (lambda + phi);
    let future_rate = (100.0 - futures_price) / 100.0;
    if delta_t < Real::EPSILON {
        Ok(z)
    } else {
        Ok((1.0 - (-z * temp_delta_t).exp()) * (future_rate + 1.0 / delta_t))
    }
}

/// The single-factor Hull-White (extended Vasicek) model
/// (`hullwhite.hpp:46`).
///
/// Composes a base [`Vasicek`] (C++ subclasses it) and a
/// [`TermStructureConsistentModel`] (the fitted curve handle), and observes that
/// handle so a relink refreshes the cached `r0`.
pub struct HullWhite {
    base: Vasicek,
    ts_model: TermStructureConsistentModel,
    /// Keeps the term-structure observer alive: the handle holds only a weak
    /// back-reference to it (see [`register_with_term_structure`]), so dropping
    /// it here would unregister the model. Never read directly.
    #[allow(dead_code)]
    ts_observer: Option<SharedMut<dyn Observer>>,
}

impl HullWhite {
    /// `HullWhite(const Handle<YieldTermStructure>&, Real a, Real sigma)`
    /// (`hullwhite.cpp:31`): chains `Vasicek(forwardRate(0,0), a, b=0, sigma,
    /// lambda=0)`, replaces the `b`/`lambda` arguments with [`NullParameter`]s,
    /// runs `generateArguments()` to seed the cached `r0`, then registers as an
    /// observer of `term_structure`.
    ///
    /// Returns a [`SharedMut`] so the observer, which holds a weak reference back
    /// to the model, can be stashed after the model is shared (C++ registers
    /// `this` from inside the constructor).
    ///
    /// # Errors
    ///
    /// Fails if `term_structure` is empty, if its forward rate at `0` is not
    /// well-defined, or if `a`/`sigma` violate the Vasicek positivity
    /// constraints.
    pub fn new(
        term_structure: Handle<dyn YieldTermStructure>,
        a: Real,
        sigma: Real,
    ) -> QlResult<SharedMut<HullWhite>> {
        let forward = term_structure
            .current_link()?
            .forward_rate(
                0.0,
                0.0,
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )?
            .rate();

        let mut base = Vasicek::new(forward, a, 0.0, sigma, 0.0)?;
        base.calibrated_model_mut().arguments_mut()[1] = NullParameter::new();
        base.calibrated_model_mut().arguments_mut()[3] = NullParameter::new();

        let ts_model = TermStructureConsistentModel::new(term_structure.clone());
        let mut model = HullWhite {
            base,
            ts_model,
            ts_observer: None,
        };
        model.generate_arguments();

        let shared = shared_mut(model);
        let observer = register_with_term_structure(&shared, &term_structure);
        shared.borrow_mut().ts_observer = Some(observer);
        Ok(shared)
    }

    /// The fitted initial short rate `r0` (`vasicek.hpp:58`), cached from
    /// `zeroRate(0)` by `generateArguments` and refreshed on every relink; C++
    /// inherits `Vasicek::r0()`.
    pub fn r0(&self) -> Rate {
        self.base.r0()
    }

    /// The fitted-curve handle (`termStructure()`, `model.hpp:77`), from which the
    /// [`JamshidianSwaptionEngine`](crate::pricingengines::swaption::JamshidianSwaptionEngine)
    /// (#392) reads the reference date and day counter it turns the swaption's
    /// dates into year fractions with.
    pub fn term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        self.ts_model.term_structure()
    }

    /// The fitted curve's discount factor `P(t)`
    /// (`termStructure()->discount(t)`), read live for the bond-option payoffs.
    fn discount(&self, t: Time) -> QlResult<Real> {
        self.ts_model
            .term_structure()
            .current_link()?
            .discount(t, false)
    }

    /// `discountBondOption(Option::Type, Real strike, Time maturity, Time
    /// bondMaturity)` (`hullwhite.cpp:90`): the price of a European option, with
    /// exercise at `maturity`, on a zero-coupon bond maturing at `bondMaturity`.
    ///
    /// The Jamshidian decomposition prices it as a [`black_formula`] on the
    /// forward bond price `f = P(bondMaturity)` struck at `k =
    /// P(maturity)*strike`, with volatility `v = sigma B(maturity, bondMaturity)
    /// sqrt(0.5 (1 - e^{-2 a maturity}) / a)` (`hullwhite.cpp:96-101`; the
    /// `a < sqrt(QL_EPSILON)` branch replaces the last factor with `sqrt(maturity)`,
    /// the same small-mean-reversion guard as `B`). Because `k` and `f` already
    /// fold in the curve discounts, `black_formula` is called with `discount = 1.0`
    /// and `displacement = 0.0` (C++'s 4-argument `blackFormula` defaults).
    ///
    /// # Errors
    ///
    /// Fails if the fitted curve is unlinked or its discount is undefined, or if
    /// `black_formula` rejects its arguments.
    pub fn discount_bond_option(
        &self,
        option_type: OptionType,
        strike: Real,
        maturity: Time,
        bond_maturity: Time,
    ) -> QlResult<Real> {
        let a = self.base.a();
        let sigma = self.base.sigma();
        let b = OneFactorAffineModel::b(self, maturity, bond_maturity);
        let v = if a < Real::EPSILON.sqrt() {
            sigma * b * maturity.sqrt()
        } else {
            sigma * b * (0.5 * (1.0 - (-2.0 * a * maturity).exp()) / a).sqrt()
        };
        let f = self.discount(bond_maturity)?;
        let k = self.discount(maturity)? * strike;
        black_formula(option_type, k, f, v, 1.0, 0.0)
    }

    /// `discountBondOption(Option::Type, Real strike, Time maturity, Time
    /// bondStart, Time bondMaturity)` (`hullwhite.cpp:108`): the option on a
    /// forward-starting zero-coupon bond spanning `[bondStart, bondMaturity]`,
    /// exercised at `maturity`. This is the overload the
    /// `JamshidianSwaptionEngine` (#392) calls.
    ///
    /// It is *not* the 4-argument overload with `bondStart` dropped: C++'s base
    /// `AffineModel::discountBondOption` (`model.hpp:151`) delegates to the 4-arg
    /// and ignores `bondStart`, but Hull-White overrides that default with a
    /// distinct volatility (`hullwhite.cpp:114-127`),
    /// `v = sigma / (a sqrt(2 a)) * sqrt(max(c, 0))`, where
    /// `c = e^{-2a(bondStart-maturity)} - e^{-2a bondStart}
    ///      - 2 (e^{-a(bondStart+bondMaturity-2 maturity)} - e^{-a(bondStart+bondMaturity)})
    ///      + e^{-2a(bondMaturity-maturity)} - e^{-2a bondMaturity}`.
    /// `c` is analytically non-negative but is floored at `0` to guard the `sqrt`
    /// against tiny negative rounding (`hullwhite.cpp:123-126`). The
    /// `a < sqrt(QL_EPSILON)` branch uses `sigma B(bondStart, bondMaturity)
    /// sqrt(maturity)`. The forward price is `f = P(bondMaturity)` struck at
    /// `k = P(bondStart)*strike` (note `bondStart`, not `maturity`).
    ///
    /// # Errors
    ///
    /// Fails if the fitted curve is unlinked or its discount is undefined, or if
    /// `black_formula` rejects its arguments.
    pub fn discount_bond_option_with_start(
        &self,
        option_type: OptionType,
        strike: Real,
        maturity: Time,
        bond_start: Time,
        bond_maturity: Time,
    ) -> QlResult<Real> {
        let a = self.base.a();
        let sigma = self.base.sigma();
        let v = if a < Real::EPSILON.sqrt() {
            sigma * OneFactorAffineModel::b(self, bond_start, bond_maturity) * maturity.sqrt()
        } else {
            let c = (-2.0 * a * (bond_start - maturity)).exp()
                - (-2.0 * a * bond_start).exp()
                - 2.0
                    * ((-a * (bond_start + bond_maturity - 2.0 * maturity)).exp()
                        - (-a * (bond_start + bond_maturity)).exp())
                + (-2.0 * a * (bond_maturity - maturity)).exp()
                - (-2.0 * a * bond_maturity).exp();
            sigma / (a * (2.0 * a).sqrt()) * c.max(0.0).sqrt()
        };
        let f = self.discount(bond_maturity)?;
        let k = self.discount(bond_start)? * strike;
        black_formula(option_type, k, f, v, 1.0, 0.0)
    }
}

/// `HullWhite::generateArguments()` (`hullwhite.cpp:85`): refreshes the cached
/// `r0` from `zeroRate(0)` on every construction and relink. The C++ method also
/// rebuilds `phi_` (the fitting law for `dynamics()`), deferred here with the
/// dynamics/tree path (see the module deferral note); `A(t,T)` reads the curve's
/// forward live and never uses `phi_`.
impl CalibratedModelHolder for HullWhite {
    fn calibrated_model(&self) -> &CalibratedModel {
        self.base.calibrated_model()
    }

    fn calibrated_model_mut(&mut self) -> &mut CalibratedModel {
        self.base.calibrated_model_mut()
    }

    fn generate_arguments(&mut self) {
        let zero = self
            .ts_model
            .term_structure()
            .current_link()
            .expect("the Hull-White model requires a non-empty term-structure handle")
            .zero_rate(0.0, Compounding::Continuous, Frequency::NoFrequency, false)
            .expect("the Hull-White zero rate at t=0 is well-defined on its curve")
            .rate();
        self.base.set_r0(zero);
    }
}

impl OneFactorAffineModel for HullWhite {
    /// `A(t, T)` (`hullwhite.cpp:75`): the curve-fit override
    /// `exp(B(t,T) f(t) - 0.25 (sigma B(t,T))^2 B(0,2t)) P(T)/P(t)`, with `B`
    /// inherited from the base Vasicek and `f(t)` the instantaneous forward.
    fn a(&self, t: Time, maturity: Time) -> Real {
        let curve = self
            .ts_model
            .term_structure()
            .current_link()
            .expect("the Hull-White model requires a non-empty term-structure handle");
        let discount1 = curve
            .discount(t, false)
            .expect("the Hull-White model's discount is well-defined on its curve");
        let discount2 = curve
            .discount(maturity, false)
            .expect("the Hull-White model's discount is well-defined on its curve");
        let forward = curve
            .forward_rate(t, t, Compounding::Continuous, Frequency::NoFrequency, false)
            .expect("the Hull-White model's forward rate is well-defined on its curve")
            .rate();
        let b = OneFactorAffineModel::b(self, t, maturity);
        let temp = self.base.sigma() * b;
        let value = b * forward - 0.25 * temp * temp * OneFactorAffineModel::b(self, 0.0, 2.0 * t);
        value.exp() * discount2 / discount1
    }

    /// `B(t, T)` inherited from the base Vasicek (C++ does not override it).
    fn b(&self, t: Time, maturity: Time) -> Real {
        OneFactorAffineModel::b(&self.base, t, maturity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::RateAveraging;
    use crate::handle::RelinkableHandle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::math::array::Array;
    use crate::math::interpolations::linear::Linear;
    use crate::math::optimization::endcriteria::{EndCriteria, EndCriteriaType};
    use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
    use crate::models::calibrationhelper::{
        BlackCalibrationHelper, CalibrationErrorType, CalibrationHelper,
    };
    use crate::models::model::{calibrate, calibration_value};
    use crate::models::shortrate::SwaptionHelper;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::JamshidianSwaptionEngine;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::{Shared, shared};
    use crate::termstructures::volatility::VolatilityType;
    use crate::termstructures::yields::{FlatForward, ZeroCurve};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    /// The sloped fixture shared with the C++ generator (scratchpad `hwgen.cpp`):
    /// an `InterpolatedZeroCurve<Linear>` through continuous zeros on
    /// `Actual365Fixed`, reference date 15 Jan 2026.
    fn sloped_curve() -> Handle<dyn YieldTermStructure> {
        let dates = vec![
            Date::new(15, Month::January, 2026),
            Date::new(15, Month::January, 2027),
            Date::new(15, Month::January, 2028),
            Date::new(15, Month::January, 2029),
            Date::new(15, Month::January, 2031),
        ];
        let zeros = vec![0.02, 0.025, 0.03, 0.033, 0.04];
        let curve = ZeroCurve::new(dates, zeros, Actual365Fixed::new(), Linear).unwrap();
        Handle::new(shared(curve) as Shared<dyn YieldTermStructure>)
    }

    #[test]
    fn discount_bond_matches_cpp_hull_white() {
        // No standalone C++ HW discountBond test exists, so these are cached from
        // QuantLib 1.43.0-dev HullWhite(h, a=0.05, sigma=0.01) on the sloped
        // fixture (scratchpad hwgen.cpp, std::setprecision(17)). The t>0 points
        // pin the variance term 0.25*(sigma B(t,T))^2 B(0,2t) that a t=0-only
        // oracle leaves identically zero.
        let handle = sloped_curve();
        let curve = handle.current_link().unwrap();

        // Fixture parity: pin P(T) against C++ so a curve mismatch surfaces here,
        // not inside the model.
        assert!((curve.discount(2.0, false).unwrap() - 0.941_764_533_584_248_7).abs() < 1e-14);
        assert!((curve.discount(3.0, false).unwrap() - 0.905_764_980_659_064).abs() < 1e-14);
        assert!((curve.discount(5.0, false).unwrap() - 0.818_770_008_233_211_2).abs() < 1e-14);

        let model = HullWhite::new(handle, 0.05, 0.01).unwrap();
        let m = model.borrow();

        assert!((m.discount_bond(0.0, 2.0, 0.03) - 0.924_010_757_799_029_2).abs() < 1e-10);
        assert!((m.discount_bond(1.0, 3.0, 0.03) - 0.928_534_477_203_476).abs() < 1e-10);
        assert!((m.discount_bond(2.0, 5.0, 0.025) - 0.900_808_567_926_092_5).abs() < 1e-10);
    }

    #[test]
    fn discount_bond_option_matches_cpp_hull_white() {
        // No standalone C++ HW discountBondOption test exists, so these are cached
        // from QuantLib 1.43.0-dev on the sloped fixture (scratchpad hwdbo.cpp,
        // std::setprecision(17)). Both overloads are pinned: a 4-arg-only oracle
        // would let a wrong 5-arg (or one that drops bondStart and delegates to the
        // 4-arg, C++'s base default) pass. Strikes straddle the forward bond price
        // so both the Call and Put ITM/OTM branches of black_formula are exercised.
        let handle = sloped_curve();
        let curve = handle.current_link().unwrap();

        // Fixture parity: pin every P(t) the option payoffs read against C++.
        assert!((curve.discount(1.0, false).unwrap() - 0.975_309_912_028_332_6).abs() < 1e-14);
        assert!((curve.discount(2.0, false).unwrap() - 0.941_764_533_584_248_7).abs() < 1e-14);
        assert!((curve.discount(3.0, false).unwrap() - 0.905_764_980_659_064).abs() < 1e-14);
        assert!((curve.discount(5.0, false).unwrap() - 0.818_770_008_233_211_2).abs() < 1e-14);

        let model = HullWhite::new(handle, 0.05, 0.01).unwrap();
        let m = model.borrow();

        // 4-arg (maturity=1, bondMaturity=3): forward bond price P(3)/P(1) = 0.9287.
        // K=0.9 is ITM for the call / OTM for the put; K=0.95 the reverse.
        assert!(
            (m.discount_bond_option(OptionType::Call, 0.9, 1.0, 3.0)
                .unwrap()
                - 0.028_295_945_329_515_248)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option(OptionType::Call, 0.95, 1.0, 3.0)
                .unwrap()
                - 0.000_912_556_023_061_549_3)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option(OptionType::Put, 0.9, 1.0, 3.0)
                .unwrap()
                - 0.000_309_885_495_950_584_07)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option(OptionType::Put, 0.95, 1.0, 3.0)
                .unwrap()
                - 0.021_691_991_790_913_523)
                .abs()
                < 1e-10
        );

        // 5-arg (maturity=1, bondStart=2, bondMaturity=5): the engine-realistic
        // geometry maturity < bondStart < bondMaturity. Forward price P(5)/P(2) =
        // 0.8694; K=0.85 ITM call / OTM put, K=0.9 the reverse.
        assert!(
            (m.discount_bond_option_with_start(OptionType::Call, 0.85, 1.0, 2.0, 5.0)
                .unwrap()
                - 0.020_478_099_685_837_1)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option_with_start(OptionType::Call, 0.9, 1.0, 2.0, 5.0)
                .unwrap()
                - 0.000_903_569_724_417_981_9)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option_with_start(OptionType::Put, 0.85, 1.0, 2.0, 5.0)
                .unwrap()
                - 0.002_207_944_999_237_256_6)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option_with_start(OptionType::Put, 0.9, 1.0, 2.0, 5.0)
                .unwrap()
                - 0.029_721_641_717_030_61)
                .abs()
                < 1e-10
        );
    }

    #[test]
    fn discount_bond_option_small_mean_reversion_matches_cpp() {
        // a = 1e-13 < sqrt(QL_EPSILON) drives the small-mean-reversion branch of
        // BOTH overloads (v uses sqrt(maturity), not the general variance term).
        // Cached from hwdbo.cpp with HullWhite(h, 1e-13, 0.01).
        let model = HullWhite::new(sloped_curve(), 1e-13, 0.01).unwrap();
        let m = model.borrow();

        assert!(
            (m.discount_bond_option(OptionType::Call, 0.9, 1.0, 3.0)
                .unwrap()
                - 0.028_431_518_729_836_437)
                .abs()
                < 1e-10
        );
        assert!(
            (m.discount_bond_option_with_start(OptionType::Call, 0.85, 1.0, 2.0, 5.0)
                .unwrap()
                - 0.021_443_413_387_680_313)
                .abs()
                < 1e-10
        );
    }

    #[test]
    fn r0_is_the_zero_rate_at_the_short_end() {
        // generateArguments caches r0 = zeroRate(0); C++ hwgen.cpp prints
        // 0.020000499999160704 on this fixture (the DT-shifted short-end zero).
        let handle = sloped_curve();
        let model = HullWhite::new(handle, 0.05, 0.01).unwrap();
        assert!((model.borrow().r0() - 0.020_000_499_999_160_704).abs() < 1e-12);
    }

    /// A flat continuously-compounded curve at `rate` on `Actual365Fixed`,
    /// reference date 19 May 2026, as a yield-curve pointee.
    fn flat(rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            Date::new(19, Month::May, 2026),
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    #[test]
    fn r0_updates_when_the_term_structure_relinks() {
        // testHullWhiteUpdatesR0WhenTermStructureRelinks (shortratemodels.cpp:58).
        // The model observes its curve handle (register_with_term_structure, #387);
        // a relink re-runs generate_arguments so the cached r0 tracks the new
        // curve. Confirmed by stubbing that dropping the registration makes this
        // assertion fail (r0 stays at the 0.02 curve's zero rate). D5: explicit
        // reference dates, no Settings.
        let rh: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::new(flat(0.02));
        let model = HullWhite::new(rh.handle(), 0.1, 0.01).unwrap();

        let new_curve = flat(0.05);
        rh.link_to(new_curve.clone());

        let expected = new_curve
            .forward_rate(
                0.0,
                0.0,
                Compounding::Continuous,
                Frequency::NoFrequency,
                false,
            )
            .unwrap()
            .rate();
        assert!((model.borrow().r0() - expected).abs() < 1e-12);
    }

    #[test]
    fn futures_convexity_bias_reproduces_the_kirikos_novak_table() {
        // testFuturesConvexityBias (shortratemodels.cpp:407-438). G. Kirikos, D.
        // Novak, "Convexity Conundrums", Risk Magazine, March 1997. The five rows
        // exercise all three branches of the body: the general temp branch (a =
        // 0.03 and a = 1e-4, both far above QL_EPSILON), the small-a temp branch
        // temp(x) = x (a = 0.0 < QL_EPSILON), and the deltaT < QL_EPSILON return
        // branch (T = t = 5.0; T = 5.001 stays on the general return).
        let future_quote = 94.0;
        let sigma = 0.015;
        let t = 5.0;
        let tolerance = 1e-7;
        let future_implied_rate = (100.0 - future_quote) / 100.0;

        for (maturity, a, expected_forward) in [
            (5.25, 0.03, 0.0573037),
            (5.25, 1e-4, 0.0568627),
            (5.25, 0.0, 0.0568611),
            (5.001, 0.03, 0.0575736),
            (5.0, 0.03, 0.0575747),
        ] {
            let bias = convexity_bias(future_quote, t, maturity, sigma, a).unwrap();
            let calculated_forward = future_implied_rate - bias;
            assert!(
                (calculated_forward - expected_forward).abs() < tolerance,
                "T={maturity}, a={a}: got {calculated_forward}, expected {expected_forward}"
            );
        }
    }

    #[test]
    fn convexity_bias_rejects_out_of_range_inputs_with_the_cpp_messages() {
        assert_eq!(
            convexity_bias(-1.0, 5.0, 5.25, 0.015, 0.03)
                .unwrap_err()
                .message(),
            "negative futures price (-1) not allowed"
        );
        assert_eq!(
            convexity_bias(94.0, -5.0, 5.25, 0.015, 0.03)
                .unwrap_err()
                .message(),
            "negative t (-5) not allowed"
        );
        assert_eq!(
            convexity_bias(94.0, 5.0, 4.0, 0.015, 0.03)
                .unwrap_err()
                .message(),
            "T (4) must not be less than t (5)"
        );
        assert_eq!(
            convexity_bias(94.0, 5.0, 5.25, -0.015, 0.03)
                .unwrap_err()
                .message(),
            "negative sigma (-0.015) not allowed"
        );
        assert_eq!(
            convexity_bias(94.0, 5.0, 5.25, 0.015, -0.03)
                .unwrap_err()
                .message(),
            "negative a (-0.03) not allowed"
        );
    }

    /// Calibrates Hull-White to the five cached swaptions for one coupon
    /// convention, returning the fitted `[a, sigma]`, the end criterion and the
    /// residual `f(a)` (`shortratemodels.cpp:108-135`).
    fn calibrate_cached_hull_white(using_at_par: bool) -> (Array, EndCriteriaType, Real) {
        let today = Date::new(15, Month::February, 2002);
        let settlement = Date::new(19, Month::February, 2002);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        settings.set_using_at_par_coupons(using_at_par);

        let term_structure: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::with_rate(
                settlement,
                0.04875825,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);

        let model = HullWhite::new(term_structure.clone(), 0.1, 0.01).unwrap();
        let index = shared(Euribor::six_months(
            term_structure.clone(),
            Shared::clone(&settings),
        ));
        let engine = shared_mut(JamshidianSwaptionEngine::new(SharedMut::clone(&model)))
            as SharedMut<dyn PricingEngine>;

        let data = [
            (1, 5, 0.1148),
            (2, 4, 0.1108),
            (3, 3, 0.1070),
            (4, 2, 0.1021),
            (5, 1, 0.1000),
        ];
        let helpers: Vec<SharedMut<dyn CalibrationHelper>> = data
            .into_iter()
            .map(|(start, length, volatility)| {
                let vol: Handle<dyn Quote> =
                    Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
                let mut helper = SwaptionHelper::new(
                    Period::new(start, TimeUnit::Years),
                    Period::new(length, TimeUnit::Years),
                    vol,
                    Shared::clone(&index),
                    Period::new(1, TimeUnit::Years),
                    Thirty360::with_convention(Convention::BondBasis),
                    Actual360::new(),
                    term_structure.clone(),
                    CalibrationErrorType::RelativePriceError,
                    None,
                    1.0,
                    VolatilityType::ShiftedLognormal,
                    0.0,
                    None,
                    RateAveraging::Compound,
                );
                helper
                    .base_mut()
                    .set_pricing_engine(SharedMut::clone(&engine));
                shared_mut(helper) as SharedMut<dyn CalibrationHelper>
            })
            .collect();

        let mut method = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false);
        let end_criteria = EndCriteria::new(10000, Some(100), 1e-6, 1e-8, Some(1e-8)).unwrap();
        calibrate(
            &model,
            &helpers,
            &mut method,
            &end_criteria,
            None,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();

        let params = model.borrow().calibrated_model().params();
        let ec_type = model.borrow().calibrated_model().end_criteria();
        let residual = calibration_value(&model, &params, &helpers).unwrap();
        (params, ec_type, residual)
    }

    #[test]
    fn hull_white_calibrates_to_the_cached_swaption_values() {
        // testCachedHullWhite (shortratemodels.cpp:83-153): calibrate Hull-White
        // to five co-terminal swaptions through the analytic Jamshidian engine via
        // Levenberg-Marquardt and reproduce the cached a/sigma to 1.3e-5
        // (shortratemodels.cpp:129-132). usingAtParCoupons (shortratemodels.cpp:86,
        // 126-130) gates the cached values; both arms are pinned, as the
        // discountingswapengine / blackcapfloorengine oracles do. The residual
        // bound is a Rust-side strengthening (C++ prints f(a) but does not assert
        // it): a 2-parameter fit to 5 swaptions cannot be exact, so the observed
        // relative-price-error RSS is ~0.1158, bounded below 0.2 with margin -
        // this pins that the fit converged rather than diverged, not a tight fit.
        let tolerance = 1.3e-5;
        for (using_at_par, cached_a, cached_sigma) in [
            (true, 0.0464041, 0.00579912),
            (false, 0.0463679, 0.00579831),
        ] {
            let (params, ec_type, residual) = calibrate_cached_hull_white(using_at_par);

            assert!(
                (params[0] - cached_a).abs() < tolerance,
                "par={using_at_par}: a = {} vs cached {cached_a} (error {})",
                params[0],
                (params[0] - cached_a).abs()
            );
            assert!(
                (params[1] - cached_sigma).abs() < tolerance,
                "par={using_at_par}: sigma = {} vs cached {cached_sigma} (error {})",
                params[1],
                (params[1] - cached_sigma).abs()
            );
            assert!(
                ec_type.succeeded(),
                "par={using_at_par}: end criteria {ec_type} did not converge"
            );
            assert!(
                residual.is_finite() && residual < 0.2,
                "par={using_at_par}: residual f(a) = {residual} not finite/bounded"
            );
        }
    }

    /// Calibrates Hull-White to the five cached swaptions under a chosen initial
    /// reversion `a0`, index fixing-day count, end criterion and fixed-parameter
    /// mask, returning the fitted `[a, sigma]`, the end criterion and the
    /// residual `f(a)`. Generalises [`calibrate_cached_hull_white`] for the
    /// fixed-reversion (`shortratemodels.cpp:155-227`) and no-start-delay
    /// (`shortratemodels.cpp:229-306`) fixtures, whose only fixture deltas are
    /// `a0`, the index fixing days, the end criterion and the fix-parameters
    /// vector.
    fn calibrate_cached_hull_white_variant(
        using_at_par: bool,
        a0: Real,
        zero_fixing_days: bool,
        end_criteria: EndCriteria,
        fix_parameters: Vec<bool>,
    ) -> (Array, EndCriteriaType, Real) {
        let today = Date::new(15, Month::February, 2002);
        let settlement = Date::new(19, Month::February, 2002);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        settings.set_using_at_par_coupons(using_at_par);

        let term_structure: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::with_rate(
                settlement,
                0.04875825,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);

        let model = HullWhite::new(term_structure.clone(), a0, 0.01).unwrap();
        // testCachedHullWhite2 (shortratemodels.cpp:246-249) rebuilds Euribor6M
        // with zero fixing days ("Euribor 6m with zero fixing days"), so the
        // swaptions carry no start delay; the fixed-reversion case keeps the
        // standard two-day index.
        let euribor = Euribor::six_months(term_structure.clone(), Shared::clone(&settings));
        let index = if zero_fixing_days {
            shared(IborIndex::new(
                euribor.family_name().to_string(),
                euribor.tenor(),
                0,
                euribor.currency().clone(),
                euribor.fixing_calendar(),
                euribor.business_day_convention(),
                euribor.end_of_month(),
                euribor.day_counter().clone(),
                term_structure.clone(),
                Shared::clone(&settings),
            ))
        } else {
            shared(euribor)
        };
        let engine = shared_mut(JamshidianSwaptionEngine::new(SharedMut::clone(&model)))
            as SharedMut<dyn PricingEngine>;

        let data = [
            (1, 5, 0.1148),
            (2, 4, 0.1108),
            (3, 3, 0.1070),
            (4, 2, 0.1021),
            (5, 1, 0.1000),
        ];
        let helpers: Vec<SharedMut<dyn CalibrationHelper>> = data
            .into_iter()
            .map(|(start, length, volatility)| {
                let vol: Handle<dyn Quote> =
                    Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
                let mut helper = SwaptionHelper::new(
                    Period::new(start, TimeUnit::Years),
                    Period::new(length, TimeUnit::Years),
                    vol,
                    Shared::clone(&index),
                    Period::new(1, TimeUnit::Years),
                    Thirty360::with_convention(Convention::BondBasis),
                    Actual360::new(),
                    term_structure.clone(),
                    CalibrationErrorType::RelativePriceError,
                    None,
                    1.0,
                    VolatilityType::ShiftedLognormal,
                    0.0,
                    None,
                    RateAveraging::Compound,
                );
                helper
                    .base_mut()
                    .set_pricing_engine(SharedMut::clone(&engine));
                shared_mut(helper) as SharedMut<dyn CalibrationHelper>
            })
            .collect();

        let mut method = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false);
        calibrate(
            &model,
            &helpers,
            &mut method,
            &end_criteria,
            None,
            Vec::new(),
            fix_parameters,
        )
        .unwrap();

        let params = model.borrow().calibrated_model().params();
        let ec_type = model.borrow().calibrated_model().end_criteria();
        let residual = calibration_value(&model, &params, &helpers).unwrap();
        (params, ec_type, residual)
    }

    #[test]
    fn hull_white_calibrates_with_fixed_reversion() {
        // testCachedHullWhiteFixedReversion (shortratemodels.cpp:155-227):
        // calibrate Hull-White starting from a = 0.05 with the reversion FIXED
        // (HullWhite::FixedReversion() == {true, false}, hullwhite.hpp:80-84,
        // passed at shortratemodels.cpp:195) so only sigma is fit, through
        // calibrate()'s Projection/ProjectedConstraint path (Batch W1). This is
        // the real end-to-end oracle for that fixed/free projection: a must stay
        // pinned at 0.05 (Projection::include reinstates the fixed seed), and
        // sigma reproduce the cached value to 1.0e-5 (shortratemodels.cpp:206).
        // usingAtParCoupons (shortratemodels.cpp:200-204) gates sigma: PAR
        // 0.00585858 (:203), INDEXED 0.00585835 (:201). Both coupon arms pinned.
        let tolerance = 1.0e-5;
        for (using_at_par, cached_sigma) in [(true, 0.00585858), (false, 0.00585835)] {
            let end_criteria = EndCriteria::new(1000, Some(500), 1e-8, 1e-8, Some(1e-8)).unwrap();
            let (params, ec_type, residual) = calibrate_cached_hull_white_variant(
                using_at_par,
                0.05,
                false,
                end_criteria,
                vec![true, false],
            );

            assert!(
                (params[0] - 0.05).abs() < 1e-15,
                "par={using_at_par}: reversion must stay fixed at 0.05, got {}",
                params[0]
            );
            assert!(
                (params[1] - cached_sigma).abs() < tolerance,
                "par={using_at_par}: sigma = {} vs cached {cached_sigma} (error {})",
                params[1],
                (params[1] - cached_sigma).abs()
            );
            assert!(
                ec_type.succeeded(),
                "par={using_at_par}: end criteria {ec_type} did not converge"
            );
            assert!(
                residual.is_finite() && residual < 0.2,
                "par={using_at_par}: residual f(a) = {residual} not finite/bounded"
            );
        }
    }

    #[test]
    fn hull_white_calibrates_without_start_delay() {
        // testCachedHullWhite2 (shortratemodels.cpp:229-306): the
        // testCachedHullWhite fixture rebuilt with a zero-fixing-days Euribor6M
        // (shortratemodels.cpp:246-249), so the swaptions carry no start delay.
        // Default HullWhite (a = 0.1, sigma = 0.01). Cached a/sigma reproduce to
        // 1.0e-5 (shortratemodels.cpp:285); usingAtParCoupons
        // (shortratemodels.cpp:280-284) gates both arms: PAR a = 0.0482063,
        // sigma = 0.00582687 (:283); INDEXED a = 0.0481608, sigma = 0.00582493
        // (:281). The cached values predate the Jamshidian engine's expiry/start
        // delay accounting (shortratemodels.cpp:276-279), which the zero-delay
        // index sidesteps. The residual bound mirrors testCachedHullWhite.
        let tolerance = 1.0e-5;
        for (using_at_par, cached_a, cached_sigma) in [
            (true, 0.0482063, 0.00582687),
            (false, 0.0481608, 0.00582493),
        ] {
            let end_criteria = EndCriteria::new(10000, Some(100), 1e-6, 1e-8, Some(1e-8)).unwrap();
            let (params, ec_type, residual) = calibrate_cached_hull_white_variant(
                using_at_par,
                0.1,
                true,
                end_criteria,
                Vec::new(),
            );

            assert!(
                (params[0] - cached_a).abs() < tolerance,
                "par={using_at_par}: a = {} vs cached {cached_a} (error {})",
                params[0],
                (params[0] - cached_a).abs()
            );
            assert!(
                (params[1] - cached_sigma).abs() < tolerance,
                "par={using_at_par}: sigma = {} vs cached {cached_sigma} (error {})",
                params[1],
                (params[1] - cached_sigma).abs()
            );
            assert!(
                ec_type.succeeded(),
                "par={using_at_par}: end criteria {ec_type} did not converge"
            );
            assert!(
                residual.is_finite() && residual < 0.2,
                "par={using_at_par}: residual f(a) = {residual} not finite/bounded"
            );
        }
    }
}
