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

use std::cell::Cell;

use crate::math::array::Array;
use crate::math::timegrid::TimeGrid;
use crate::methods::lattices::tree::Tree;
use crate::methods::lattices::treelattice::TreeLatticeImpl;
use crate::methods::lattices::trinomialtree::TrinomialTree;
use crate::shared::Shared;
use crate::stochasticprocess::StochasticProcess1D;
use crate::types::{Rate, Real, Size, Time};

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

/// Base description of a one-factor short-rate's dynamics
/// (`OneFactorModel::ShortRateDynamics`, `onefactormodel.hpp:54`).
///
/// A model maps between its Markov state variable `x` and the short rate `r`, and
/// exposes the risk-neutral process the state follows. The numerical tree
/// discretizes [`process`](Self::process) and reads [`short_rate`](Self::short_rate)
/// off each node to discount.
pub trait ShortRateDynamics {
    /// `variable(Time t, Rate r)` (`onefactormodel.hpp:61`): the state variable
    /// implied by the short rate `r` at `t`.
    fn variable(&self, t: Time, r: Rate) -> Real;

    /// `shortRate(Time t, Real variable)` (`onefactormodel.hpp:64`): the short
    /// rate implied by the state `variable` at `t`.
    fn short_rate(&self, t: Time, variable: Real) -> Rate;

    /// `process()` (`onefactormodel.hpp:67`): the risk-neutral dynamics of the
    /// state variable, the tree discretizes.
    fn process(&self) -> Shared<dyn StochasticProcess1D>;
}

/// Recombining trinomial tree discretizing a short-rate model's state variable
/// (`OneFactorModel::ShortRateTree`, `onefactormodel.hpp:75`).
///
/// C++'s `ShortRateTree` *is-a* `TreeLattice1D<ShortRateTree>` (CRTP self-
/// inheritance) and supplies the per-node discount off the model's dynamics.
/// Here it is instead the [`TreeLatticeImpl`] callback surface a
/// [`TreeLattice1D`](crate::methods::lattices::TreeLattice1D) induces over: it
/// embeds the [`TrinomialTree`] (which serves `size`/`descendant`/`probability`/
/// `underlying`) and the [`ShortRateDynamics`], and computes
/// [`discount`](TreeLatticeImpl::discount)`(i, index) =
/// exp(-(short_rate(t_i, x) + spread) * dt_i)` (`onefactormodel.hpp:91`). It
/// carries its own [`TimeGrid`] clone because the discount callback needs
/// `t_i`/`dt_i`, and the impl is nested inside - not above - the lattice that
/// owns the grid (the [`TreeLattice1D`](crate::methods::lattices::TreeLattice1D)
/// test fixtures do the same).
///
/// Only the plain build-up is ported; the Brent-fitting ctor and its `Helper`
/// (`onefactormodel.cpp:28,56`) belong to the deferred generic `tree()` path
/// (Hull-White's `tree()` fits `phi` in closed form instead).
pub struct ShortRateTree {
    tree: Shared<TrinomialTree>,
    dynamics: Shared<dyn ShortRateDynamics>,
    time_grid: TimeGrid,
    spread: Cell<Real>,
}

impl ShortRateTree {
    /// Plain build-up from a trinomial `tree` and short-rate `dynamics` over
    /// `time_grid` (`onefactormodel.cpp:80`); the spread starts at `0`.
    pub fn new(
        tree: Shared<TrinomialTree>,
        dynamics: Shared<dyn ShortRateDynamics>,
        time_grid: TimeGrid,
    ) -> Self {
        ShortRateTree {
            tree,
            dynamics,
            time_grid,
            spread: Cell::new(0.0),
        }
    }

    /// `setSpread(Spread)` (`onefactormodel.hpp:105`): an additive spread on the
    /// short rate, used by spread-adjusted engines (e.g. callable bonds). Held
    /// behind a [`Cell`] so it is settable through the shared, immutable impl the
    /// lattice exposes via `implementation()`.
    pub fn set_spread(&self, spread: Real) {
        self.spread.set(spread);
    }
}

impl TreeLatticeImpl for ShortRateTree {
    type Tree = TrinomialTree;

    fn tree(&self) -> &TrinomialTree {
        &self.tree
    }

    fn discount(&self, i: Size, index: Size) -> Real {
        let x = self.tree.underlying(i, index);
        let r = self.dynamics.short_rate(self.time_grid[i], x) + self.spread.get();
        (-r * self.time_grid.dt(i)).exp()
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
