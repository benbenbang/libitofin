//! Single-factor Longstaff-Schwartz regression bases.
//!
//! Mirrors QuantLib's monomials and six Gaussian weighted-polynomial families.
//! Multi-path tensor-product bases remain outside this single-factor module.

use crate::math::integrals::gaussianorthogonalpolynomial::{
    GaussHermitePolynomial, GaussHyperbolicPolynomial, GaussJacobiPolynomial,
    GaussLaguerrePolynomial, GaussianOrthogonalPolynomial,
};
use crate::types::{Real, Size};

/// Polynomial families of QuantLib's `LsmBasisSystem`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PolynomialType {
    /// Powers of the state.
    Monomial,
    /// Weighted Gauss-Laguerre polynomials.
    Laguerre,
    /// Weighted Gauss-Hermite polynomials.
    Hermite,
    /// Weighted hyperbolic polynomials.
    Hyperbolic,
    /// Gauss-Legendre polynomials.
    Legendre,
    /// Weighted first-kind Chebyshev polynomials.
    Chebyshev,
    /// Weighted second-kind Chebyshev polynomials.
    Chebyshev2nd,
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
        (0..=order)
            .map(|i| {
                Box::new(move |x| match poly_type {
                    PolynomialType::Monomial => monomial(i, x),
                    PolynomialType::Laguerre => GaussLaguerrePolynomial::new(0.0)
                        .expect("zero is a valid Laguerre exponent")
                        .weighted_value(i, x),
                    PolynomialType::Hermite => GaussHermitePolynomial::new(0.0)
                        .expect("zero is a valid Hermite exponent")
                        .weighted_value(i, x),
                    PolynomialType::Hyperbolic => GaussHyperbolicPolynomial.weighted_value(i, x),
                    PolynomialType::Legendre => {
                        GaussJacobiPolynomial::legendre().weighted_value(i, x)
                    }
                    PolynomialType::Chebyshev => {
                        GaussJacobiPolynomial::chebyshev().weighted_value(i, x)
                    }
                    PolynomialType::Chebyshev2nd => {
                        GaussJacobiPolynomial::chebyshev2nd().weighted_value(i, x)
                    }
                }) as Box<dyn Fn(Real) -> Real>
            })
            .collect()
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

    #[test]
    fn gaussian_families_match_independent_quantlib_weighted_values() {
        let families = [
            PolynomialType::Monomial,
            PolynomialType::Laguerre,
            PolynomialType::Hermite,
            PolynomialType::Hyperbolic,
            PolynomialType::Legendre,
            PolynomialType::Chebyshev,
            PolynomialType::Chebyshev2nd,
        ];
        for row in include_str!("../../../tests/fixtures/mc_american/basis.csv").lines() {
            let values: Vec<Real> = row.split(',').map(|v| v.parse().unwrap()).collect();
            let basis = LsmBasisSystem::path_basis_system(3, families[values[0] as usize]);
            for (f, expected) in basis.iter().zip(&values[2..]) {
                assert!((f(values[1]) - expected).abs() < 1e-13, "{row}");
            }
        }
    }
}
