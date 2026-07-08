//! Cost function for optimization problems.
//!
//! Port of `ql/math/optimization/costfunction.hpp`. The C++ abstract class
//! becomes a trait: `values` is the one required method, while the scalar
//! cost (root mean square of `values`), the central finite-difference
//! gradient and Jacobian, and the combined evaluations are provided defaults
//! that implementations can override with analytic versions.

use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::types::Real;

/// A vector-valued cost function to minimize.
pub trait CostFunction {
    /// Computes the cost function values at `x`.
    fn values(&self, x: &Array) -> Array;

    /// Computes the scalar cost at `x`: the root mean square of [`Self::values`].
    ///
    /// # Panics
    ///
    /// Panics if [`Self::values`] returns an empty array: the mean of zero
    /// residuals would silently evaluate to NaN.
    fn value(&self, x: &Array) -> Real {
        let v = self.values(x);
        assert!(!v.is_empty(), "no residuals returned by the cost function");
        (v.iter().map(|e| e * e).sum::<Real>() / v.size() as Real).sqrt()
    }

    /// Computes `grad`, the first derivative of the scalar cost at `x`.
    fn gradient(&self, grad: &mut Array, x: &Array) {
        let eps = self.finite_difference_epsilon();
        let mut xx = x.clone();
        for i in 0..x.size() {
            xx[i] += eps;
            let fp = self.value(&xx);
            xx[i] -= 2.0 * eps;
            let fm = self.value(&xx);
            grad[i] = 0.5 * (fp - fm) / eps;
            xx[i] = x[i];
        }
    }

    /// Computes both the gradient and the scalar cost at `x`.
    fn value_and_gradient(&self, grad: &mut Array, x: &Array) -> Real {
        self.gradient(grad, x);
        self.value(x)
    }

    /// Computes `jac`, the Jacobian of the cost function at `x`.
    fn jacobian(&self, jac: &mut Matrix, x: &Array) {
        let eps = self.finite_difference_epsilon();
        let mut xx = x.clone();
        for i in 0..x.size() {
            xx[i] += eps;
            let fp = self.values(&xx);
            xx[i] -= 2.0 * eps;
            let fm = self.values(&xx);
            for j in 0..fp.size() {
                jac[(j, i)] = 0.5 * (fp[j] - fm[j]) / eps;
            }
            xx[i] = x[i];
        }
    }

    /// Computes both the Jacobian and the cost function values at `x`.
    fn values_and_jacobian(&self, jac: &mut Matrix, x: &Array) -> Array {
        self.jacobian(jac, x);
        self.values(x)
    }

    /// The step used by the default finite-difference derivatives.
    fn finite_difference_epsilon(&self) -> Real {
        1e-8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Quadratic;

    impl CostFunction for Quadratic {
        fn values(&self, x: &Array) -> Array {
            Array::from([x[0] * x[0] + x[1], 2.0 * x[1]])
        }
    }

    #[test]
    fn value_is_root_mean_square_of_values() {
        let x = Array::from([2.0, 3.0]);
        let expected = ((7.0 * 7.0 + 6.0 * 6.0) / 2.0_f64).sqrt();
        assert!((Quadratic.value(&x) - expected).abs() < 1e-15);
    }

    #[test]
    fn default_gradient_matches_analytic_derivative() {
        let x = Array::from([2.0, 3.0]);
        let mut grad = Array::with_size(2);
        Quadratic.gradient(&mut grad, &x);
        // d/dx of sqrt(((x^2+y)^2 + (2y)^2)/2) at (2, 3)
        let f = Quadratic.value(&x);
        let analytic_dx = 7.0 * 2.0 * x[0] / (2.0 * f);
        let analytic_dy = (7.0 + 4.0 * x[1]) / (2.0 * f);
        assert!((grad[0] - analytic_dx).abs() < 1e-6);
        assert!((grad[1] - analytic_dy).abs() < 1e-6);
    }

    #[test]
    fn default_jacobian_matches_analytic_derivatives() {
        let x = Array::from([2.0, 3.0]);
        let mut jac = Matrix::with_size(2, 2);
        Quadratic.jacobian(&mut jac, &x);
        assert!((jac[(0, 0)] - 4.0).abs() < 1e-6);
        assert!((jac[(0, 1)] - 1.0).abs() < 1e-6);
        assert!((jac[(1, 0)] - 0.0).abs() < 1e-6);
        assert!((jac[(1, 1)] - 2.0).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "no residuals returned by the cost function")]
    fn default_value_rejects_an_empty_residual_vector() {
        struct EmptyCost;
        impl CostFunction for EmptyCost {
            fn values(&self, _x: &Array) -> Array {
                Array::new()
            }
        }
        let _ = EmptyCost.value(&Array::from([1.0]));
    }

    #[test]
    fn combined_evaluations_agree_with_separate_calls() {
        let x = Array::from([2.0, 3.0]);
        let mut grad = Array::with_size(2);
        let v = Quadratic.value_and_gradient(&mut grad, &x);
        assert_eq!(v, Quadratic.value(&x));
        let mut jac = Matrix::with_size(2, 2);
        let vals = Quadratic.values_and_jacobian(&mut jac, &x);
        assert_eq!(vals, Quadratic.values(&x));
    }
}
