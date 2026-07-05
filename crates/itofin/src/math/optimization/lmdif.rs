//! MINPACK routines backing the Levenberg-Marquardt method.
//!
//! Port of `ql/math/optimization/lmdif.{hpp,cpp}`, itself the C translation
//! of the MINPACK Fortran (Argonne National Laboratory, Garbow, Hillstrom,
//! More, 1980). Matrices are stored column-major in flat slices, as in the
//! original; the arithmetic constants `MACHEP` and `DWARF` keep MINPACK's
//! values so results match QuantLib's.

use crate::types::Real;

/// Resolution of arithmetic assumed by MINPACK.
pub(crate) const MACHEP: Real = 1.2e-16;

/// Computes the Euclidean norm of `x`.
///
/// The sum of squares is accumulated in three scaled sums (small,
/// intermediate, large components) so that neither overflow nor destructive
/// underflow occurs.
pub fn enorm(x: &[Real]) -> Real {
    const RDWARF: Real = 3.834e-20;
    const RGIANT: Real = 1.304e19;

    let mut s1 = 0.0;
    let mut s2 = 0.0;
    let mut s3 = 0.0;
    let mut x1max = 0.0;
    let mut x3max = 0.0;
    let agiant = RGIANT / x.len() as Real;

    for &value in x {
        let xabs = value.abs();
        if xabs > RDWARF && xabs < agiant {
            // sum for intermediate components
            s2 += xabs * xabs;
        } else if xabs > RDWARF {
            // sum for large components
            if xabs > x1max {
                let temp = x1max / xabs;
                s1 = 1.0 + s1 * temp * temp;
                x1max = xabs;
            } else {
                let temp = xabs / x1max;
                s1 += temp * temp;
            }
        } else if xabs > x3max {
            // sum for small components
            let temp = x3max / xabs;
            s3 = 1.0 + s3 * temp * temp;
            x3max = xabs;
        } else if xabs != 0.0 {
            let temp = xabs / x3max;
            s3 += temp * temp;
        }
    }

    if s1 != 0.0 {
        x1max * (s1 + (s2 / x1max) / x1max).sqrt()
    } else if s2 != 0.0 {
        let temp = if s2 >= x3max {
            s2 * (1.0 + (x3max / s2) * (x3max * s3))
        } else {
            x3max * ((s2 / x3max) + (x3max * s3))
        };
        temp.sqrt()
    } else {
        x3max * s3.sqrt()
    }
}

/// Computes the QR factorization `A * P = Q * R` of the column-major `m` by
/// `n` matrix `a` by Householder transformations with optional column
/// pivoting.
///
/// On exit the strict upper trapezoid of `a` holds the corresponding part of
/// `R` (its diagonal is returned in `rdiag`, with nonincreasing magnitude
/// when pivoting), while the lower trapezoid holds a factored form of `Q`.
/// `ipvt` receives the permutation (column `j` of `A * P` is column
/// `ipvt[j]` of `A`), `acnorm` the input column norms, and `wa` is a work
/// array of length `n`.
///
/// # Panics
///
/// Like the original, sizes are the caller's contract: panics if `a` is
/// shorter than `m * n` or if `ipvt`, `rdiag`, `acnorm` or `wa` are shorter
/// than `n`.
#[allow(clippy::too_many_arguments)]
pub fn qrfac(
    m: usize,
    n: usize,
    a: &mut [Real],
    pivot: bool,
    ipvt: &mut [usize],
    rdiag: &mut [Real],
    acnorm: &mut [Real],
    wa: &mut [Real],
) {
    const P05: Real = 0.05;

    // compute the initial column norms and initialize several arrays
    for j in 0..n {
        acnorm[j] = enorm(&a[m * j..m * (j + 1)]);
        rdiag[j] = acnorm[j];
        wa[j] = rdiag[j];
        if pivot {
            ipvt[j] = j;
        }
    }

    for j in 0..m.min(n) {
        if pivot {
            // bring the column of largest norm into the pivot position
            let mut kmax = j;
            for k in j..n {
                if rdiag[k] > rdiag[kmax] {
                    kmax = k;
                }
            }
            if kmax != j {
                for i in 0..m {
                    a.swap(i + m * j, i + m * kmax);
                }
                rdiag[kmax] = rdiag[j];
                wa[kmax] = wa[j];
                ipvt.swap(j, kmax);
            }
        }

        // compute the householder transformation to reduce the j-th column
        // of a to a multiple of the j-th unit vector
        let jj = j + m * j;
        let mut ajnorm = enorm(&a[jj..m * (j + 1)]);
        if ajnorm != 0.0 {
            if a[jj] < 0.0 {
                ajnorm = -ajnorm;
            }
            for i in j..m {
                a[i + m * j] /= ajnorm;
            }
            a[jj] += 1.0;

            // apply the transformation to the remaining columns and update
            // the norms
            let jp1 = j + 1;
            for k in jp1..n {
                let mut sum = 0.0;
                for i in j..m {
                    sum += a[i + m * j] * a[i + m * k];
                }
                let temp = sum / a[j + m * j];
                for i in j..m {
                    a[i + m * k] -= temp * a[i + m * j];
                }
                if pivot && rdiag[k] != 0.0 {
                    let temp = a[j + m * k] / rdiag[k];
                    rdiag[k] *= (1.0 - temp * temp).max(0.0).sqrt();
                    let temp = rdiag[k] / wa[k];
                    if P05 * temp * temp <= MACHEP {
                        rdiag[k] = enorm(&a[jp1 + m * k..m * (k + 1)]);
                        wa[k] = rdiag[k];
                    }
                }
            }
        }
        rdiag[j] = -ajnorm;
    }
}

/// Solves the least-squares system `A*x = b`, `D*x = 0` given the QR
/// factorization of `A * P = Q * R`.
///
/// `r` holds `R` in its upper triangle (leading dimension `ldr`), `ipvt` the
/// permutation from [`qrfac`], `diag` the diagonal of `D` and `qtb` the
/// first `n` elements of `Q^T * b`. On exit `x` holds the solution, `sdiag`
/// the diagonal of the upper triangular factor `S` with
/// `P^T * (A^T*A + D*D) * P = S^T * S`, the strict lower triangle of `r`
/// the transposed strict upper triangle of `S`, and `wa` is a work array of
/// length `n`.
///
/// # Panics
///
/// Like the original, sizes are the caller's contract: panics if `r` is
/// shorter than `ldr * n` with `ldr >= n`, if `ipvt`, `qtb`, `x`, `sdiag`
/// or `wa` are shorter than `n`, or if an `ipvt` entry is not a valid index
/// into `diag`.
#[allow(clippy::too_many_arguments)]
pub fn qrsolv(
    n: usize,
    r: &mut [Real],
    ldr: usize,
    ipvt: &[usize],
    diag: &[Real],
    qtb: &[Real],
    x: &mut [Real],
    sdiag: &mut [Real],
    wa: &mut [Real],
) {
    const P5: Real = 0.5;
    const P25: Real = 0.25;

    // copy r and qtb to preserve input and initialize s; in particular,
    // save the diagonal elements of r in x
    for j in 0..n {
        let kk = j + ldr * j;
        for i in j..n {
            r[i + ldr * j] = r[j + ldr * i];
        }
        x[j] = r[kk];
        wa[j] = qtb[j];
    }

    // eliminate the diagonal matrix d using a givens rotation
    for j in 0..n {
        // prepare the row of d to be eliminated, locating the diagonal
        // element using p from the qr factorization
        let l = ipvt[j];
        if diag[l] != 0.0 {
            for item in sdiag.iter_mut().take(n).skip(j) {
                *item = 0.0;
            }
            sdiag[j] = diag[l];

            // the transformations to eliminate the row of d modify only a
            // single element of qtb beyond the first n, which is initially
            // zero
            let mut qtbpj = 0.0;
            for k in j..n {
                // determine a givens rotation which eliminates the
                // appropriate element in the current row of d
                if sdiag[k] == 0.0 {
                    continue;
                }
                let kk = k + ldr * k;
                let (cos, sin) = if r[kk].abs() < sdiag[k].abs() {
                    let cotan = r[kk] / sdiag[k];
                    let sin = P5 / (P25 + P25 * cotan * cotan).sqrt();
                    (sin * cotan, sin)
                } else {
                    let tan = sdiag[k] / r[kk];
                    let cos = P5 / (P25 + P25 * tan * tan).sqrt();
                    (cos, cos * tan)
                };

                // compute the modified diagonal element of r and the
                // modified element of (qtb, 0)
                r[kk] = cos * r[kk] + sin * sdiag[k];
                let temp = cos * wa[k] + sin * qtbpj;
                qtbpj = -sin * wa[k] + cos * qtbpj;
                wa[k] = temp;

                // accumulate the transformation in the row of s
                for i in (k + 1)..n {
                    let temp = cos * r[i + ldr * k] + sin * sdiag[i];
                    sdiag[i] = -sin * r[i + ldr * k] + cos * sdiag[i];
                    r[i + ldr * k] = temp;
                }
            }
        }

        // store the diagonal element of s and restore the corresponding
        // diagonal element of r
        let kk = j + ldr * j;
        sdiag[j] = r[kk];
        r[kk] = x[j];
    }

    // solve the triangular system for z; if the system is singular, obtain
    // a least squares solution
    let mut nsing = n;
    for j in 0..n {
        if sdiag[j] == 0.0 && nsing == n {
            nsing = j;
        }
        if nsing < n {
            wa[j] = 0.0;
        }
    }
    for k in 0..nsing {
        let j = nsing - k - 1;
        let mut sum = 0.0;
        for i in (j + 1)..nsing {
            sum += r[i + ldr * j] * wa[i];
        }
        wa[j] = (wa[j] - sum) / sdiag[j];
    }

    // permute the components of z back to components of x
    for j in 0..n {
        x[ipvt[j]] = wa[j];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOL: Real = 1e-12;

    #[test]
    fn enorm_matches_naive_norm_on_moderate_values() {
        let x = [3.0, -4.0, 12.0];
        assert!((enorm(&x) - 13.0).abs() < TOL);
        assert_eq!(enorm(&[0.0, 0.0]), 0.0);
    }

    #[test]
    fn enorm_avoids_overflow_and_underflow() {
        let large = [1.0e200, 1.0e200];
        assert!((enorm(&large) - 2.0_f64.sqrt() * 1.0e200).abs() < 1.0e188);
        let small = [3.0e-200, 4.0e-200];
        assert!((enorm(&small) - 5.0e-200).abs() < 1.0e-212);
    }

    #[test]
    fn qrfac_reproduces_normal_equations_of_permuted_matrix() {
        // column-major 3x2 matrix [[1, 4], [2, 5], [3, 6]]
        let (m, n) = (3, 2);
        let original = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let mut a = original;
        let mut ipvt = [0_usize; 2];
        let mut rdiag = [0.0; 2];
        let mut acnorm = [0.0; 2];
        let mut wa = [0.0; 2];
        qrfac(
            m,
            n,
            &mut a,
            true,
            &mut ipvt,
            &mut rdiag,
            &mut acnorm,
            &mut wa,
        );

        // input column norms are reported, the larger column is pivoted first
        assert!((acnorm[0] - 14.0_f64.sqrt()).abs() < TOL);
        assert!((acnorm[1] - 77.0_f64.sqrt()).abs() < TOL);
        assert_eq!(ipvt, [1, 0]);
        assert!(rdiag[0].abs() >= rdiag[1].abs());

        // R^T R must equal (A*P)^T (A*P) since Q is orthogonal
        let r = [[rdiag[0], a[m]], [0.0, rdiag[1]]];
        for i in 0..n {
            for j in 0..n {
                let rtr: Real = (0..n).map(|k| r[k][i] * r[k][j]).sum();
                let ata: Real = (0..m)
                    .map(|k| original[k + m * ipvt[i]] * original[k + m * ipvt[j]])
                    .sum();
                assert!((rtr - ata).abs() < 1e-10, "mismatch at ({i}, {j})");
            }
        }
    }

    #[test]
    fn qrsolv_solves_triangular_system_without_damping() {
        // R = [[2, 1], [0, 3]] stored column-major with ldr = 2
        let n = 2;
        let mut r = [2.0, 0.0, 1.0, 3.0];
        let ipvt = [0_usize, 1];
        let diag = [0.0, 0.0];
        let qtb = [5.0, 6.0];
        let mut x = [0.0; 2];
        let mut sdiag = [0.0; 2];
        let mut wa = [0.0; 2];
        qrsolv(
            n, &mut r, 2, &ipvt, &diag, &qtb, &mut x, &mut sdiag, &mut wa,
        );
        // R x = qtb => x1 = 2, x0 = (5 - 1*2)/2 = 1.5
        assert!((x[0] - 1.5).abs() < TOL);
        assert!((x[1] - 2.0).abs() < TOL);
    }

    #[test]
    fn qrsolv_applies_diagonal_damping() {
        // R = I, D = diag(1, 2): (R^T R + D^T D) x = qtb
        let n = 2;
        let mut r = [1.0, 0.0, 0.0, 1.0];
        let ipvt = [0_usize, 1];
        let diag = [1.0, 2.0];
        let qtb = [4.0, 10.0];
        let mut x = [0.0; 2];
        let mut sdiag = [0.0; 2];
        let mut wa = [0.0; 2];
        qrsolv(
            n, &mut r, 2, &ipvt, &diag, &qtb, &mut x, &mut sdiag, &mut wa,
        );
        assert!((x[0] - 4.0 / 2.0).abs() < TOL);
        assert!((x[1] - 10.0 / 5.0).abs() < TOL);
    }
}
