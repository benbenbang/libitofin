//! Shared fixtures for the decomposition tests: the `setup()` matrices and
//! `norm` helpers from `test-suite/matrices.cpp`.

use crate::math::matrix::Matrix;
use crate::types::Real;

pub(crate) fn matrices_m1() -> Matrix {
    Matrix::from([[1.0, 0.9, 0.7], [0.9, 1.0, 0.4], [0.7, 0.4, 1.0]])
}

pub(crate) fn matrices_m2() -> Matrix {
    Matrix::from([[1.0, 0.9, 0.7], [0.9, 1.0, 0.3], [0.7, 0.3, 1.0]])
}

pub(crate) fn matrices_m3() -> Matrix {
    Matrix::from([
        [1.0, 2.0, 3.0, 4.0],
        [2.0, 0.0, 2.0, 1.0],
        [0.0, 1.0, 0.0, 0.0],
    ])
}

pub(crate) fn matrices_m4() -> Matrix {
    Matrix::from([
        [1.0, 2.0, 400.0],
        [2.0, 0.0, 1.0],
        [30.0, 2.0, 0.0],
        [2.0, 0.0, 1.05],
    ])
}

pub(crate) fn identity(n: usize) -> Matrix {
    let mut m = Matrix::with_size(n, n);
    for i in 0..n {
        m[(i, i)] = 1.0;
    }
    m
}

/// The Frobenius norm of `m`, matching `norm(const Matrix&)` in the C++ suite.
pub(crate) fn norm_matrix(m: &Matrix) -> Real {
    let mut sum = 0.0;
    for i in 0..m.rows() {
        for j in 0..m.columns() {
            sum += m[(i, j)] * m[(i, j)];
        }
    }
    sum.sqrt()
}
