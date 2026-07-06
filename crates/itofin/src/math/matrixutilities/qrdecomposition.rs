//! QR decomposition and least-squares solve.
//!
//! Port of `ql/math/matrixutilities/qrdecomposition.{hpp,cpp}`: thin wrappers
//! over the MINPACK `qrfac`/`qrsolv` routines already ported in
//! `math::optimization::lmdif`. For an m x n matrix A (with optional column
//! pivoting P) computes A * P = Q * R with Q (m x n) orthogonal and R (n x n)
//! upper triangular, and solves min |A x - b| in the least-squares sense.
//! MINPACK works on column-major buffers, so the wrappers copy through flat
//! `Vec<Real>`s exactly like the C++ goes through transposed matrices.

use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::math::optimization::lmdif::{qrfac, qrsolv};
use crate::types::{Real, Size};

/// QR decomposition with optional column pivoting.
///
/// Returns `(q, r, ipvt)` such that `a * p == q * r`, where the permutation
/// matrix `p` is given by `p[ipvt[i]][i] = 1`. Without pivoting `ipvt` is the
/// identity permutation.
pub fn qr_decomposition(a: &Matrix, pivot: bool) -> (Matrix, Matrix, Vec<Size>) {
    let m = a.rows();
    let n = a.columns();

    // Column-major copy of A, i.e. the row-major layout of A^T that the C++
    // hands to MINPACK.
    let mut mt = vec![0.0; n * m];
    for i in 0..n {
        for j in 0..m {
            mt[i * m + j] = a[(j, i)];
        }
    }

    let mut lipvt = vec![0_usize; n];
    let mut rdiag = vec![0.0; n];
    let mut acnorm = vec![0.0; n];
    let mut wa = vec![0.0; n];

    qrfac(
        m,
        n,
        &mut mt,
        pivot,
        &mut lipvt,
        &mut rdiag,
        &mut acnorm,
        &mut wa,
    );

    let mut r = Matrix::with_size(n, n);
    for i in 0..n {
        r[(i, i)] = rdiag[i];
        if i < m {
            for k in i + 1..n {
                r[(i, k)] = mt[k * m + i];
            }
        }
    }

    let mut q = Matrix::with_size(m, n);
    if m > n {
        let u = n.min(m);
        for i in 0..u {
            q[(i, i)] = 1.0;
        }

        let mut v = vec![0.0; m];
        for i in (0..u).rev() {
            if mt[i * m + i].abs() > Real::EPSILON {
                let tau = 1.0 / mt[i * m + i];

                v[..i].fill(0.0);
                v[i..m].copy_from_slice(&mt[i * m + i..(i + 1) * m]);

                let mut w = vec![0.0; n];
                for (l, w_l) in w.iter_mut().enumerate() {
                    for k in i..m {
                        *w_l += v[k] * q[(k, l)];
                    }
                }

                for k in i..m {
                    let scale = tau * v[k];
                    for l in 0..n {
                        q[(k, l)] -= scale * w[l];
                    }
                }
            }
        }
    } else {
        let mut w = vec![0.0; m];
        for k in 0..m {
            w.fill(0.0);
            w[k] = 1.0;

            for j in 0..n.min(m) {
                let t3 = mt[j * m + j];
                if t3 != 0.0 {
                    let mut t = 0.0;
                    for i in j..m {
                        t += mt[j * m + i] * w[i];
                    }
                    t /= t3;
                    for i in j..m {
                        w[i] -= mt[j * m + i] * t;
                    }
                }
                q[(k, j)] = w[j];
            }
        }
    }

    let ipvt = if pivot { lipvt } else { (0..n).collect() };
    (q, r, ipvt)
}

/// Solves `a * x = b` in the least-squares sense via the QR decomposition,
/// with optional column pivoting and an optional diagonal scaling `d` as in
/// MINPACK's `qrsolv`.
///
/// # Panics
///
/// Panics if the dimensions of `b` or a non-empty `d` do not match `a`.
pub fn qr_solve(a: &Matrix, b: &Array, pivot: bool, d: Option<&Array>) -> Array {
    let m = a.rows();
    let n = a.columns();

    assert_eq!(b.len(), m, "dimensions of A and b don't match");
    if let Some(d) = d {
        assert!(
            d.len() == n || d.is_empty(),
            "dimensions of A and d don't match"
        );
    }

    let (q, r, ipvt) = qr_decomposition(a, pivot);

    // Column-major copy of R (the row-major layout of R^T in the C++).
    let mut rt = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            rt[n * j + i] = r[(i, j)];
        }
    }

    let mut ld = Array::with_size(n);
    if let Some(d) = d
        && !d.is_empty()
    {
        ld.copy_from_slice(d);
    }

    let qtb = &q.transpose() * b;

    let mut x = vec![0.0; n];
    let mut sdiag = vec![0.0; n];
    let mut wa = vec![0.0; n];
    qrsolv(n, &mut rt, n, &ipvt, &ld, &qtb, &mut x, &mut sdiag, &mut wa);

    x.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::matrixutilities::svd::Svd;
    use crate::math::matrixutilities::testsupport::{
        TestRng, identity, matrices_m1, matrices_m2, matrices_m3, matrices_m4, matrices_m5,
        matrices_m7, norm_matrix,
    };

    #[test]
    fn qr_decomposition_recovers_input() {
        let tol = 1.0e-12;
        let m3 = matrices_m3();
        let m4 = matrices_m4();
        let test_matrices = [
            matrices_m1(),
            matrices_m2(),
            identity(3),
            m3.clone(),
            m3.transpose(),
            m4.clone(),
            m4.transpose(),
            matrices_m5(),
        ];

        for a in test_matrices {
            let (q, r, ipvt) = qr_decomposition(&a, true);

            let mut p = Matrix::with_size(a.columns(), a.columns());
            for i in 0..p.columns() {
                p[(ipvt[i], i)] = 1.0;
            }

            let error = norm_matrix(&(&(&q * &r) - &(&a * &p)));
            assert!(error <= tol, "Q*R does not match A*P (norm = {error})");

            let (q, r, _) = qr_decomposition(&a, false);
            let error = norm_matrix(&(&(&q * &r) - &a));
            assert!(error <= tol, "Q*R does not match A (norm = {error})");
        }
    }

    #[test]
    fn qr_solve_matches_direct_and_least_squares_solutions() {
        let tol = 1.0e-12;
        let mut rng = TestRng::new(1234);

        let mut big = Matrix::with_size(50, 100);
        for i in 0..big.rows().min(big.columns()) {
            big[(i, i)] = (i + 1) as Real;
        }

        let mut rand = Matrix::with_size(50, 200);
        for i in 0..rand.rows() {
            for j in 0..rand.columns() {
                rand[(i, j)] = rng.next_real();
            }
        }

        let m3 = matrices_m3();
        let m4 = matrices_m4();
        let test_matrices = [
            matrices_m1(),
            matrices_m2(),
            m3.clone(),
            m3.transpose(),
            m4.clone(),
            m4.transpose(),
            matrices_m5(),
            identity(3),
            matrices_m7(),
            big.clone(),
            big.transpose(),
            rand.clone(),
            rand.transpose(),
        ];

        for a in test_matrices {
            for _ in 0..10 {
                let b: Array = (0..a.rows()).map(|_| rng.next_real()).collect();
                let x = qr_solve(&a, &b, true, None);

                if a.columns() >= a.rows() {
                    let error = (&(&a * &x) - &b).norm2();
                    assert!(error <= tol, "A*x does not match b (norm = {error})");
                } else {
                    // use the SVD to calculate the reference values
                    let n = a.columns();
                    let mut xr = Array::with_size(n);

                    let svd = Svd::new(&a);
                    let v = svd.v();
                    let u = svd.u();
                    let w = svd.singular_values();
                    let threshold = n as Real * Real::EPSILON;

                    for i in 0..n {
                        if w[i] > threshold {
                            let mut t = 0.0;
                            for row in 0..u.rows() {
                                t += u[(row, i)] * b[row];
                            }
                            t /= w[i];
                            for j in 0..n {
                                xr[j] += t * v[(j, i)];
                            }
                        }
                    }

                    let error = (&xr - &x).norm2();
                    assert!(
                        error <= tol,
                        "least square solution does not match (norm = {error})"
                    );
                }
            }
        }
    }
}
