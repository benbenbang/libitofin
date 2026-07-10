//! Singular value decomposition.
//!
//! Port of `ql/math/matrixutilities/svd.{hpp,cpp}` (itself adapted from the
//! NIST TNT project): for an m x n matrix M with m >= n, computes
//! M = U * S * V^T with U (m x n) and V (n x n) orthogonal and S the diagonal
//! matrix of singular values, sorted in decreasing order. Matrices with
//! m < n are handled by decomposing the transpose and swapping U and V.
//! Indices are kept signed as in the C++ (the bidiagonalization and QR sweeps
//! count down to -1), with casts only at the access sites.

use crate::math::array::Array;
use crate::math::matrix::Matrix;
use crate::types::{Real, Size};

/// Returns the hypotenuse of `a` and `b` avoiding under/overflow, matching
/// the TNT helper rather than `Real::hypot` so sequences match the C++.
fn hypot(a: Real, b: Real) -> Real {
    if a == 0.0 {
        b.abs()
    } else {
        let c = b / a;
        a.abs() * (1.0 + c * c).sqrt()
    }
}

/// Applies the Givens rotation `(cs, sn)` to columns `j` and `k` of `mat`.
fn rotate_columns(mat: &mut Matrix, rows: isize, cs: Real, sn: Real, j: isize, k: isize) {
    for i in 0..rows as usize {
        let t = cs * mat[(i, j as usize)] + sn * mat[(i, k as usize)];
        mat[(i, k as usize)] = -sn * mat[(i, j as usize)] + cs * mat[(i, k as usize)];
        mat[(i, j as usize)] = t;
    }
}

/// Swaps columns `j` and `k` of `mat`.
fn swap_columns(mat: &mut Matrix, rows: isize, j: isize, k: isize) {
    for i in 0..rows as usize {
        let t = mat[(i, j as usize)];
        mat[(i, j as usize)] = mat[(i, k as usize)];
        mat[(i, k as usize)] = t;
    }
}

/// Singular value decomposition of a real matrix.
#[derive(Clone, Debug)]
pub struct Svd {
    u: Matrix,
    v: Matrix,
    s: Array,
    m: isize,
    n: isize,
    transposed: bool,
}

impl Svd {
    /// Decomposes `mat`, which must be non-empty.
    ///
    /// # Panics
    ///
    /// Panics if `mat` is empty.
    #[allow(clippy::needless_range_loop)]
    pub fn new(mat: &Matrix) -> Self {
        assert!(mat.rows() > 0 && mat.columns() > 0, "null matrix given");

        let (mut a, transposed) = if mat.rows() >= mat.columns() {
            (mat.clone(), false)
        } else {
            (mat.transpose(), true)
        };

        let m = a.rows() as isize;
        let n = a.columns() as isize;

        let mut s = Array::with_size(n as usize);
        let mut u = Matrix::with_size(m as usize, n as usize);
        let mut v = Matrix::with_size(n as usize, n as usize);
        let mut e = Array::with_size(n as usize);
        let mut work = Array::with_size(m as usize);

        let at = |a: &Matrix, i: isize, j: isize| a[(i as usize, j as usize)];

        // Reduce A to bidiagonal form, storing the diagonal elements in s and
        // the super-diagonal elements in e.
        let nct = (m - 1).min(n);
        let nrt = 0.max(n - 2);
        for k in 0..nct.max(nrt) {
            if k < nct {
                s[k as usize] = 0.0;
                for i in k..m {
                    s[k as usize] = hypot(s[k as usize], at(&a, i, k));
                }
                if s[k as usize] != 0.0 {
                    if at(&a, k, k) < 0.0 {
                        s[k as usize] = -s[k as usize];
                    }
                    for i in k..m {
                        a[(i as usize, k as usize)] /= s[k as usize];
                    }
                    a[(k as usize, k as usize)] += 1.0;
                }
                s[k as usize] = -s[k as usize];
            }
            for j in k + 1..n {
                if k < nct && s[k as usize] != 0.0 {
                    let mut t = 0.0;
                    for i in k..m {
                        t += at(&a, i, k) * at(&a, i, j);
                    }
                    t = -t / at(&a, k, k);
                    for i in k..m {
                        a[(i as usize, j as usize)] += t * at(&a, i, k);
                    }
                }
                e[j as usize] = at(&a, k, j);
            }
            if k < nct {
                for i in k..m {
                    u[(i as usize, k as usize)] = at(&a, i, k);
                }
            }
            if k < nrt {
                e[k as usize] = 0.0;
                for i in k + 1..n {
                    e[k as usize] = hypot(e[k as usize], e[i as usize]);
                }
                if e[k as usize] != 0.0 {
                    if e[(k + 1) as usize] < 0.0 {
                        e[k as usize] = -e[k as usize];
                    }
                    for i in k + 1..n {
                        e[i as usize] /= e[k as usize];
                    }
                    e[(k + 1) as usize] += 1.0;
                }
                e[k as usize] = -e[k as usize];
                if k + 1 < m && e[k as usize] != 0.0 {
                    for i in k + 1..m {
                        work[i as usize] = 0.0;
                    }
                    for j in k + 1..n {
                        for i in k + 1..m {
                            work[i as usize] += e[j as usize] * at(&a, i, j);
                        }
                    }
                    for j in k + 1..n {
                        let t = -e[j as usize] / e[(k + 1) as usize];
                        for i in k + 1..m {
                            a[(i as usize, j as usize)] += t * work[i as usize];
                        }
                    }
                }
                for i in k + 1..n {
                    v[(i as usize, k as usize)] = e[i as usize];
                }
            }
        }

        // Set up the final bidiagonal matrix of order n.
        if nct < n {
            s[nct as usize] = at(&a, nct, nct);
        }
        if nrt + 1 < n {
            e[nrt as usize] = at(&a, nrt, n - 1);
        }
        e[(n - 1) as usize] = 0.0;

        // Generate U.
        for j in nct..n {
            for i in 0..m {
                u[(i as usize, j as usize)] = 0.0;
            }
            u[(j as usize, j as usize)] = 1.0;
        }
        for k in (0..nct).rev() {
            if s[k as usize] != 0.0 {
                for j in k + 1..n {
                    let mut t = 0.0;
                    for i in k..m {
                        t += at(&u, i, k) * at(&u, i, j);
                    }
                    t = -t / at(&u, k, k);
                    for i in k..m {
                        u[(i as usize, j as usize)] += t * at(&u, i, k);
                    }
                }
                for i in k..m {
                    u[(i as usize, k as usize)] = -at(&u, i, k);
                }
                u[(k as usize, k as usize)] += 1.0;
                for i in 0..k - 1 {
                    u[(i as usize, k as usize)] = 0.0;
                }
            } else {
                for i in 0..m {
                    u[(i as usize, k as usize)] = 0.0;
                }
                u[(k as usize, k as usize)] = 1.0;
            }
        }

        // Generate V.
        for k in (0..n).rev() {
            if k < nrt && e[k as usize] != 0.0 {
                for j in k + 1..n {
                    let mut t = 0.0;
                    for i in k + 1..n {
                        t += at(&v, i, k) * at(&v, i, j);
                    }
                    t = -t / at(&v, k + 1, k);
                    for i in k + 1..n {
                        v[(i as usize, j as usize)] += t * at(&v, i, k);
                    }
                }
            }
            for i in 0..n {
                v[(i as usize, k as usize)] = 0.0;
            }
            v[(k as usize, k as usize)] = 1.0;
        }

        // Main iteration loop for the singular values.
        let mut p = n;
        let pp = p - 1;
        let eps: Real = 2.0_f64.powi(-52);
        while p > 0 {
            // Inspect for negligible elements in the s and e arrays; on
            // completion kase and k are set as follows:
            // kase = 1  s(p) and e[k-1] are negligible and k < p
            // kase = 2  s(k) is negligible and k < p
            // kase = 3  e[k-1] is negligible, k < p, and s(k..p) are not (qr step)
            // kase = 4  e(p-1) is negligible (convergence)
            let mut k = p - 2;
            while k >= -1 {
                if k == -1 {
                    break;
                }
                if e[k as usize].abs() <= eps * (s[k as usize].abs() + s[(k + 1) as usize].abs()) {
                    e[k as usize] = 0.0;
                    break;
                }
                k -= 1;
            }
            let kase;
            if k == p - 2 {
                kase = 4;
            } else {
                let mut ks = p - 1;
                while ks >= k {
                    if ks == k {
                        break;
                    }
                    let t = if ks != p { e[ks as usize].abs() } else { 0.0 }
                        + if ks != k + 1 {
                            e[(ks - 1) as usize].abs()
                        } else {
                            0.0
                        };
                    if s[ks as usize].abs() <= eps * t {
                        s[ks as usize] = 0.0;
                        break;
                    }
                    ks -= 1;
                }
                if ks == k {
                    kase = 3;
                } else if ks == p - 1 {
                    kase = 1;
                } else {
                    kase = 2;
                    k = ks;
                }
            }
            k += 1;

            match kase {
                // Deflate negligible s(p).
                1 => {
                    let mut f = e[(p - 2) as usize];
                    e[(p - 2) as usize] = 0.0;
                    for j in (k..=p - 2).rev() {
                        let t = hypot(s[j as usize], f);
                        let cs = s[j as usize] / t;
                        let sn = f / t;
                        s[j as usize] = t;
                        if j != k {
                            f = -sn * e[(j - 1) as usize];
                            e[(j - 1) as usize] *= cs;
                        }
                        rotate_columns(&mut v, n, cs, sn, j, p - 1);
                    }
                }

                // Split at negligible s(k).
                2 => {
                    let mut f = e[(k - 1) as usize];
                    e[(k - 1) as usize] = 0.0;
                    for j in k..p {
                        let t = hypot(s[j as usize], f);
                        let cs = s[j as usize] / t;
                        let sn = f / t;
                        s[j as usize] = t;
                        f = -sn * e[j as usize];
                        e[j as usize] *= cs;
                        rotate_columns(&mut u, m, cs, sn, j, k - 1);
                    }
                }

                // Perform one qr step.
                3 => {
                    let scale = s[(p - 1) as usize]
                        .abs()
                        .max(s[(p - 2) as usize].abs())
                        .max(e[(p - 2) as usize].abs())
                        .max(s[k as usize].abs())
                        .max(e[k as usize].abs());
                    let sp = s[(p - 1) as usize] / scale;
                    let spm1 = s[(p - 2) as usize] / scale;
                    let epm1 = e[(p - 2) as usize] / scale;
                    let sk = s[k as usize] / scale;
                    let ek = e[k as usize] / scale;
                    let b = ((spm1 + sp) * (spm1 - sp) + epm1 * epm1) / 2.0;
                    let c = (sp * epm1) * (sp * epm1);
                    let mut shift = 0.0;
                    if b != 0.0 || c != 0.0 {
                        shift = (b * b + c).sqrt();
                        if b < 0.0 {
                            shift = -shift;
                        }
                        shift = c / (b + shift);
                    }
                    let mut f = (sk + sp) * (sk - sp) + shift;
                    let mut g = sk * ek;

                    // Chase zeros.
                    for j in k..p - 1 {
                        let mut t = hypot(f, g);
                        let mut cs = f / t;
                        let mut sn = g / t;
                        if j != k {
                            e[(j - 1) as usize] = t;
                        }
                        f = cs * s[j as usize] + sn * e[j as usize];
                        e[j as usize] = cs * e[j as usize] - sn * s[j as usize];
                        g = sn * s[(j + 1) as usize];
                        s[(j + 1) as usize] *= cs;
                        rotate_columns(&mut v, n, cs, sn, j, j + 1);
                        t = hypot(f, g);
                        cs = f / t;
                        sn = g / t;
                        s[j as usize] = t;
                        f = cs * e[j as usize] + sn * s[(j + 1) as usize];
                        s[(j + 1) as usize] = -sn * e[j as usize] + cs * s[(j + 1) as usize];
                        g = sn * e[(j + 1) as usize];
                        e[(j + 1) as usize] *= cs;
                        if j < m - 1 {
                            rotate_columns(&mut u, m, cs, sn, j, j + 1);
                        }
                    }
                    e[(p - 2) as usize] = f;
                }

                // Convergence.
                _ => {
                    // Make the singular values positive.
                    if s[k as usize] <= 0.0 {
                        s[k as usize] = if s[k as usize] < 0.0 {
                            -s[k as usize]
                        } else {
                            0.0
                        };
                        for i in 0..=pp {
                            v[(i as usize, k as usize)] = -at(&v, i, k);
                        }
                    }

                    // Order the singular values.
                    while k < pp {
                        if s[k as usize] >= s[(k + 1) as usize] {
                            break;
                        }
                        let t = s[k as usize];
                        s[k as usize] = s[(k + 1) as usize];
                        s[(k + 1) as usize] = t;
                        if k < n - 1 {
                            swap_columns(&mut v, n, k, k + 1);
                        }
                        if k < m - 1 {
                            swap_columns(&mut u, m, k, k + 1);
                        }
                        k += 1;
                    }
                    p -= 1;
                }
            }
        }

        Svd {
            u,
            v,
            s,
            m,
            n,
            transposed,
        }
    }

    /// The left singular vectors (an m x n matrix for the original m x n
    /// input; V of the transposed problem when the input had m < n).
    pub fn u(&self) -> &Matrix {
        if self.transposed { &self.v } else { &self.u }
    }

    /// The right singular vectors (an n x n matrix).
    pub fn v(&self) -> &Matrix {
        if self.transposed { &self.u } else { &self.v }
    }

    /// The singular values, sorted in decreasing order.
    pub fn singular_values(&self) -> &Array {
        &self.s
    }

    /// The diagonal matrix S of singular values.
    pub fn s(&self) -> Matrix {
        let n = self.n as usize;
        let mut s = Matrix::with_size(n, n);
        for i in 0..n {
            s[(i, i)] = self.s[i];
        }
        s
    }

    /// The 2-norm of the input matrix (its largest singular value).
    pub fn norm2(&self) -> Real {
        self.s[0]
    }

    /// The condition number (ratio of largest to smallest singular value).
    pub fn cond(&self) -> Real {
        self.s[0] / self.s[(self.n - 1) as usize]
    }

    /// The numerical rank (number of singular values above the tolerance
    /// `m * s[0] * eps`).
    pub fn rank(&self) -> Size {
        let tol = self.m as Real * self.s[0] * Real::EPSILON;
        self.s.iter().filter(|&&x| x > tol).count()
    }

    /// The minimum-norm least-squares solution of `M x = b` via the
    /// pseudo-inverse.
    ///
    /// Divergence: `SVD::solveFor` (`svd.cpp:528`) has no dimension check; the
    /// mismatch surfaces deep inside `Matrix * Array`. The assertion names the
    /// caller's mistake at the boundary instead.
    ///
    /// # Panics
    ///
    /// Panics if `b` is not as long as the decomposed matrix has rows.
    pub fn solve_for(&self, b: &Array) -> Array {
        assert_eq!(
            b.len(),
            self.u().rows(),
            "dimensions of SVD input and b don't match"
        );
        let n = self.n as usize;
        let mut w = Matrix::with_size(n, n);
        for i in 0..self.rank() {
            w[(i, i)] = 1.0 / self.s[i];
        }
        let inverse = &(self.v() * &w) * &self.u().transpose();
        &inverse * b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::matrixutilities::testsupport::{
        identity, matrices_m1, matrices_m2, matrices_m3, matrices_m4, norm_matrix,
    };

    #[test]
    fn singular_value_decomposition() {
        let tol = 1.0e-12;
        let identity = identity(3);

        for a in [matrices_m1(), matrices_m2(), matrices_m3(), matrices_m4()] {
            let svd = Svd::new(&a);
            let u = svd.u();
            let singular = svd.singular_values();
            let s = svd.s();
            let v = svd.v();

            for i in 0..s.rows() {
                assert_eq!(s[(i, i)], singular[i], "S not consistent with s");
            }

            let u_error = norm_matrix(&(&(&u.transpose() * u) - &identity));
            assert!(u_error <= tol, "U not orthogonal ({u_error})");

            let v_error = norm_matrix(&(&(&v.transpose() * v) - &identity));
            assert!(v_error <= tol, "V not orthogonal ({v_error})");

            let reconstructed = &(u * &s) * &v.transpose();
            let error = norm_matrix(&(&reconstructed - &a));
            assert!(error <= tol, "product does not recover A ({error})");
        }
    }

    #[test]
    fn solve_for_rejects_wrong_rhs_dimension() {
        let svd = Svd::new(&matrices_m1());
        let result = std::panic::catch_unwind(|| {
            let _ = svd.solve_for(&Array::from([1.0, 2.0]));
        });
        assert!(result.is_err());
    }
}
