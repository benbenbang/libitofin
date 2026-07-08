//! Pseudo square root of a real symmetric matrix.
//!
//! Port of `ql/math/matrixutilities/pseudosqrt.{hpp,cpp}`: returns a matrix m
//! with m * m^T equal to the input, salvaging matrices that are not positive
//! semi-definite. The `Hypersphere` and `LowerDiagonal` salvaging variants
//! (which wrap the conjugate-gradient optimizer) are not ported yet; the
//! `matrices.cpp` oracle exercises `None`, `Higham` and `Principal`.

use crate::math::matrix::Matrix;
use crate::math::matrixutilities::choleskydecomposition::cholesky_decomposition;
use crate::math::matrixutilities::symmetricschurdecomposition::SymmetricSchurDecomposition;
use crate::types::{Real, Size};

/// Algorithm used to salvage a matrix that is not positive semi-definite.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SalvagingAlgorithm {
    /// No salvaging: the input must be positive semi-definite.
    None,
    /// Negative eigenvalues are set to zero (principal component analysis).
    Spectral,
    /// Higham's projection onto the nearest correlation matrix.
    Higham,
    /// The symmetric principal square root `V sqrt(D) V^T`.
    Principal,
}

/// Rescales the rows of `pseudo` so that `pseudo * pseudo^T` reproduces the
/// diagonal of `matrix`.
fn normalize_pseudo_root(matrix: &Matrix, pseudo: &mut Matrix) {
    let size = matrix.rows();
    assert!(
        size == pseudo.rows(),
        "matrix/pseudo mismatch: matrix rows are {size} while pseudo rows are {}",
        pseudo.rows()
    );
    let pseudo_cols = pseudo.columns();

    for i in 0..size {
        let mut norm = 0.0;
        for j in 0..pseudo_cols {
            norm += pseudo[(i, j)] * pseudo[(i, j)];
        }
        if norm > 0.0 {
            let norm_adj = (matrix[(i, i)] / norm).sqrt();
            for j in 0..pseudo_cols {
                pseudo[(i, j)] *= norm_adj;
            }
        }
    }
}

/// The matrix infinity norm; see Golub and Van Loan (2.3.10).
fn norm_inf(m: &Matrix) -> Real {
    let mut norm: Real = 0.0;
    for i in 0..m.rows() {
        let mut row_sum = 0.0;
        for j in 0..m.columns() {
            row_sum += m[(i, j)].abs();
        }
        norm = norm.max(row_sum);
    }
    norm
}

/// A copy of `m` with all diagonal entries set to 1.
fn project_to_unit_diagonal_matrix(m: &Matrix) -> Matrix {
    let size = m.rows();
    assert!(size == m.columns(), "matrix not square");

    let mut result = m.clone();
    for i in 0..size {
        result[(i, i)] = 1.0;
    }
    result
}

/// A copy of `m` with all negative eigenvalues clipped to zero.
fn project_to_positive_semidefinite_matrix(m: &Matrix) -> Matrix {
    let size = m.rows();
    assert!(size == m.columns(), "matrix not square");

    let jd = SymmetricSchurDecomposition::new(m);
    let mut diagonal = Matrix::with_size(size, size);
    for i in 0..size {
        diagonal[(i, i)] = jd.eigenvalues()[i].max(0.0);
    }

    &(jd.eigenvectors() * &diagonal) * &jd.eigenvectors().transpose()
}

/// Higham's alternating-projections algorithm for the nearest correlation
/// matrix.
fn higham_implementation(a: &Matrix, max_iterations: Size, tolerance: Real) -> Matrix {
    let size = a.rows();
    let mut y = a.clone();
    let mut x = a.clone();
    let mut delta_s = Matrix::with_size(size, size);

    let mut last_x = x.clone();
    let mut last_y = y.clone();

    for _ in 0..max_iterations {
        let r = &y - &delta_s;
        x = project_to_positive_semidefinite_matrix(&r);
        delta_s = &x - &r;
        y = project_to_unit_diagonal_matrix(&x);

        let error = (norm_inf(&(&x - &last_x)) / norm_inf(&x))
            .max(norm_inf(&(&y - &last_y)) / norm_inf(&y))
            .max(norm_inf(&(&y - &x)) / norm_inf(&y));
        if error <= tolerance {
            break;
        }
        last_x = x.clone();
        last_y = y.clone();
    }

    // ensure we return a symmetric matrix
    for i in 0..size {
        for j in 0..i {
            y[(i, j)] = y[(j, i)];
        }
    }

    y
}

/// A pseudo square root m of `matrix` (m * m^T reproduces it), salvaging
/// non-positive-semi-definite inputs with the given algorithm.
///
/// # Panics
///
/// Panics if `matrix` is not square, or if it has negative eigenvalues beyond
/// what the chosen salvaging algorithm accepts.
pub fn pseudo_sqrt(matrix: &Matrix, sa: SalvagingAlgorithm) -> Matrix {
    let size = matrix.rows();
    assert!(
        size == matrix.columns(),
        "non square matrix: {size} rows, {} columns",
        matrix.columns()
    );

    // spectral (a.k.a. principal component) analysis
    let jd = SymmetricSchurDecomposition::new(matrix);
    let mut diagonal = Matrix::with_size(size, size);

    match sa {
        SalvagingAlgorithm::None => {
            // eigenvalues are sorted in decreasing order
            assert!(
                jd.eigenvalues()[size - 1] >= -1e-16,
                "negative eigenvalue(s) ({:e})",
                jd.eigenvalues()[size - 1]
            );
            cholesky_decomposition(matrix, true)
        }
        SalvagingAlgorithm::Spectral => {
            // negative eigenvalues set to zero
            for i in 0..size {
                diagonal[(i, i)] = jd.eigenvalues()[i].max(0.0).sqrt();
            }

            let mut result = jd.eigenvectors() * &diagonal;
            normalize_pseudo_root(matrix, &mut result);
            result
        }
        SalvagingAlgorithm::Higham => {
            let max_iterations = 40;
            let tol = 1e-6;
            let result = higham_implementation(matrix, max_iterations, tol);
            cholesky_decomposition(&result, true)
        }
        SalvagingAlgorithm::Principal => {
            assert!(
                jd.eigenvalues()[size - 1] >= -10.0 * Real::EPSILON,
                "negative eigenvalue(s) ({:e})",
                jd.eigenvalues()[size - 1]
            );

            let sqrt_eigenvalues: Vec<Real> = jd
                .eigenvalues()
                .iter()
                .map(|&lambda| lambda.max(0.0).sqrt())
                .collect();

            for i in 0..size {
                for j in 0..size {
                    diagonal[(j, i)] = sqrt_eigenvalues[j] * jd.eigenvectors()[(i, j)];
                }
            }

            let result = jd.eigenvectors() * &diagonal;
            &(&result + &result.transpose()) * 0.5
        }
    }
}

/// A rank-reduced pseudo square root of `matrix`: the number of columns is
/// the number of retained eigenvalue components, capped at `max_rank`.
///
/// # Panics
///
/// Panics if `matrix` is not square, if `component_retained_percentage` is
/// outside (0, 1], if `max_rank` is zero, or on negative eigenvalues when
/// `sa` is [`SalvagingAlgorithm::None`]. [`SalvagingAlgorithm::Principal`] is
/// not a valid salvaging choice here.
pub fn rank_reduced_sqrt(
    matrix: &Matrix,
    max_rank: Size,
    component_retained_percentage: Real,
    sa: SalvagingAlgorithm,
) -> Matrix {
    let size = matrix.rows();
    assert!(
        size == matrix.columns(),
        "non square matrix: {size} rows, {} columns",
        matrix.columns()
    );
    assert!(
        component_retained_percentage > 0.0,
        "no eigenvalues retained"
    );
    assert!(
        component_retained_percentage <= 1.0,
        "percentage to be retained > 100%"
    );
    assert!(max_rank >= 1, "max rank required < 1");

    // spectral (a.k.a. principal component) analysis
    let mut jd = SymmetricSchurDecomposition::new(matrix);
    let mut eigen_values = jd.eigenvalues().clone();

    // salvaging algorithm
    match sa {
        SalvagingAlgorithm::None => {
            // eigenvalues are sorted in decreasing order
            assert!(
                eigen_values[size - 1] >= -1e-16,
                "negative eigenvalue(s) ({:e})",
                eigen_values[size - 1]
            );
        }
        SalvagingAlgorithm::Spectral => {
            // negative eigenvalues set to zero
            for i in 0..size {
                eigen_values[i] = eigen_values[i].max(0.0);
            }
        }
        SalvagingAlgorithm::Higham => {
            let max_iterations = 40;
            let tolerance = 1e-6;
            let adjusted_matrix = higham_implementation(matrix, max_iterations, tolerance);
            jd = SymmetricSchurDecomposition::new(&adjusted_matrix);
            eigen_values = jd.eigenvalues().clone();
        }
        SalvagingAlgorithm::Principal => {
            panic!("unknown or invalid salvaging algorithm");
        }
    }

    // factor reduction
    let mut enough = component_retained_percentage * eigen_values.iter().sum::<Real>();
    if component_retained_percentage == 1.0 {
        // numerical glitches might cause some factors to be discarded
        enough *= 1.1;
    }
    // retain at least one factor
    let mut components = eigen_values[0];
    let mut retained_factors: Size = 1;
    let mut i = 1;
    while components < enough && i < size {
        components += eigen_values[i];
        retained_factors += 1;
        i += 1;
    }
    // output is granted to have a rank <= max_rank
    retained_factors = retained_factors.min(max_rank);

    let mut diagonal = Matrix::with_size(size, retained_factors);
    for i in 0..retained_factors {
        diagonal[(i, i)] = eigen_values[i].sqrt();
    }
    let mut result = jd.eigenvectors() * &diagonal;

    normalize_pseudo_root(matrix, &mut result);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::matrixutilities::testsupport::{
        assert_close_matrix, create_test_correlation_matrix, matrices_m1, matrices_m5, matrices_m6,
        norm_matrix,
    };

    #[test]
    fn matricial_square_root() {
        let m1 = matrices_m1();
        let m = pseudo_sqrt(&m1, SalvagingAlgorithm::None);
        let temp = &m * &m.transpose();
        let error = norm_matrix(&(&temp - &m1));
        let tolerance = 1.0e-12;
        assert!(
            error <= tolerance,
            "matrix square root calculation failed (error {error})"
        );
    }

    #[test]
    fn higham_matricial_square_root() {
        let temp_sqrt = pseudo_sqrt(&matrices_m5(), SalvagingAlgorithm::Higham);
        let ans_sqrt = pseudo_sqrt(&matrices_m6(), SalvagingAlgorithm::None);
        let error = norm_matrix(&(&ans_sqrt - &temp_sqrt));
        let tolerance = 1.0e-4;
        assert!(
            error <= tolerance,
            "Higham matrix correction failed (error {error})"
        );
    }

    #[test]
    fn principal_matricial_square_root() {
        for n in [1, 4, 10, 40] {
            let rho = create_test_correlation_matrix(n);
            let sqrt_rho = pseudo_sqrt(&rho, SalvagingAlgorithm::Principal);

            assert_close_matrix(&sqrt_rho, &sqrt_rho.transpose(), 1e3 * Real::EPSILON);
            assert_close_matrix(&(&sqrt_rho * &sqrt_rho), &rho, 1e5 * Real::EPSILON);
        }
    }

    #[test]
    fn rank_reduced_sqrt_reconstructs_full_rank_input() {
        let m1 = matrices_m1();
        let m = rank_reduced_sqrt(&m1, 3, 1.0, SalvagingAlgorithm::Spectral);
        let error = norm_matrix(&(&(&m * &m.transpose()) - &m1));
        assert!(
            error <= 1.0e-12,
            "full-rank reconstruction failed ({error})"
        );

        let reduced = rank_reduced_sqrt(&m1, 1, 1.0, SalvagingAlgorithm::Spectral);
        assert_eq!((reduced.rows(), reduced.columns()), (3, 1));
        for i in 0..3 {
            let diag: Real = (0..reduced.columns())
                .map(|j| reduced[(i, j)].powi(2))
                .sum();
            assert!((diag - m1[(i, i)]).abs() <= 1.0e-14);
        }
    }
}
