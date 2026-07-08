//! Cholesky decomposition.
//!
//! Port of `ql/math/matrixutilities/choleskydecomposition.{hpp,cpp}`: for a
//! symmetric positive definite matrix S computes the lower triangular L with
//! S = L * L^T. In flexible mode positive semi-definite matrices are handled
//! by zeroing the pivots whose partial sum is not positive.

use crate::math::array::Array;
use crate::math::comparison::close_enough;
use crate::math::matrix::Matrix;

/// The lower triangular Cholesky factor of `s`.
///
/// # Panics
///
/// Panics if `s` is not square, or if `flexible` is false and `s` is not
/// positive definite.
pub fn cholesky_decomposition(s: &Matrix, flexible: bool) -> Matrix {
    let size = s.rows();
    assert!(size == s.columns(), "input matrix is not a square matrix");

    let mut result = Matrix::with_size(size, size);
    for i in 0..size {
        for j in i..size {
            let mut sum = s[(i, j)];
            for k in 0..i {
                sum -= result[(i, k)] * result[(j, k)];
            }
            if i == j {
                assert!(
                    flexible || sum > 0.0,
                    "input matrix is not positive definite"
                );
                // To handle positive semi-definite matrices take the square
                // root of sum if positive, else zero.
                result[(i, i)] = sum.max(0.0).sqrt();
            } else {
                // With positive semi-definite matrices it is possible to have
                // result[i][i] == 0.0; in this case sum is zero as well.
                result[(j, i)] = if close_enough(result[(i, i)], 0.0) {
                    0.0
                } else {
                    sum / result[(i, i)]
                };
            }
        }
    }
    result
}

/// Solves `L * L^T * x = b` by forward then backward substitution, given the
/// lower triangular Cholesky factor `L`.
///
/// # Panics
///
/// Panics if the dimensions of `l` and `b` do not match.
pub fn cholesky_solve_for(l: &Matrix, b: &Array) -> Array {
    let n = b.len();
    assert!(
        l.columns() == n && l.rows() == n,
        "size of input matrix and vector does not match"
    );

    let mut x = Array::with_size(n);
    for i in 0..n {
        let mut sum = b[i];
        for k in 0..i {
            sum -= l[(i, k)] * x[k];
        }
        x[i] = sum / l[(i, i)];
    }

    for i in (0..n).rev() {
        let mut sum = x[i];
        for k in i + 1..n {
            sum -= l[(k, i)] * x[k];
        }
        x[i] = sum / l[(i, i)];
    }

    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::matrixutilities::testsupport::assert_close_matrix;
    use crate::types::{Real, Size};

    #[test]
    fn cholesky_decomposition_of_near_singular_matrix() {
        // The eigenvalues of this matrix range from ~4.4e-2 down to ~5.8e-19;
        // the flexible decomposition must reproduce it without NaNs.
        let tmp: [[f64; 11]; 11] = [
            [
                6.4e-05,
                5.28e-05,
                2.28e-05,
                0.00032,
                0.00036,
                6.4e-05,
                6.3968010664e-06,
                7.2e-05,
                7.19460269899e-06,
                1.2e-05,
                1.19970004999e-06,
            ],
            [
                5.28e-05,
                0.000121,
                1.045e-05,
                0.00044,
                0.000165,
                2.2e-05,
                2.19890036657e-06,
                1.65e-05,
                1.64876311852e-06,
                1.1e-05,
                1.09972504583e-06,
            ],
            [
                2.28e-05,
                1.045e-05,
                9.025e-05,
                0.0,
                0.0001425,
                9.5e-06,
                9.49525158294e-07,
                2.85e-05,
                2.84786356835e-06,
                4.75e-06,
                4.74881269789e-07,
            ],
            [
                0.00032,
                0.00044,
                0.0,
                0.04,
                0.009,
                0.0008,
                7.996001333e-05,
                0.0006,
                5.99550224916e-05,
                0.0001,
                9.99750041661e-06,
            ],
            [
                0.00036,
                0.000165,
                0.0001425,
                0.009,
                0.0225,
                0.0003,
                2.99850049987e-05,
                0.001125,
                0.000112415667172,
                0.000225,
                2.24943759374e-05,
            ],
            [
                6.4e-05,
                2.2e-05,
                9.5e-06,
                0.0008,
                0.0003,
                0.0001,
                9.99500166625e-06,
                7.5e-05,
                7.49437781145e-06,
                2e-05,
                1.99950008332e-06,
            ],
            [
                6.3968010664e-06,
                2.19890036657e-06,
                9.49525158294e-07,
                7.996001333e-05,
                2.99850049987e-05,
                9.99500166625e-06,
                9.99000583083e-07,
                7.49625124969e-06,
                7.49063187129e-07,
                1.99900033325e-06,
                1.99850066645e-07,
            ],
            [
                7.2e-05,
                1.65e-05,
                2.85e-05,
                0.0006,
                0.001125,
                7.5e-05,
                7.49625124969e-06,
                0.000225,
                2.24831334343e-05,
                1.5e-05,
                1.49962506249e-06,
            ],
            [
                7.19460269899e-06,
                1.64876311852e-06,
                2.84786356835e-06,
                5.99550224916e-05,
                0.000112415667172,
                7.49437781145e-06,
                7.49063187129e-07,
                2.24831334343e-05,
                2.24662795123e-06,
                1.49887556229e-06,
                1.49850090584e-07,
            ],
            [
                1.2e-05,
                1.1e-05,
                4.75e-06,
                0.0001,
                0.000225,
                2e-05,
                1.99900033325e-06,
                1.5e-05,
                1.49887556229e-06,
                2.5e-05,
                2.49937510415e-06,
            ],
            [
                1.19970004999e-06,
                1.09972504583e-06,
                4.74881269789e-07,
                9.99750041661e-06,
                2.24943759374e-05,
                1.99950008332e-06,
                1.99850066645e-07,
                1.49962506249e-06,
                1.49850090584e-07,
                2.49937510415e-06,
                2.49875036451e-07,
            ],
        ];

        let mut m = Matrix::with_size(11, 11);
        for i in 0..11 {
            for j in 0..11 {
                m[(i, j)] = tmp[i][j];
            }
        }

        let c = cholesky_decomposition(&m, true);
        let m2 = &c * &c.transpose();

        let tol = 1.0e-12;
        for i in 0..11 {
            for j in 0..11 {
                assert!(
                    !m2[(i, j)].is_nan(),
                    "failed to verify Cholesky decomposition at ({i}, {j}): replicated value is nan"
                );
                let error = (m[(i, j)] - m2[(i, j)]).abs();
                assert!(
                    error <= tol,
                    "failed to verify Cholesky decomposition at ({i}, {j}): original value is {}, replicated value is {}",
                    m[(i, j)],
                    m2[(i, j)]
                );
            }
        }
    }

    #[test]
    fn cholesky_solve_recovers_solution() {
        let s = Matrix::from([[4.0, 2.0, 0.8], [2.0, 5.0, 1.2], [0.8, 1.2, 3.0]]);
        let l = cholesky_decomposition(&s, false);
        let expected = Array::from([1.0, -2.0, 3.0]);
        let b = &s * &expected;
        let x = cholesky_solve_for(&l, &b);
        for i in 0..3 as Size {
            assert!((x[i] - expected[i]).abs() <= 1.0e-14);
        }
    }

    #[test]
    fn cholesky_solver_for_incomplete_matrix() {
        let n = 4;
        let mut rho = Matrix::with_size(n, n);
        rho[(0, 0)] = 1.0;
        rho[(1, 1)] = 1.0;
        rho[(0, 1)] = 0.9;
        rho[(1, 0)] = 0.9;

        let l = cholesky_decomposition(&rho, true);
        assert_close_matrix(&(&l * &l.transpose()), &rho, 100.0 * Real::EPSILON);
    }

    #[test]
    #[should_panic(expected = "not positive definite")]
    fn strict_mode_rejects_semi_definite_matrix() {
        let s = Matrix::from([[1.0, 1.0], [1.0, 1.0]]);
        let _ = cholesky_decomposition(&s, false);
    }
}
