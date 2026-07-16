//! One-factor affine short-rate models.
//!
//! Port of the closed-form parts of `ql/models/shortrate/onefactormodel.hpp`:
//! the affine `discountBond` payoff. A single-factor affine model prices a
//! zero-coupon bond in closed form as
//! `P(t, T, r_t) = A(t,T) e^{-B(t,T) r_t}` (`onefactormodel.hpp:136`), with
//! `A` and `B` supplied by the concrete model (Vasicek in #378).
//!
//! ## Trait surface (deferral by omission)
//!
//! Rust controls the trait surface, so the deferred machinery is omitted rather
//! than stubbed to a panic. The only affine method here is
//! [`discount_bond`](OneFactorAffineModel::discount_bond) plus the factor
//! overload; three C++ pure-virtuals are intentionally absent:
//!
//! - `AffineModel::discount(Time)` (`onefactormodel.cpp:97`) routes through the
//!   deferred `ShortRateDynamics`; `discountBond` takes an explicit rate and
//!   does not call it, and the #378 oracle calls `discountBond` directly.
//! - `AffineModel::discountBondOption` (`model.hpp:54`) is left pure-virtual by
//!   `OneFactorAffineModel` in C++ and needs the cumulative normal.
//! - `ShortRateModel::tree(const TimeGrid&)` (`model.hpp:144`) is the numerical
//!   lattice path.
//!
//! ## Collapsed intermediates
//!
//! C++'s `ShortRateModel` (`model.hpp:141`) and `OneFactorModel`
//! (`onefactormodel.hpp:38`) sit between `CalibratedModel` and this trait. Each
//! adds only a constructor forwarding the argument count (subsumed by
//! [`CalibratedModel::new`](crate::models::CalibratedModel::new)) plus deferred
//! virtuals: `ShortRateModel` adds `tree()`; `OneFactorModel` adds `dynamics()`
//! and its `tree()` implementation, both numerical-tree machinery. With their
//! only non-deferred content subsumed, they carry no Rust surface this slice; a
//! concrete affine model embeds a `CalibratedModel` and implements
//! [`OneFactorAffineModel`] directly.

use crate::math::array::Array;
use crate::types::{Rate, Real, Time};

/// Analytically tractable model (`ql/models/model.hpp:45`), reduced to its one
/// non-deferred method: the zero-coupon bond price as a function of the state
/// factors.
pub trait AffineModel {
    /// `discountBond(Time now, Time maturity, Array factors)` (`model.hpp:50`).
    fn discount_bond_factors(&self, now: Time, maturity: Time, factors: &Array) -> Real;
}

/// Single-factor affine base (`onefactormodel.hpp:126`).
///
/// A concrete model supplies `A(t,T)` and `B(t,T)`; the closed-form
/// [`discount_bond`](Self::discount_bond) and, through the [`AffineModel`]
/// blanket impl, the factor overload follow.
///
/// C++'s `A`/`B` are distinct from a model's scalar parameter inspectors
/// `a()`/`b()` only by case; lowercased in Rust they share a name. The two
/// coexist on a concrete model (the inherent 0-argument inspector wins
/// method-syntax resolution, and [`discount_bond`](Self::discount_bond)
/// resolves against the trait bound), but where a model's [`a`](Self::a)
/// cross-references its [`b`](Self::b) it must qualify the call as
/// `OneFactorAffineModel::b(self, t, maturity)`.
pub trait OneFactorAffineModel {
    /// `A(t, T)` (`onefactormodel.hpp:143`).
    fn a(&self, t: Time, maturity: Time) -> Real;

    /// `B(t, T)` (`onefactormodel.hpp:144`).
    fn b(&self, t: Time, maturity: Time) -> Real;

    /// `discountBond(Time now, Time maturity, Rate rate)`
    /// (`onefactormodel.hpp:136`): `A(now,maturity) e^{-B(now,maturity) rate}`.
    fn discount_bond(&self, now: Time, maturity: Time, rate: Rate) -> Real {
        self.a(now, maturity) * (-self.b(now, maturity) * rate).exp()
    }
}

/// `OneFactorAffineModel::discountBond(Array)` (`onefactormodel.hpp:132`)
/// delegates to the rate overload on the first factor.
impl<M: OneFactorAffineModel> AffineModel for M {
    fn discount_bond_factors(&self, now: Time, maturity: Time, factors: &Array) -> Real {
        self.discount_bond(now, maturity, factors[0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ConstantAffine {
        a: Real,
        b: Real,
    }

    impl OneFactorAffineModel for ConstantAffine {
        fn a(&self, _t: Time, _maturity: Time) -> Real {
            self.a
        }
        fn b(&self, _t: Time, _maturity: Time) -> Real {
            self.b
        }
    }

    #[test]
    fn discount_bond_is_a_times_exp_minus_b_rate() {
        let model = ConstantAffine { a: 0.9, b: 2.0 };
        let expected = 0.9 * (-2.0 * 0.05_f64).exp();
        assert_eq!(model.discount_bond(0.5, 1.5, 0.05), expected);
    }

    #[test]
    fn factor_overload_delegates_to_the_rate_overload() {
        let model = ConstantAffine { a: 0.9, b: 2.0 };
        assert_eq!(
            model.discount_bond_factors(0.5, 1.5, &Array::from([0.05])),
            model.discount_bond(0.5, 1.5, 0.05)
        );
    }
}
