//! Eigen decomposition of a symmetric tridiagonal matrix.
//!
//! Port of `ql/math/matrixutilities/tqreigendecomposition.{hpp,cpp}`: the
//! tridiagonal QR algorithm with implicit (Wilkinson) shift, after Wilkinson
//! and Reinsch ("Linear Algebra", Handbook for Automatic Computation vol. II)
//! and "Numerical Recipes in C", 2nd edition. Given the diagonal and
//! subdiagonal of a symmetric tridiagonal matrix it computes the eigenvalues
//! (sorted in decreasing order) and, on request, all eigenvectors or only
//! their first components - the latter being all that Gaussian quadratures
//! need for the weights.

#![allow(clippy::excessive_precision)]

use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::types::{Real, Size};

/// How much of the eigenvector matrix to compute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EigenVectorCalculation {
    /// The full eigenvector matrix.
    WithEigenVector,
    /// Eigenvalues only.
    WithoutEigenVector,
    /// Only the first row of the eigenvector matrix (the first component of
    /// each eigenvector), enough to derive Gaussian quadrature weights.
    OnlyFirstRowEigenVector,
}

/// Shift strategy for the QR iteration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShiftStrategy {
    /// Unshifted QR (slow convergence, kept for completeness).
    NoShift,
    /// Wilkinson shift scaled by 1.25 on the trailing eigenvalue.
    Overrelaxation,
    /// Plain Wilkinson shift: the eigenvalue of the trailing 2x2 block closest
    /// to the last diagonal entry.
    CloseEigenValue,
}

/// Tridiagonal QR eigen decomposition with explicit shift.
///
/// Eigenvalues are sorted in decreasing order; each computed eigenvector is
/// sign-fixed so its first component is positive, matching QuantLib.
#[derive(Clone, Debug)]
pub struct TqrEigenDecomposition {
    iterations: Size,
    d: Array,
    ev: Matrix,
}

impl TqrEigenDecomposition {
    /// Decomposes the symmetric tridiagonal matrix with diagonal `diag` and
    /// subdiagonal `sub`.
    ///
    /// # Panics
    ///
    /// Panics if `diag` is empty or `sub.size() != diag.size() - 1`.
    pub fn new(
        diag: &Array,
        sub: &Array,
        calc: EigenVectorCalculation,
        strategy: ShiftStrategy,
    ) -> Self {
        let n = diag.size();
        assert!(n > 0, "null diagonal given");
        assert!(n == sub.size() + 1, "wrong dimensions");

        let mut d = diag.clone();
        let rows = match calc {
            EigenVectorCalculation::WithEigenVector => n,
            EigenVectorCalculation::WithoutEigenVector => 0,
            EigenVectorCalculation::OnlyFirstRowEigenVector => 1,
        };
        let mut ev = Matrix::with_size(rows, n);
        for i in 0..rows {
            ev[(i, i)] = 1.0;
        }

        // e[i] couples d[i-1] and d[i]; e[0] stays unused, as in QuantLib.
        let mut e = Array::with_size(n);
        for i in 1..n {
            e[i] = sub[i - 1];
        }

        let mut iterations: Size = 0;
        for k in (1..n).rev() {
            while !off_diag_is_zero(k, &d, &e) {
                let mut l = k;
                loop {
                    l -= 1;
                    if l == 0 || off_diag_is_zero(l, &d, &e) {
                        break;
                    }
                }
                iterations += 1;

                let mut q = d[l];
                if strategy != ShiftStrategy::NoShift {
                    // Eigenvalue of the trailing 2x2 block
                    //   [ d[k-1] e[k] ]
                    //   [ e[k]   d[k] ]
                    // closer to d[k], used as the Wilkinson shift.
                    let t1 = (0.25 * (d[k] * d[k] + d[k - 1] * d[k - 1]) - 0.5 * d[k - 1] * d[k]
                        + e[k] * e[k])
                        .sqrt();
                    let t2 = 0.5 * (d[k] + d[k - 1]);

                    let lambda = if (t2 + t1 - d[k]).abs() < (t2 - t1 - d[k]).abs() {
                        t2 + t1
                    } else {
                        t2 - t1
                    };

                    if strategy == ShiftStrategy::CloseEigenValue {
                        q -= lambda;
                    } else {
                        q -= if k == n - 1 { 1.25 } else { 1.0 } * lambda;
                    }
                }

                // The QR transformation via Givens rotations.
                let mut sine = 1.0;
                let mut cosine = 1.0;
                let mut u = 0.0;

                let mut recover_underflow = false;
                let mut i = l + 1;
                while i <= k && !recover_underflow {
                    let h = cosine * e[i];
                    let p = sine * e[i];

                    e[i - 1] = (p * p + q * q).sqrt();
                    if e[i - 1] != 0.0 {
                        sine = p / e[i - 1];
                        cosine = q / e[i - 1];

                        let g = d[i - 1] - u;
                        let t = (d[i] - g) * sine + 2.0 * cosine * h;

                        u = sine * t;
                        d[i - 1] = g + u;
                        q = cosine * t - h;

                        for j in 0..rows {
                            let tmp = ev[(j, i - 1)];
                            ev[(j, i - 1)] = sine * ev[(j, i)] + cosine * tmp;
                            ev[(j, i)] = cosine * ev[(j, i)] - sine * tmp;
                        }
                    } else {
                        // Recover from underflow.
                        d[i - 1] -= u;
                        e[l] = 0.0;
                        recover_underflow = true;
                    }
                    i += 1;
                }

                if !recover_underflow {
                    d[k] -= u;
                    e[k] = q;
                    e[l] = 0.0;
                }
            }
        }

        // Sort (eigenvalue, eigenvector) pairs in decreasing order and fix
        // each eigenvector's sign so its first component is positive.
        let mut temp: Vec<(Real, Vec<Real>)> = (0..n)
            .map(|i| {
                let eigen_vector: Vec<Real> = (0..rows).map(|j| ev[(j, i)]).collect();
                (d[i], eigen_vector)
            })
            .collect();
        temp.sort_by(|a, b| b.partial_cmp(a).expect("eigenvalues are not NaN"));
        for (i, (value, vector)) in temp.iter().enumerate() {
            d[i] = *value;
            let sign = if rows > 0 && vector[0] < 0.0 {
                -1.0
            } else {
                1.0
            };
            for (j, component) in vector.iter().enumerate() {
                ev[(j, i)] = sign * component;
            }
        }

        TqrEigenDecomposition { iterations, d, ev }
    }

    /// The eigenvalues, sorted in decreasing order.
    pub fn eigenvalues(&self) -> &Array {
        &self.d
    }

    /// The computed rows of the eigenvector matrix: row `j` holds component
    /// `j` of every eigenvector, columns ordered as
    /// [`eigenvalues`](Self::eigenvalues).
    pub fn eigenvectors(&self) -> &Matrix {
        &self.ev
    }

    /// The number of QR iterations performed.
    pub fn iterations(&self) -> Size {
        self.iterations
    }
}

/// Numerical-Recipes stopping test: the off-diagonal `e[k]` counts as zero
/// once adding it to `|d[k-1]| + |d[k]|` no longer changes the sum in floating
/// point. The exact float comparison is the point of the test.
#[allow(clippy::float_cmp)]
fn off_diag_is_zero(k: Size, d: &Array, e: &Array) -> bool {
    d[k - 1].abs() + d[k].abs() == d[k - 1].abs() + d[k].abs() + e[k].abs()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOLERANCE: Real = 1.0e-10;

    #[test]
    fn eigenvalue_decomposition() {
        // Faithful port of testEigenValueDecomposition from
        // test-suite/tqreigendecomposition.cpp.
        let diag: Array = [11.0, 7.0, 6.0, 2.0, 0.0].into_iter().collect();
        let sub = Array::filled(4, 1.0);
        let expected = [
            11.2467832217139119,
            7.4854967362908535,
            5.5251516080277518,
            2.1811760273123308,
            -0.4386075933448487,
        ];

        let tqre = TqrEigenDecomposition::new(
            &diag,
            &sub,
            EigenVectorCalculation::WithoutEigenVector,
            ShiftStrategy::CloseEigenValue,
        );
        for (i, &ev) in expected.iter().enumerate() {
            let calculated = tqre.eigenvalues()[i];
            assert!(
                (ev - calculated).abs() <= TOLERANCE,
                "wrong eigenvalue: calculated {calculated}, expected {ev}"
            );
        }
    }

    #[test]
    fn zero_off_diagonal_eigenvalues() {
        // Faithful port of testZeroOffDiagEigenValues: exactly-zero and
        // nearly-zero subdiagonal entries must give the same spectrum.
        let diag: Array = [12.0, 9.0, 6.0, 3.0, 0.0].into_iter().collect();

        let mut sub = Array::filled(4, 1.0);
        sub[0] = 0.0;
        sub[2] = 0.0;
        let tqre1 = TqrEigenDecomposition::new(
            &diag,
            &sub,
            EigenVectorCalculation::WithEigenVector,
            ShiftStrategy::CloseEigenValue,
        );

        sub[0] = 1e-14;
        sub[2] = 1e-14;
        let tqre2 = TqrEigenDecomposition::new(
            &diag,
            &sub,
            EigenVectorCalculation::WithEigenVector,
            ShiftStrategy::CloseEigenValue,
        );

        for i in 0..diag.size() {
            let expected = tqre2.eigenvalues()[i];
            let calculated = tqre1.eigenvalues()[i];
            assert!(
                (expected - calculated).abs() <= TOLERANCE,
                "wrong eigenvalue: calculated {calculated}, expected {expected}"
            );
        }
    }

    #[test]
    fn eigenvector_decomposition() {
        // Faithful port of testEigenVectorDecomposition: for the 2x2 matrix
        // [[1, 1], [1, 1]] the four eigenvector components multiply to -1/4.
        let diag = Array::filled(2, 1.0);
        let sub = Array::filled(1, 1.0);

        let tqre = TqrEigenDecomposition::new(
            &diag,
            &sub,
            EigenVectorCalculation::WithEigenVector,
            ShiftStrategy::CloseEigenValue,
        );
        let ev = tqre.eigenvectors();
        let product = ev[(0, 0)] * ev[(0, 1)] * ev[(1, 0)] * ev[(1, 1)];
        assert!(
            (0.25 + product).abs() <= TOLERANCE,
            "wrong eigenvector: product {product}, expected -0.25"
        );
    }
}
