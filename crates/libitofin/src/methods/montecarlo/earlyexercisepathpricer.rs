//! Base trait for early-exercise single-path pricers.
//!
//! Port of `ql/methods/montecarlo/earlyexercisepathpricer.hpp:62-76`: the three
//! things a Longstaff-Schwartz backward induction needs from an instrument at
//! each exercise date - the intrinsic value of exercising now, the regressor
//! state to condition the continuation value on, and the basis system to
//! regress that continuation value against.
//!
//! Divergences from `earlyexercisepathpricer.hpp`, all deliberate:
//! - **`State` is an associated type, not an `EarlyExerciseTraits` lookup**:
//!   C++ derives `StateType` from the path type through a traits class
//!   specialized per path (`:34-54`, `Real` for `Path`, `Array` for
//!   `MultiPath`). Rust states the same fact directly on the implementor, which
//!   drops the traits indirection and the dummy primary template that fails to
//!   compile for an unknown path type (`:34-37`).
//! - **`TimeType` and `ValueType` are fixed to [`Size`] and [`Real`]**: the C++
//!   template parameters (`:62-63`) default to exactly those and no instrument
//!   in QuantLib instantiates them otherwise.
//! - **`operator()` is named `value`**: an exercise value is not a call.

use crate::types::{Real, Size};

/// Values an early-exercisable instrument along a single realized path.
///
/// `P` is the path type produced by the generator ([`Path`](super::Path) or
/// [`MultiPath`](super::MultiPath)); `t` indexes the exercise date within it.
pub trait EarlyExercisePathPricer<P> {
    /// The regressor observed at an exercise date: `Real` for a single-factor
    /// [`Path`](super::Path), an [`Array`](crate::math::array::Array) for a
    /// basket over a [`MultiPath`](super::MultiPath)
    /// (`earlyexercisepathpricer.hpp:41,50`).
    type State;

    /// The intrinsic value of exercising at index `t` along `path`, the C++
    /// `operator()` (`earlyexercisepathpricer.hpp:68-69`).
    fn value(&self, path: &P, t: Size) -> Real;

    /// The state to regress the continuation value against at index `t`
    /// (`earlyexercisepathpricer.hpp:71-72`).
    fn state(&self, path: &P, t: Size) -> Self::State;

    /// The basis functions spanning the continuation-value regression
    /// (`earlyexercisepathpricer.hpp:73-75`), typically
    /// [`LsmBasisSystem::path_basis_system`](super::LsmBasisSystem::path_basis_system).
    fn basis_system(&self) -> Vec<Box<dyn Fn(Self::State) -> Real>>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::array::Array;
    use crate::math::timegrid::TimeGrid;
    use crate::methods::montecarlo::{LsmBasisSystem, Path, PolynomialType};

    /// The American-put shape every Longstaff-Schwartz instrument takes: the
    /// exercise value is the payoff of the spot at `t`, the regressor is that
    /// same spot, and the basis system is the monomials.
    struct AmericanPut {
        strike: Real,
        order: Size,
    }

    impl EarlyExercisePathPricer<Path> for AmericanPut {
        type State = Real;

        fn value(&self, path: &Path, t: Size) -> Real {
            (self.strike - path[t]).max(0.0)
        }

        fn state(&self, path: &Path, t: Size) -> Real {
            path[t]
        }

        fn basis_system(&self) -> Vec<Box<dyn Fn(Real) -> Real>> {
            LsmBasisSystem::path_basis_system(self.order, PolynomialType::Monomial)
        }
    }

    fn path(values: [Real; 3]) -> Path {
        let grid = TimeGrid::new(1.0, 2).unwrap();
        Path::new(grid, Array::from(values)).unwrap()
    }

    /// The trait is usable as written: an implementor over [`Path`] with
    /// `State = Real` reports the intrinsic value, the regressor, and a basis
    /// system whose arity matches the requested order.
    #[test]
    fn a_path_implementor_reports_value_state_and_basis() {
        let pricer = AmericanPut {
            strike: 40.0,
            order: 2,
        };
        let p = path([36.0, 44.0, 38.0]);

        assert_eq!(pricer.value(&p, 0), 4.0);
        assert_eq!(
            pricer.value(&p, 1),
            0.0,
            "an out-of-the-money put is worth 0"
        );
        assert_eq!(pricer.value(&p, 2), 2.0);

        assert_eq!(pricer.state(&p, 1), 44.0);
        assert_eq!(pricer.basis_system().len(), 3);
    }

    /// Object safety: the backward induction holds one pricer behind a pointer,
    /// so the trait must survive erasure with `State` named.
    #[test]
    fn the_trait_is_object_safe() {
        let pricer: Box<dyn EarlyExercisePathPricer<Path, State = Real>> = Box::new(AmericanPut {
            strike: 40.0,
            order: 1,
        });
        assert_eq!(pricer.value(&path([36.0, 44.0, 38.0]), 0), 4.0);
    }
}
