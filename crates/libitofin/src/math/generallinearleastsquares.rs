//! General linear least squares regression.
//!
//! Port of `ql/math/generallinearleastsquares.hpp:45-147`: fits the linear
//! combination of basis functions `v` that minimises the squared distance to
//! the samples `y`, solving the normal equations through the SVD of the design
//! matrix rather than forming `A^T A`.
//!
//! Divergence: `RegressionAbsoluteError` and the weighted-least-squares
//! overload are not ported, and neither are the `residuals()`, `error()` and
//! `standardErrors()` accessors (`generallinearleastsquares.hpp:57-62`); the
//! only consumer is the Longstaff-Schwartz backward regression, whose surfaced
//! uncertainty is the Monte Carlo error estimate. The size checks return
//! [`QlError`](crate::errors::QlError) instead of throwing (D4).

use crate::errors::QlResult;
use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::math::matrixutilities::svd::Svd;
use crate::require;
use crate::types::{Real, Size};

/// A general linear least squares fit of `y` against the basis functions `v`
/// evaluated at the regressor states `x`.
#[derive(Clone, Debug)]
pub struct GeneralLinearLeastSquares {
    a: Array,
}

impl GeneralLinearLeastSquares {
    /// Fits the samples eagerly, as the C++ constructor does
    /// (`generallinearleastsquares.hpp:78-87`).
    ///
    /// The basis functions are taken as a slice of any callable, so a
    /// heterogeneous system is passed as `&[Box<dyn Fn(X) -> Real>]`.
    ///
    /// # Errors
    ///
    /// Returns an error if `x` and `y` differ in length, if `v` is empty, or
    /// if there are fewer samples than basis functions.
    #[allow(clippy::needless_range_loop)]
    pub fn new<X, F>(x: &[X], y: &[Real], v: &[F]) -> QlResult<Self>
    where
        X: Copy,
        F: Fn(X) -> Real,
    {
        let n = y.len();
        let m = v.len();

        require!(
            x.len() == n,
            "sample set need to be of the same size, got {} regressors and {} values",
            x.len(),
            n
        );
        require!(!v.is_empty(), "no basis functions given");
        require!(n >= m, "sample set is too small");

        let mut design = Matrix::with_size(n, m);
        for j in 0..m {
            for i in 0..n {
                design[(i, j)] = v[j](x[i]);
            }
        }

        let svd = Svd::new(&design);
        let u = svd.u();
        let vt = svd.v();
        let w = svd.singular_values();
        let threshold = n as Real * Real::EPSILON * w[0];

        let mut a = Array::with_size(m);
        for i in 0..m {
            if w[i] > threshold {
                let mut projection = 0.0;
                for k in 0..n {
                    projection += u[(k, i)] * y[k];
                }
                projection /= w[i];

                for j in 0..m {
                    a[j] += projection * vt[(j, i)];
                }
            }
        }

        Ok(GeneralLinearLeastSquares { a })
    }

    /// The fitted coefficients, one per basis function.
    pub fn coefficients(&self) -> &Array {
        &self.a
    }

    /// The number of basis functions.
    pub fn dim(&self) -> Size {
        self.a.size()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type Basis = Vec<Box<dyn Fn(Real) -> Real>>;

    fn monomials(degree: Size) -> Basis {
        (0..=degree)
            .map(|k| Box::new(move |x: Real| x.powi(k as i32)) as Box<dyn Fn(Real) -> Real>)
            .collect()
    }

    fn fitted(x: Real, basis: &Basis, a: &Array) -> Real {
        (0..basis.len()).map(|j| a[j] * basis[j](x)).sum()
    }

    #[test]
    fn recovers_an_exactly_representable_quadratic() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let y: Vec<Real> = x.iter().map(|&x| 1.0 + 2.0 * x + 3.0 * x * x).collect();
        let basis = monomials(2);

        let fit = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap();
        let a = fit.coefficients();

        assert_eq!(fit.dim(), 3);
        assert!((a[0] - 1.0).abs() < 1e-10, "a0 = {}", a[0]);
        assert!((a[1] - 2.0).abs() < 1e-10, "a1 = {}", a[1]);
        assert!((a[2] - 3.0).abs() < 1e-10, "a2 = {}", a[2]);
    }

    #[test]
    fn projects_inconsistent_samples_onto_the_least_squares_line() {
        let x = [0.0, 1.0, 2.0, 3.0];
        let y = [1.0, 3.0, 2.0, 4.0];
        let basis = monomials(1);

        let fit = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap();
        let a = fit.coefficients();

        assert!((a[0] - 1.3).abs() < 1e-10, "intercept = {}", a[0]);
        assert!((a[1] - 0.8).abs() < 1e-10, "slope = {}", a[1]);
    }

    #[test]
    fn solves_a_rank_deficient_system() {
        let x = [1.0, 1.0, 2.0, 2.0, 3.0, 3.0];
        let y: Vec<Real> = x.iter().map(|&x| 1.0 + 2.0 * x + 3.0 * x * x).collect();
        let basis = monomials(3);

        let fit = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap();
        let a = fit.coefficients();

        assert_eq!(fit.dim(), 4);
        for j in 0..4 {
            assert!(a[j].is_finite(), "a{} = {}", j, a[j]);
        }
        for i in 0..x.len() {
            let f = fitted(x[i], &basis, a);
            assert!(
                (f - y[i]).abs() < 1e-10,
                "fit at {} = {}, want {}",
                x[i],
                f,
                y[i]
            );
        }
    }

    #[test]
    fn drops_the_singular_direction_of_a_zero_basis_function() {
        let x = [0.0, 1.0, 2.0, 3.0, 4.0, 5.0];
        let y: Vec<Real> = x.iter().map(|&x| 1.0 + 2.0 * x + 3.0 * x * x).collect();
        let mut basis = monomials(2);
        basis.push(Box::new(|_: Real| 0.0));

        let fit = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap();
        let a = fit.coefficients();

        assert!((a[0] - 1.0).abs() < 1e-10, "a0 = {}", a[0]);
        assert!((a[1] - 2.0).abs() < 1e-10, "a1 = {}", a[1]);
        assert!((a[2] - 3.0).abs() < 1e-10, "a2 = {}", a[2]);
        assert!((a[3]).abs() < 1e-10, "a3 = {}", a[3]);
    }

    #[test]
    fn rejects_mismatched_sample_sets() {
        let x = [0.0, 1.0, 2.0];
        let y = [1.0, 2.0];
        let basis = monomials(1);

        let err = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap_err();
        assert!(err.message().contains("same size"), "{}", err);
    }

    #[test]
    fn rejects_a_sample_set_smaller_than_the_basis() {
        let x = [0.0, 1.0];
        let y = [1.0, 2.0];
        let basis = monomials(2);

        let err = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap_err();
        assert!(err.message().contains("too small"), "{}", err);
    }

    #[test]
    fn rejects_an_empty_basis() {
        let x = [0.0, 1.0];
        let y = [1.0, 2.0];
        let basis: Basis = Vec::new();

        let err = GeneralLinearLeastSquares::new(&x, &y, &basis).unwrap_err();
        assert!(err.message().contains("basis"), "{}", err);
    }
}
