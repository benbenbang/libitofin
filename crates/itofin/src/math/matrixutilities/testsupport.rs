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

pub(crate) fn matrices_m5() -> Matrix {
    Matrix::from([
        [2.0, -1.0, 0.0, 0.0],
        [-1.0, 2.0, -1.0, 0.0],
        [0.0, -1.0, 2.0, -1.0],
        [0.0, 0.0, -1.0, 2.0],
    ])
}

pub(crate) fn matrices_m6() -> Matrix {
    Matrix::from([
        [1.0, -0.8084124981, 0.1915875019, 0.106775049],
        [-0.8084124981, 1.0, -0.6562326948, 0.1915875019],
        [0.1915875019, -0.6562326948, 1.0, -0.8084124981],
        [0.106775049, 0.1915875019, -0.8084124981, 1.0],
    ])
}

pub(crate) fn matrices_m7() -> Matrix {
    let mut m = matrices_m1();
    m[(0, 1)] = 0.3;
    m[(0, 2)] = 0.2;
    m[(2, 1)] = 1.2;
    m
}

pub(crate) fn identity(n: usize) -> Matrix {
    let mut m = Matrix::with_size(n, n);
    for i in 0..n {
        m[(i, i)] = 1.0;
    }
    m
}

/// A small deterministic PCG-style generator standing in for the QuantLib
/// MersenneTwisterUniformRng used by the C++ suite (ported separately as
/// QL-1.11); the decomposition tests only need reproducible values in [0, 1).
pub(crate) struct TestRng(u64);

impl TestRng {
    pub(crate) fn new(seed: u64) -> Self {
        TestRng(seed)
    }

    pub(crate) fn next_real(&mut self) -> Real {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 11) as Real / (1_u64 << 53) as Real
    }
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
