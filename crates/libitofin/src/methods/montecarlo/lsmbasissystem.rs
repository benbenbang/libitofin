//! Basis systems for Longstaff-Schwartz early-exercise Monte Carlo.
//!
//! Port of `ql/methods/montecarlo/lsmbasissystem.{hpp,cpp}`: the family of
//! functions the backward induction regresses the continuation value against.
//! [`LsmBasisSystem::path_basis_system`] returns the `order + 1` monomials
//! `{1, x, x^2, ..., x^order}` (`lsmbasissystem.cpp:109-114`).
//!
//! Deferred, rejected visibly rather than silently ignored:
//! - **the Gauss families** (`Laguerre`, `Hermite`, `Hyperbolic`, `Legendre`,
//!   `Chebyshev`, `Chebyshev2nd` - `lsmbasissystem.hpp:39-41`, built from
//!   `GaussianQuadrature::weightedValue` at `lsmbasissystem.cpp:116-151`).
//!   [`PolynomialType`] therefore carries the one ported variant rather than a
//!   full set with unreachable arms. `MakeMCAmericanEngine` defaults to
//!   `Monomial` (`mcamericanengine.hpp:136`), and the only test that varies the
//!   family indexes its array by `0*(i*3+j)%5`
//!   (`test-suite/mclongstaffschwartzengine.cpp:193`), whose `0*` factor pins
//!   every iteration to element 0, `Monomial` (`:161-164`). No oracle in this
//!   stack reaches the others.
//! - **`multiPathBasisSystem`** (`lsmbasissystem.hpp:46-47`), the tensor
//!   product over a `MultiPath` for multi-asset early exercise.

use crate::types::{Real, Size};

/// The polynomial family a basis system is built from
/// (`lsmbasissystem.hpp:38-41`).
///
/// Only `Monomial` is ported; see the module docs for the deferred Gauss
/// families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolynomialType {
    /// `x^i`, the family `MCAmericanEngine` uses.
    Monomial,
}

/// `x^order`, evaluated as C++ does by iterated multiplication rather than
/// `powi` (`lsmbasissystem.cpp:41-50`).
///
/// The two agree to the last bit only up to order 3; binary exponentiation
/// rounds differently above that, and the regression is the oracle's, so this
/// follows the C++ arithmetic exactly.
fn monomial(order: Size, x: Real) -> Real {
    let mut ret = 1.0;
    for _ in 0..order {
        ret *= x;
    }
    ret
}

/// The basis systems of `lsmbasissystem.hpp:37-48`.
pub struct LsmBasisSystem;

impl LsmBasisSystem {
    /// The `order + 1` basis functions over a single-factor state
    /// (`lsmbasissystem.cpp:109-114`): for `PolynomialType::Monomial`,
    /// `{1, x, x^2, ..., x^order}`.
    pub fn path_basis_system(
        order: Size,
        poly_type: PolynomialType,
    ) -> Vec<Box<dyn Fn(Real) -> Real>> {
        match poly_type {
            PolynomialType::Monomial => (0..=order)
                .map(|i| Box::new(move |x: Real| monomial(i, x)) as Box<dyn Fn(Real) -> Real>)
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: `pathBasisSystem(3, Monomial)` is `{1, x, x^2, x^3}`
    /// (`lsmbasissystem.cpp:109-114` over the `MonomialFct` of `:41-50`).
    ///
    /// The expectations are written as repeated multiplication, matching the
    /// C++ loop, so the comparison is exact rather than one ulp away from
    /// `powi`.
    #[test]
    fn monomials_of_order_three_evaluate_to_the_powers() {
        let basis = LsmBasisSystem::path_basis_system(3, PolynomialType::Monomial);
        assert_eq!(basis.len(), 4, "order n yields n+1 functions");

        for x in [0.5, 1.7, -2.0] {
            let expected = [1.0, x, x * x, x * x * x];
            for (i, (f, want)) in basis.iter().zip(expected).enumerate() {
                assert!(
                    (f(x) - want).abs() <= 1e-15 * want.abs(),
                    "basis[{i}]({x}) = {}, expected {want}",
                    f(x)
                );
            }
        }
    }

    /// The degenerate end of the range: order 0 is the single constant
    /// function, and it is 1 everywhere rather than the identity. A basis
    /// built as `{x^1, ..., x^(order+1)}` would pass the order-3 check on
    /// length but fail here.
    #[test]
    fn order_zero_is_the_constant_one() {
        let basis = LsmBasisSystem::path_basis_system(0, PolynomialType::Monomial);
        assert_eq!(basis.len(), 1);
        for x in [0.0, 0.5, -3.25] {
            assert_eq!(basis[0](x), 1.0);
        }
    }

    /// The basis is what [`GeneralLinearLeastSquares`] consumes, so it must
    /// typecheck as its `&[F] where F: Fn(Real) -> Real` slice and recover the
    /// coefficients of a polynomial sampled exactly.
    ///
    /// [`GeneralLinearLeastSquares`]: crate::math::generallinearleastsquares::GeneralLinearLeastSquares
    #[test]
    fn the_basis_drives_a_least_squares_fit() {
        use crate::math::generallinearleastsquares::GeneralLinearLeastSquares;

        let basis = LsmBasisSystem::path_basis_system(2, PolynomialType::Monomial);
        let x = [-2.0, -1.0, 0.0, 1.0, 2.0, 3.0];
        let y: Vec<Real> = x.iter().map(|x| 3.0 - 2.0 * x + 0.5 * x * x).collect();

        let fit = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap();
        let c = fit.coefficients();

        assert!((c[0] - 3.0).abs() < 1e-12, "constant term {}", c[0]);
        assert!((c[1] + 2.0).abs() < 1e-12, "linear term {}", c[1]);
        assert!((c[2] - 0.5).abs() < 1e-12, "quadratic term {}", c[2]);
    }
}
