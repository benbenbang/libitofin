//! Eigenvalues and eigenvectors of a real symmetric matrix.
//!
//! Port of `ql/math/matrixutilities/symmetricschurdecomposition.{hpp,cpp}`:
//! the symmetric threshold Jacobi algorithm (Golub and Van Loan, "Matrix
//! Computations", 2nd edition). Given a real symmetric matrix S it computes
//! S = U * D * U^T with D the diagonal matrix of eigenvalues (sorted in
//! decreasing order) and U the orthogonal matrix of eigenvectors.

use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::types::{Real, Size};

/// Schur decomposition of a real symmetric matrix via the threshold Jacobi
/// algorithm. Eigenvalues are sorted in decreasing order; each eigenvector
/// column is normalized and sign-fixed so its first component is positive.
#[derive(Clone, Debug)]
pub struct SymmetricSchurDecomposition {
    diagonal: Array,
    eigen_vectors: Matrix,
}

impl SymmetricSchurDecomposition {
    /// Decomposes `s`, which must be a non-empty square symmetric matrix.
    ///
    /// # Panics
    ///
    /// Panics if `s` is empty or not square, or if the Jacobi sweep fails to
    /// converge within 100 iterations.
    pub fn new(s: &Matrix) -> Self {
        assert!(s.rows() > 0 && s.columns() > 0, "null matrix given");
        assert!(s.rows() == s.columns(), "input matrix must be square");

        let size = s.rows();
        let mut diagonal = Array::with_size(size);
        let mut eigen_vectors = Matrix::with_size(size, size);
        for q in 0..size {
            diagonal[q] = s[(q, q)];
            eigen_vectors[(q, q)] = 1.0;
        }
        let mut ss = s.clone();

        let mut tmp_diag: Vec<Real> = diagonal.to_vec();
        let mut tmp_accumulate = vec![0.0; size];
        let eps_prec: Real = 1e-15;
        let mut keeplooping = true;
        let max_iterations: Size = 100;
        let mut ite: Size = 1;
        loop {
            let mut sum: Real = 0.0;
            for a in 0..size.saturating_sub(1) {
                for b in a + 1..size {
                    sum += ss[(a, b)].abs();
                }
            }

            if sum == 0.0 {
                keeplooping = false;
            } else {
                let threshold = if ite < 5 {
                    0.2 * sum / ((size * size) as Real)
                } else {
                    0.0
                };

                for j in 0..size - 1 {
                    for k in j + 1..size {
                        let smll = ss[(j, k)].abs();
                        if ite > 5
                            && smll < eps_prec * diagonal[j].abs()
                            && smll < eps_prec * diagonal[k].abs()
                        {
                            ss[(j, k)] = 0.0;
                        } else if ss[(j, k)].abs() > threshold {
                            let mut heig = diagonal[k] - diagonal[j];
                            let tang = if smll < eps_prec * heig.abs() {
                                ss[(j, k)] / heig
                            } else {
                                let beta = 0.5 * heig / ss[(j, k)];
                                let mut t = 1.0 / (beta.abs() + (1.0 + beta * beta).sqrt());
                                if beta < 0.0 {
                                    t = -t;
                                }
                                t
                            };
                            let cosin = 1.0 / (1.0 + tang * tang).sqrt();
                            let sine = tang * cosin;
                            let rho = sine / (1.0 + cosin);
                            heig = tang * ss[(j, k)];
                            tmp_accumulate[j] -= heig;
                            tmp_accumulate[k] += heig;
                            diagonal[j] -= heig;
                            diagonal[k] += heig;
                            ss[(j, k)] = 0.0;
                            for l in 0..j {
                                jacobi_rotate(&mut ss, rho, sine, l, j, l, k);
                            }
                            for l in j + 1..k {
                                jacobi_rotate(&mut ss, rho, sine, j, l, l, k);
                            }
                            for l in k + 1..size {
                                jacobi_rotate(&mut ss, rho, sine, j, l, k, l);
                            }
                            for l in 0..size {
                                jacobi_rotate(&mut eigen_vectors, rho, sine, l, j, l, k);
                            }
                        }
                    }
                }
                for k in 0..size {
                    tmp_diag[k] += tmp_accumulate[k];
                    diagonal[k] = tmp_diag[k];
                    tmp_accumulate[k] = 0.0;
                }
            }

            ite += 1;
            if ite > max_iterations || !keeplooping {
                break;
            }
        }
        assert!(
            ite <= max_iterations,
            "too many iterations ({max_iterations}) reached"
        );

        let mut temp: Vec<(Real, Vec<Real>)> = (0..size)
            .map(|col| {
                let eigen_vector: Vec<Real> =
                    (0..size).map(|row| eigen_vectors[(row, col)]).collect();
                (diagonal[col], eigen_vector)
            })
            .collect();
        temp.sort_by(|a, b| b.partial_cmp(a).expect("eigenvalues are not NaN"));
        let max_ev = temp[0].0;
        for (col, (value, vector)) in temp.iter().enumerate() {
            diagonal[col] = if (value / max_ev).abs() < 1e-16 {
                0.0
            } else {
                *value
            };
            let sign = if vector[0] < 0.0 { -1.0 } else { 1.0 };
            for (row, component) in vector.iter().enumerate() {
                eigen_vectors[(row, col)] = sign * component;
            }
        }

        SymmetricSchurDecomposition {
            diagonal,
            eigen_vectors,
        }
    }

    /// The eigenvalues, sorted in decreasing order.
    pub fn eigenvalues(&self) -> &Array {
        &self.diagonal
    }

    /// The orthogonal matrix whose columns are the eigenvectors, in the same
    /// order as [`eigenvalues`](Self::eigenvalues).
    pub fn eigenvectors(&self) -> &Matrix {
        &self.eigen_vectors
    }
}

/// The Jacobi (Givens) rotation applied to the `(j1, k1)` and `(j2, k2)`
/// entries of `m`.
fn jacobi_rotate(m: &mut Matrix, rot: Real, dil: Real, j1: Size, k1: Size, j2: Size, k2: Size) {
    let x1 = m[(j1, k1)];
    let x2 = m[(j2, k2)];
    m[(j1, k1)] = x1 - dil * (x2 + x1 * rot);
    m[(j2, k2)] = x2 + dil * (x1 - x2 * rot);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::matrixutilities::testsupport::{
        identity, matrices_m1, matrices_m2, norm_matrix,
    };

    #[test]
    fn eigenvalues_and_eigenvectors() {
        let n = 3;
        let identity = identity(n);

        for m in [matrices_m1(), matrices_m2()] {
            let dec = SymmetricSchurDecomposition::new(&m);
            let eigen_values = dec.eigenvalues();
            let eigen_vectors = dec.eigenvectors();
            let mut min_holder = Real::MAX;

            for i in 0..n {
                let v: Array = (0..n).map(|j| eigen_vectors[(j, i)]).collect();
                let a = &m * &v;
                let b = eigen_values[i] * &v;
                let error = (&a - &b).norm2();
                assert!(
                    error <= 1.0e-15,
                    "eigenvector definition not satisfied (error {error})"
                );
                assert!(
                    eigen_values[i] < min_holder,
                    "eigenvalues not ordered: {eigen_values:?}"
                );
                min_holder = eigen_values[i];
            }

            let normalization = eigen_vectors * &eigen_vectors.transpose();
            let error = norm_matrix(&(&normalization - &identity));
            assert!(error <= 1.0e-15, "eigenvector not normalized ({error})");
        }
    }
}
