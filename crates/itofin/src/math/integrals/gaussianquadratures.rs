//! Gaussian quadrature rules built from orthogonal polynomials.
//!
//! Port of `GaussianQuadrature` from `ql/math/integrals/gaussianquadratures.
//! {hpp,cpp}`: given an orthogonal-polynomial family, the abscissas are the
//! eigenvalues of the symmetric tridiagonal Jacobi matrix of its recurrence
//! coefficients and the weights follow from the first eigenvector components
//! (Golub and Welsch). The resulting rule approximates the plain integral of
//! `f` over the family's support - the weight function is folded into the
//! quadrature weights.

use crate::math::array::Array;
use crate::math::integrals::Integrator;
use crate::math::integrals::gaussianorthogonalpolynomial::{
    GaussHermitePolynomial, GaussHyperbolicPolynomial, GaussJacobiPolynomial,
    GaussLaguerrePolynomial, GaussianOrthogonalPolynomial,
};
use crate::math::matrixutilities::tqreigendecomposition::{
    EigenVectorCalculation, ShiftStrategy, TqrEigenDecomposition,
};
use crate::require;
use crate::types::{Real, Size};

/// An `n`-point Gaussian quadrature rule for a given orthogonal-polynomial
/// family.
#[derive(Clone, Debug)]
pub struct GaussianQuadrature {
    x: Array,
    w: Array,
}

impl GaussianQuadrature {
    /// Builds the `n`-point rule for the polynomial family `poly` by solving
    /// the eigenproblem of its Jacobi matrix.
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn new<P>(n: Size, poly: &P) -> crate::errors::QlResult<Self>
    where
        P: GaussianOrthogonalPolynomial + ?Sized,
    {
        require!(n > 0, "at least one integration point required");

        let mut diag = Array::with_size(n);
        let mut sub = Array::with_size(n - 1);
        diag[0] = poly.alpha(0);
        for i in 1..n {
            diag[i] = poly.alpha(i);
            sub[i - 1] = poly.beta(i).sqrt();
        }

        let tqr = TqrEigenDecomposition::new(
            &diag,
            &sub,
            EigenVectorCalculation::OnlyFirstRowEigenVector,
            ShiftStrategy::Overrelaxation,
        );

        let x = tqr.eigenvalues().clone();
        let ev = tqr.eigenvectors();
        let mu_0 = poly.mu_0();
        let w: Array = (0..n)
            .map(|i| mu_0 * ev[(0, i)] * ev[(0, i)] / poly.w(x[i]))
            .collect();

        Ok(GaussianQuadrature { x, w })
    }

    /// Integrates `f` over the family's support, summing in QuantLib's order
    /// (last node first) for bit-comparable results.
    pub fn integrate<F: FnMut(Real) -> Real>(&self, mut f: F) -> Real {
        let mut sum = 0.0;
        for i in (0..self.order()).rev() {
            sum += self.w[i] * f(self.x[i]);
        }
        sum
    }

    /// The number of integration points.
    pub fn order(&self) -> Size {
        self.x.size()
    }

    /// The quadrature weights.
    pub fn weights(&self) -> &Array {
        &self.w
    }

    /// The abscissas (QuantLib's `x()`), sorted in decreasing order.
    pub fn abscissas(&self) -> &Array {
        &self.x
    }
}

impl GaussianQuadrature {
    /// Gauss-Legendre integration over `[-1, 1]` (QuantLib's
    /// `GaussLegendreIntegration`).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn legendre(n: Size) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussJacobiPolynomial::legendre())
    }

    /// Gauss-Chebyshev integration (first kind) over `[-1, 1]` (QuantLib's
    /// `GaussChebyshevIntegration`).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn chebyshev(n: Size) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussJacobiPolynomial::chebyshev())
    }

    /// Gauss-Chebyshev integration (second kind) over `[-1, 1]` (QuantLib's
    /// `GaussChebyshev2ndIntegration`).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn chebyshev2nd(n: Size) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussJacobiPolynomial::chebyshev2nd())
    }

    /// Gauss-Jacobi integration over `[-1, 1]` (QuantLib's
    /// `GaussJacobiIntegration`).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero or the exponents are out of domain
    /// (see [`GaussJacobiPolynomial::new`]).
    pub fn jacobi(n: Size, alpha: Real, beta: Real) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussJacobiPolynomial::new(alpha, beta)?)
    }

    /// Gauss-Gegenbauer integration over `[-1, 1]` (QuantLib's
    /// `GaussGegenbauerIntegration`).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero or `lambda <= -1/2`.
    pub fn gegenbauer(n: Size, lambda: Real) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussJacobiPolynomial::gegenbauer(lambda)?)
    }

    /// Generalized Gauss-Laguerre integration over `[0, inf)` (QuantLib's
    /// `GaussLaguerreIntegration`; pass `s = 0` for the plain rule).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero or `s <= -1`.
    pub fn laguerre(n: Size, s: Real) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussLaguerrePolynomial::new(s)?)
    }

    /// Generalized Gauss-Hermite integration over the real line (QuantLib's
    /// `GaussHermiteIntegration`; pass `mu = 0` for the plain rule).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero or `mu <= -1/2`.
    pub fn hermite(n: Size, mu: Real) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussHermitePolynomial::new(mu)?)
    }

    /// Gauss-Hyperbolic integration over the real line (QuantLib's
    /// `GaussHyperbolicIntegration`).
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn hyperbolic(n: Size) -> crate::errors::QlResult<Self> {
        Self::new(n, &GaussHyperbolicPolynomial)
    }
}

/// [`Integrator`] adapter over a `[-1, 1]` Gaussian quadrature rule, mapping
/// `[a, b]` onto the rule's support affinely.
///
/// Port of QuantLib's `detail::GaussianQuadratureIntegrator` and its
/// `GaussLegendreIntegrator`, `GaussChebyshevIntegrator` and
/// `GaussChebyshev2ndIntegrator` instantiations.
#[derive(Clone, Debug)]
pub struct GaussianQuadratureIntegrator {
    integration: GaussianQuadrature,
}

impl GaussianQuadratureIntegrator {
    /// An integrator over the `n`-point Gauss-Legendre rule.
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn legendre(n: Size) -> crate::errors::QlResult<Self> {
        Ok(GaussianQuadratureIntegrator {
            integration: GaussianQuadrature::legendre(n)?,
        })
    }

    /// An integrator over the `n`-point Gauss-Chebyshev (first kind) rule.
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn chebyshev(n: Size) -> crate::errors::QlResult<Self> {
        Ok(GaussianQuadratureIntegrator {
            integration: GaussianQuadrature::chebyshev(n)?,
        })
    }

    /// An integrator over the `n`-point Gauss-Chebyshev (second kind) rule.
    ///
    /// # Errors
    ///
    /// Returns an error if `n` is zero.
    pub fn chebyshev2nd(n: Size) -> crate::errors::QlResult<Self> {
        Ok(GaussianQuadratureIntegrator {
            integration: GaussianQuadrature::chebyshev2nd(n)?,
        })
    }

    /// The underlying quadrature rule (QuantLib's `getIntegration`).
    pub fn integration(&self) -> &GaussianQuadrature {
        &self.integration
    }
}

impl Integrator for GaussianQuadratureIntegrator {
    fn integrate_impl<F>(&self, f: &mut F, a: Real, b: Real) -> crate::errors::QlResult<Real>
    where
        F: FnMut(Real) -> Real,
    {
        let c1 = 0.5 * (b - a);
        let c2 = 0.5 * (a + b);
        Ok(c1 * self.integration.integrate(|x| f(c1 * x + c2)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::integrals::gaussianorthogonalpolynomial::GaussianOrthogonalPolynomial;

    // Monic Legendre recurrence (w(x) = 1 on [-1, 1]): the driver must
    // reproduce the textbook 3-point Gauss-Legendre rule from it.
    struct MonicLegendre;

    impl GaussianOrthogonalPolynomial for MonicLegendre {
        fn mu_0(&self) -> Real {
            2.0
        }
        fn alpha(&self, _i: Size) -> Real {
            0.0
        }
        fn beta(&self, i: Size) -> Real {
            let k = i as Real;
            k * k / (4.0 * k * k - 1.0)
        }
        fn w(&self, _x: Real) -> Real {
            1.0
        }
    }

    #[test]
    fn reproduces_three_point_gauss_legendre_rule() {
        let quad = GaussianQuadrature::new(3, &MonicLegendre).unwrap();
        let node = (0.6 as Real).sqrt();
        let expected_x = [node, 0.0, -node];
        let expected_w = [5.0 / 9.0, 8.0 / 9.0, 5.0 / 9.0];
        for i in 0..3 {
            assert!(
                (quad.abscissas()[i] - expected_x[i]).abs() < 1e-14,
                "node {i}: got {}, expected {}",
                quad.abscissas()[i],
                expected_x[i]
            );
            assert!(
                (quad.weights()[i] - expected_w[i]).abs() < 1e-14,
                "weight {i}: got {}, expected {}",
                quad.weights()[i],
                expected_w[i]
            );
        }
    }

    #[test]
    fn integrates_polynomials_up_to_degree_2n_minus_1_exactly() {
        let quad = GaussianQuadrature::new(3, &MonicLegendre).unwrap();
        assert_eq!(quad.order(), 3);
        assert!((quad.integrate(|_| 1.0) - 2.0).abs() < 1e-14);
        assert!((quad.integrate(|x| x * x) - 2.0 / 3.0).abs() < 1e-14);
        assert!((quad.integrate(|x| x.powi(5))).abs() < 1e-14);
    }

    #[test]
    fn rejects_zero_points() {
        assert!(GaussianQuadrature::new(0, &MonicLegendre).is_err());
    }

    fn check_single<F: Fn(Real) -> Real>(
        quad: &GaussianQuadrature,
        tag: &str,
        f: F,
        expected: Real,
    ) {
        let calculated = quad.integrate(f);
        assert!(
            (calculated - expected).abs() <= 1.0e-4,
            "integrating {tag}: calculated {calculated}, expected {expected}"
        );
    }

    // Faithful port of testJacobi from test-suite/gaussianquadratures.cpp:
    // each rule integrates 1, x, x^2, sin, cos and the standard normal
    // density over [-1, 1] to within 1e-4.
    fn check_jacobi_family(quad: &GaussianQuadrature) {
        use crate::math::distributions::normal::{
            CumulativeNormalDistribution, NormalDistribution,
        };

        check_single(quad, "f(x) = 1", |_| 1.0, 2.0);
        check_single(quad, "f(x) = x", |x| x, 0.0);
        check_single(quad, "f(x) = x^2", |x| x * x, 2.0 / 3.0);
        check_single(quad, "f(x) = sin(x)", |x| x.sin(), 0.0);
        check_single(
            quad,
            "f(x) = cos(x)",
            |x| x.cos(),
            (1.0 as Real).sin() - (-1.0 as Real).sin(),
        );
        let cnd = CumulativeNormalDistribution::standard();
        check_single(
            quad,
            "f(x) = Gaussian(x)",
            |x| NormalDistribution::standard().value(x),
            cnd.value(1.0) - cnd.value(-1.0),
        );
    }

    #[test]
    fn jacobi_family_oracle() {
        check_jacobi_family(&GaussianQuadrature::legendre(16).unwrap());
        check_jacobi_family(&GaussianQuadrature::chebyshev(130).unwrap());
        check_jacobi_family(&GaussianQuadrature::chebyshev2nd(130).unwrap());
        check_jacobi_family(&GaussianQuadrature::gegenbauer(50, 0.55).unwrap());
    }

    // Faithful port of testLaguerre from test-suite/gaussianquadratures.cpp.
    #[test]
    fn laguerre_oracle() {
        use crate::math::distributions::normal::NormalDistribution;

        for quad in [
            GaussianQuadrature::laguerre(16, 0.0).unwrap(),
            GaussianQuadrature::laguerre(150, 0.01).unwrap(),
        ] {
            check_single(&quad, "f(x) = exp(-x)", |x| (-x).exp(), 1.0);
            check_single(&quad, "f(x) = x*exp(-x)", |x| x * (-x).exp(), 1.0);
            check_single(
                &quad,
                "f(x) = Gaussian(x)",
                |x| NormalDistribution::standard().value(x),
                0.5,
            );
        }

        check_single(
            &GaussianQuadrature::laguerre(16, 1.0).unwrap(),
            "f(x) = x*exp(-x)",
            |x| x * (-x).exp(),
            1.0,
        );
        check_single(
            &GaussianQuadrature::laguerre(32, 0.9).unwrap(),
            "f(x) = x*exp(-x)",
            |x| x * (-x).exp(),
            1.0,
        );
    }

    // Faithful port of testHermite from test-suite/gaussianquadratures.cpp.
    #[test]
    fn hermite_oracle() {
        use crate::math::distributions::normal::NormalDistribution;

        check_single(
            &GaussianQuadrature::hermite(16, 0.0).unwrap(),
            "f(x) = Gaussian(x)",
            |x| NormalDistribution::standard().value(x),
            1.0,
        );
        check_single(
            &GaussianQuadrature::hermite(16, 0.5).unwrap(),
            "f(x) = x*Gaussian(x)",
            |x| x * NormalDistribution::standard().value(x),
            0.0,
        );
        check_single(
            &GaussianQuadrature::hermite(64, 0.9).unwrap(),
            "f(x) = x*x*Gaussian(x)",
            |x| x * x * NormalDistribution::standard().value(x),
            1.0,
        );
    }

    // Faithful port of testHyperbolic from test-suite/gaussianquadratures.cpp.
    #[test]
    fn hyperbolic_oracle() {
        let quad = GaussianQuadrature::hyperbolic(16).unwrap();
        check_single(
            &quad,
            "f(x) = 1/cosh(x)",
            |x| 1.0 / x.cosh(),
            std::f64::consts::PI,
        );
        check_single(&quad, "f(x) = x/cosh(x)", |x| x / x.cosh(), 0.0);
    }

    // Faithful ports of testGaussLegendreIntegrator, testGaussChebyshev
    // Integrator and testGaussChebyshev2ndIntegrator from
    // test-suite/integrals.cpp (Abcd case omitted, not yet ported); the
    // shared tolerance there is 1e-6.
    const INTEGRATOR_TOL: Real = 1.0e-6;

    fn check_integrator_single<F: FnMut(Real) -> Real>(
        integrator: &GaussianQuadratureIntegrator,
        tag: &str,
        f: F,
        a: Real,
        b: Real,
        expected: Real,
    ) {
        let calculated = integrator.integrate(f, a, b).unwrap();
        assert!(
            (calculated - expected).abs() <= INTEGRATOR_TOL,
            "integrating {tag}: calculated {calculated}, expected {expected}"
        );
    }

    fn check_integrator_several(integrator: &GaussianQuadratureIntegrator) {
        use crate::math::distributions::normal::NormalDistribution;
        use std::f64::consts::PI;

        check_integrator_single(integrator, "f(x) = 0", |_| 0.0, 0.0, 1.0, 0.0);
        check_integrator_single(integrator, "f(x) = 1", |_| 1.0, 0.0, 1.0, 1.0);
        check_integrator_single(integrator, "f(x) = x", |x| x, 0.0, 1.0, 0.5);
        check_integrator_single(integrator, "f(x) = x^2", |x| x * x, 0.0, 1.0, 1.0 / 3.0);
        check_integrator_single(integrator, "f(x) = sin(x)", |x| x.sin(), 0.0, PI, 2.0);
        check_integrator_single(integrator, "f(x) = cos(x)", |x| x.cos(), 0.0, PI, 0.0);
        check_integrator_single(
            integrator,
            "f(x) = Gaussian(x)",
            |x| NormalDistribution::standard().value(x),
            -10.0,
            10.0,
            1.0,
        );
    }

    fn check_degenerated_domain(integrator: &GaussianQuadratureIntegrator) {
        check_integrator_single(
            integrator,
            "f(x) = 0 over [1, 1 + macheps]",
            |_| 0.0,
            1.0,
            1.0 + Real::EPSILON,
            0.0,
        );
    }

    #[test]
    fn gauss_legendre_integrator_oracle() {
        let integrator = GaussianQuadratureIntegrator::legendre(64).unwrap();
        check_integrator_several(&integrator);
        check_degenerated_domain(&integrator);
    }

    #[test]
    fn gauss_chebyshev_integrator_oracle() {
        use crate::math::distributions::normal::NormalDistribution;

        let integrator = GaussianQuadratureIntegrator::chebyshev(64).unwrap();
        check_integrator_single(
            &integrator,
            "f(x) = Gaussian(x)",
            |x| NormalDistribution::standard().value(x),
            -10.0,
            10.0,
            1.0,
        );
        check_degenerated_domain(&integrator);
    }

    #[test]
    fn gauss_chebyshev2nd_integrator_oracle() {
        use crate::math::distributions::normal::NormalDistribution;

        let integrator = GaussianQuadratureIntegrator::chebyshev2nd(64).unwrap();
        check_integrator_single(
            &integrator,
            "f(x) = Gaussian(x)",
            |x| NormalDistribution::standard().value(x),
            -10.0,
            10.0,
            1.0,
        );
        check_degenerated_domain(&integrator);
    }
}
