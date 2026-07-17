//! The Hull-White one-factor short-rate model.
//!
//! Port of `ql/models/shortrate/onefactormodels/hullwhite.{hpp,cpp}`: the
//! extended-Vasicek short rate `dr_t = (theta(t) - a r_t) dt + sigma dW_t`,
//! whose deterministic drift `theta(t)` is fitted so the model reprices the
//! input [`YieldTermStructure`](crate::termstructures::yieldtermstructure::YieldTermStructure)
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
//! - `discountBondOption` (`hullwhite.hpp:55`/`:60`, `hullwhite.cpp:90`/`:108`),
//!   the Jamshidian bond option, needs `blackFormula` and has no oracle in this
//!   batch; porting it now ships unpinned code (#262 rule).
//! - `HullWhite::Dynamics` (`hullwhite.hpp:107`), `tree` (`hullwhite.cpp:43`),
//!   `FixedReversion` (`hullwhite.hpp:80`) and the `JamshidianSwaptionEngine`
//!   swaption oracles (`testCachedHullWhite*`) are the simulation/lattice and
//!   calibration paths, deferred with the short-rate dynamics per #377.
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
use crate::patterns::observable::Observer;
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
    use crate::math::interpolations::linear::Linear;
    use crate::shared::{Shared, shared};
    use crate::termstructures::yields::ZeroCurve;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

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
    fn r0_is_the_zero_rate_at_the_short_end() {
        // generateArguments caches r0 = zeroRate(0); C++ hwgen.cpp prints
        // 0.020000499999160704 on this fixture (the DT-shifted short-end zero).
        let handle = sloped_curve();
        let model = HullWhite::new(handle, 0.05, 0.01).unwrap();
        assert!((model.borrow().r0() - 0.020_000_499_999_160_704).abs() < 1e-12);
    }

    #[test]
    fn futures_convexity_bias_reproduces_the_kirikos_novak_table() {
        // testFuturesConvexityBias (shortratemodels.cpp:407-438). G. Kirikos, D.
        // Novak, "Convexity Conundrums", Risk Magazine, March 1997. The five rows
        // exercise all three branches of the body: general a (0.03), the
        // small-a threshold (1e-4, below QL_EPSILON only where 2t makes it bite),
        // a == 0, and deltaT -> 0 (T = 5.001 and T = t = 5.0).
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
}
