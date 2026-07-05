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
/// Smallest nonzero number assumed by MINPACK.
pub(crate) const DWARF: Real = 1.0e-38;

/// The user-supplied functions minimized by [`lmdif`].
///
/// The C original takes plain function pointers for the residuals and the
/// optional analytic Jacobian; a single trait keeps both callbacks able to
/// share mutable state.
pub trait LmdifCostFunction {
    /// Evaluates the `m` functions at `x`, writing them into `fvec`.
    fn fcn(&mut self, x: &[Real], fvec: &mut [Real]);

    /// Whether [`Self::jacobian`] is available; when `false`, [`lmdif`] uses
    /// the forward-difference approximation [`fdjac2`].
    fn has_jacobian(&self) -> bool {
        false
    }

    /// Evaluates the `m` by `n` Jacobian at `x` into the column-major `fjac`.
    ///
    /// # Panics
    ///
    /// The default implementation panics; [`lmdif`] only calls it when
    /// [`Self::has_jacobian`] returns `true`.
    fn jacobian(&mut self, _x: &[Real], _fjac: &mut [Real]) {
        unimplemented!("no user-supplied jacobian")
    }
}

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

/// Computes a forward-difference approximation to the `m` by `n` Jacobian at
/// `x` into the column-major `fjac`.
///
/// `fvec` must contain the functions evaluated at `x`; `epsfcn` estimates
/// the relative error in the functions (machine precision is assumed when it
/// is smaller); `wa` is a work array of length `m`. `x` is perturbed and
/// restored in place.
#[allow(clippy::too_many_arguments)]
pub fn fdjac2(
    m: usize,
    n: usize,
    x: &mut [Real],
    fvec: &[Real],
    fjac: &mut [Real],
    epsfcn: Real,
    wa: &mut [Real],
    fcn: &mut dyn LmdifCostFunction,
) {
    let eps = epsfcn.max(MACHEP).sqrt();
    for j in 0..n {
        let temp = x[j];
        let mut h = eps * temp.abs();
        if h == 0.0 {
            h = eps;
        }
        x[j] = temp + h;
        fcn.fcn(x, wa);
        x[j] = temp;
        for i in 0..m {
            fjac[i + m * j] = (wa[i] - fvec[i]) / h;
        }
    }
}

/// Determines the Levenberg-Marquardt parameter `par` and the corresponding
/// step `x` such that, with `dxnorm = ||D*x||`, either `par` is zero and
/// `dxnorm - delta <= 0.1 * delta`, or `par` is positive and
/// `|dxnorm - delta| <= 0.1 * delta`.
///
/// `r`, `ldr` and `ipvt` hold the QR factorization from [`qrfac`], `diag`
/// the scaling `D` and `qtb` the first `n` elements of `Q^T * b`. On exit
/// `x` holds the step, `sdiag` and the strict lower triangle of `r` the
/// factor `S` from [`qrsolv`]; `wa1` and `wa2` are work arrays of length
/// `n`.
#[allow(clippy::too_many_arguments)]
pub fn lmpar(
    n: usize,
    r: &mut [Real],
    ldr: usize,
    ipvt: &[usize],
    diag: &[Real],
    qtb: &[Real],
    delta: Real,
    par: &mut Real,
    x: &mut [Real],
    sdiag: &mut [Real],
    wa1: &mut [Real],
    wa2: &mut [Real],
) {
    const P1: Real = 0.1;
    const P001: Real = 0.001;

    // compute and store in x the gauss-newton direction; if the jacobian is
    // rank-deficient, obtain a least squares solution
    let mut nsing = n;
    for j in 0..n {
        wa1[j] = qtb[j];
        if r[j + ldr * j] == 0.0 && nsing == n {
            nsing = j;
        }
        if nsing < n {
            wa1[j] = 0.0;
        }
    }
    for k in 0..nsing {
        let j = nsing - k - 1;
        wa1[j] /= r[j + ldr * j];
        let temp = wa1[j];
        for i in 0..j {
            wa1[i] -= r[i + ldr * j] * temp;
        }
    }
    for j in 0..n {
        x[ipvt[j]] = wa1[j];
    }

    // evaluate the function at the origin, and test for acceptance of the
    // gauss-newton direction
    let mut iter = 0;
    for j in 0..n {
        wa2[j] = diag[j] * x[j];
    }
    let mut dxnorm = enorm(&wa2[..n]);
    let mut fp = dxnorm - delta;
    if fp <= P1 * delta {
        *par = 0.0;
        return;
    }

    // if the jacobian is not rank deficient, the newton step provides a
    // lower bound, parl, for the zero of the function
    let mut parl = 0.0;
    if nsing >= n {
        for j in 0..n {
            let l = ipvt[j];
            wa1[j] = diag[l] * (wa2[l] / dxnorm);
        }
        for j in 0..n {
            let mut sum = 0.0;
            for i in 0..j {
                sum += r[i + ldr * j] * wa1[i];
            }
            wa1[j] = (wa1[j] - sum) / r[j + ldr * j];
        }
        let temp = enorm(&wa1[..n]);
        parl = ((fp / delta) / temp) / temp;
    }

    // calculate an upper bound, paru, for the zero of the function
    for j in 0..n {
        let mut sum = 0.0;
        for i in 0..=j {
            sum += r[i + ldr * j] * qtb[i];
        }
        wa1[j] = sum / diag[ipvt[j]];
    }
    let gnorm = enorm(&wa1[..n]);
    let mut paru = gnorm / delta;
    if paru == 0.0 {
        paru = DWARF / delta.min(P1);
    }

    // if the input par lies outside of the interval (parl, paru), set par
    // to the closer endpoint
    *par = (*par).max(parl).min(paru);
    if *par == 0.0 {
        *par = gnorm / dxnorm;
    }

    loop {
        iter += 1;
        // evaluate the function at the current value of par
        if *par == 0.0 {
            *par = DWARF.max(P001 * paru);
        }
        let temp = par.sqrt();
        for j in 0..n {
            wa1[j] = temp * diag[j];
        }
        qrsolv(n, r, ldr, ipvt, wa1, qtb, x, sdiag, wa2);
        for j in 0..n {
            wa2[j] = diag[j] * x[j];
        }
        dxnorm = enorm(&wa2[..n]);
        let temp = fp;
        fp = dxnorm - delta;

        // if the function is small enough, accept the current value of par;
        // also test for the exceptional cases where parl is zero or the
        // number of iterations has reached 10
        if fp.abs() <= P1 * delta || (parl == 0.0 && fp <= temp && temp < 0.0) || iter == 10 {
            return;
        }

        // compute the newton correction
        for j in 0..n {
            let l = ipvt[j];
            wa1[j] = diag[l] * (wa2[l] / dxnorm);
        }
        for j in 0..n {
            wa1[j] /= sdiag[j];
            let temp = wa1[j];
            for i in (j + 1)..n {
                wa1[i] -= r[i + ldr * j] * temp;
            }
        }
        let temp = enorm(&wa1[..n]);
        let parc = ((fp / delta) / temp) / temp;

        // depending on the sign of the function, update parl or paru
        if fp > 0.0 {
            parl = parl.max(*par);
        }
        if fp < 0.0 {
            paru = paru.min(*par);
        }
        *par = parl.max(*par + parc);
    }
}

/// Minimizes the sum of the squares of `m` nonlinear functions in `n`
/// variables by the Levenberg-Marquardt algorithm, approximating the
/// Jacobian by forward differences unless `fcn` supplies it.
///
/// `x` holds the initial estimate and, on exit, the final one; `fvec` (of
/// length `m`) receives the functions at `x`. `ftol`, `xtol` and `gtol`
/// bound the relative reductions, the relative step and the cosine of the
/// gradient angle; `maxfev` caps the function evaluations, `epsfcn` seeds
/// the finite-difference step and `factor` bounds the initial trust region.
/// The original's `diag`, `mode` and `nprint` arguments are fixed to
/// QuantLib's usage (internal scaling, no printing). As in the original,
/// the evaluation count grows by `n` per Jacobian pass even when the
/// analytic Jacobian is supplied, so `maxfev` keeps its MINPACK meaning of
/// roughly `iterations * (n + 1)` in both modes.
///
/// Returns the MINPACK `info` code: `0` improper input, `1`-`4` converged
/// (ftol/xtol/both/gtol), `5` `maxfev` reached, `6` `ftol` too small, `7`
/// `xtol` too small, `8` `gtol` too small.
#[allow(clippy::too_many_arguments)]
pub fn lmdif(
    m: usize,
    n: usize,
    x: &mut [Real],
    fvec: &mut [Real],
    ftol: Real,
    xtol: Real,
    gtol: Real,
    maxfev: usize,
    epsfcn: Real,
    factor: Real,
    fcn: &mut dyn LmdifCostFunction,
) -> i32 {
    const P1: Real = 0.1;
    const P5: Real = 0.5;
    const P25: Real = 0.25;
    const P75: Real = 0.75;
    const P0001: Real = 1.0e-4;

    if n == 0 || m < n || ftol < 0.0 || xtol < 0.0 || gtol < 0.0 || maxfev == 0 || factor <= 0.0 {
        return 0;
    }

    let mut diag = vec![0.0; n];
    let mut fjac = vec![0.0; m * n];
    let mut ipvt = vec![0_usize; n];
    let mut qtf = vec![0.0; n];
    let mut wa1 = vec![0.0; n];
    let mut wa2 = vec![0.0; n];
    let mut wa3 = vec![0.0; n];
    let mut wa4 = vec![0.0; m];

    // evaluate the function at the starting point and calculate its norm
    fcn.fcn(x, fvec);
    let mut nfev = 1;
    let mut fnorm = enorm(fvec);

    let mut par = 0.0;
    let mut iter = 1;
    let mut xnorm = 0.0;
    let mut delta = 0.0;
    let mut info = 0;

    // outer loop: one jacobian evaluation per pass
    'outer: loop {
        if fcn.has_jacobian() {
            fcn.jacobian(x, &mut fjac);
        } else {
            fdjac2(m, n, x, fvec, &mut fjac, epsfcn, &mut wa4, fcn);
        }
        nfev += n;

        // compute the qr factorization of the jacobian
        qrfac(
            m, n, &mut fjac, true, &mut ipvt, &mut wa1, &mut wa2, &mut wa3,
        );

        // on the first iteration scale according to the norms of the
        // columns of the initial jacobian and calculate the initial step
        // bound delta
        if iter == 1 {
            for j in 0..n {
                diag[j] = if wa2[j] == 0.0 { 1.0 } else { wa2[j] };
            }
            for j in 0..n {
                wa3[j] = diag[j] * x[j];
            }
            xnorm = enorm(&wa3);
            delta = factor * xnorm;
            if delta == 0.0 {
                delta = factor;
            }
        }

        // form q^T * fvec and store the first n components in qtf
        wa4.copy_from_slice(fvec);
        for j in 0..n {
            let jj = j + m * j;
            let temp3 = fjac[jj];
            if temp3 != 0.0 {
                let mut sum = 0.0;
                for i in j..m {
                    sum += fjac[i + m * j] * wa4[i];
                }
                let temp = -sum / temp3;
                for i in j..m {
                    wa4[i] += fjac[i + m * j] * temp;
                }
            }
            fjac[jj] = wa1[j];
            qtf[j] = wa4[j];
        }

        // compute the norm of the scaled gradient
        let mut gnorm: Real = 0.0;
        if fnorm != 0.0 {
            for j in 0..n {
                let l = ipvt[j];
                if wa2[l] != 0.0 {
                    let mut sum = 0.0;
                    for i in 0..=j {
                        sum += fjac[i + m * j] * (qtf[i] / fnorm);
                    }
                    gnorm = gnorm.max((sum / wa2[l]).abs());
                }
            }
        }

        // test for convergence of the gradient norm
        if gnorm <= gtol {
            info = 4;
            break 'outer;
        }

        // rescale
        for j in 0..n {
            diag[j] = diag[j].max(wa2[j]);
        }

        // inner loop: repeat until a successful iteration
        loop {
            // determine the levenberg-marquardt parameter
            lmpar(
                n, &mut fjac, m, &ipvt, &diag, &qtf, delta, &mut par, &mut wa1, &mut wa2, &mut wa3,
                &mut wa4,
            );

            // store the direction p and x + p; calculate the norm of p
            for j in 0..n {
                wa1[j] = -wa1[j];
                wa2[j] = x[j] + wa1[j];
                wa3[j] = diag[j] * wa1[j];
            }
            let pnorm = enorm(&wa3);

            // on the first iteration, adjust the initial step bound
            if iter == 1 {
                delta = delta.min(pnorm);
            }

            // evaluate the function at x + p and calculate its norm
            fcn.fcn(&wa2, &mut wa4);
            nfev += 1;
            let fnorm1 = enorm(&wa4);

            // compute the scaled actual reduction
            let mut actred = -1.0;
            if P1 * fnorm1 < fnorm {
                let temp = fnorm1 / fnorm;
                actred = 1.0 - temp * temp;
            }

            // compute the scaled predicted reduction and the scaled
            // directional derivative
            for item in wa3.iter_mut() {
                *item = 0.0;
            }
            for j in 0..n {
                let temp = wa1[ipvt[j]];
                for i in 0..=j {
                    wa3[i] += fjac[i + m * j] * temp;
                }
            }
            let temp1 = enorm(&wa3) / fnorm;
            let temp2 = (par.sqrt() * pnorm) / fnorm;
            let prered = temp1 * temp1 + (temp2 * temp2) / P5;
            let dirder = -(temp1 * temp1 + temp2 * temp2);

            // compute the ratio of the actual to the predicted reduction
            let ratio = if prered != 0.0 { actred / prered } else { 0.0 };

            // update the step bound
            if ratio <= P25 {
                let mut temp = if actred >= 0.0 {
                    P5
                } else {
                    P5 * dirder / (dirder + P5 * actred)
                };
                if P1 * fnorm1 >= fnorm || temp < P1 {
                    temp = P1;
                }
                delta = temp * delta.min(pnorm / P1);
                par /= temp;
            } else if par == 0.0 || ratio >= P75 {
                delta = pnorm / P5;
                par *= P5;
            }

            let successful = ratio >= P0001;
            if successful {
                // successful iteration: update x, fvec, and their norms
                for j in 0..n {
                    x[j] = wa2[j];
                    wa2[j] = diag[j] * x[j];
                }
                fvec.copy_from_slice(&wa4);
                xnorm = enorm(&wa2);
                fnorm = fnorm1;
                iter += 1;
            }

            // tests for convergence
            if actred.abs() <= ftol && prered <= ftol && P5 * ratio <= 1.0 {
                info = 1;
            }
            if delta <= xtol * xnorm {
                info = 2;
            }
            if actred.abs() <= ftol && prered <= ftol && P5 * ratio <= 1.0 && info == 2 {
                info = 3;
            }
            if info != 0 {
                break 'outer;
            }

            // tests for termination and stringent tolerances
            if nfev >= maxfev {
                info = 5;
            }
            if actred.abs() <= MACHEP && prered <= MACHEP && P5 * ratio <= 1.0 {
                info = 6;
            }
            if delta <= MACHEP * xnorm {
                info = 7;
            }
            if gnorm <= MACHEP {
                info = 8;
            }
            if info != 0 {
                break 'outer;
            }

            if successful {
                break;
            }
        }
    }
    info
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

    struct LinearSystem {
        with_jacobian: bool,
    }

    impl LmdifCostFunction for LinearSystem {
        fn fcn(&mut self, x: &[Real], fvec: &mut [Real]) {
            fvec[0] = x[0] + 2.0 * x[1] - 4.0;
            fvec[1] = 3.0 * x[0] - x[1] - 5.0;
        }

        fn has_jacobian(&self) -> bool {
            self.with_jacobian
        }

        fn jacobian(&mut self, _x: &[Real], fjac: &mut [Real]) {
            // column-major 2x2: d f_i / d x_j at fjac[i + 2 j]
            fjac.copy_from_slice(&[1.0, 3.0, 2.0, -1.0]);
        }
    }

    #[test]
    fn fdjac2_approximates_the_jacobian() {
        let (m, n) = (2, 2);
        let mut fcn = LinearSystem {
            with_jacobian: false,
        };
        let mut x = vec![1.0, 2.0];
        let mut fvec = vec![0.0; m];
        let x0 = x.clone();
        fcn.fcn(&x0, &mut fvec);
        let mut fjac = vec![0.0; m * n];
        let mut wa = vec![0.0; m];
        let residuals = fvec.clone();
        fdjac2(m, n, &mut x, &residuals, &mut fjac, 1e-8, &mut wa, &mut fcn);
        assert_eq!(x, x0);
        let exact = [1.0, 3.0, 2.0, -1.0];
        for k in 0..m * n {
            assert!((fjac[k] - exact[k]).abs() < 1e-6, "entry {k}: {}", fjac[k]);
        }
    }

    #[test]
    fn lmpar_returns_gauss_newton_step_inside_trust_region() {
        let n = 2;
        let mut r = [1.0, 0.0, 0.0, 1.0];
        let ipvt = [0_usize, 1];
        let diag = [1.0, 1.0];
        let qtb = [3.0, 4.0];
        let mut par = 0.0;
        let (mut x, mut sdiag, mut wa1, mut wa2) = ([0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]);
        lmpar(
            n, &mut r, 2, &ipvt, &diag, &qtb, 100.0, &mut par, &mut x, &mut sdiag, &mut wa1,
            &mut wa2,
        );
        assert_eq!(par, 0.0);
        assert!((x[0] - 3.0).abs() < TOL);
        assert!((x[1] - 4.0).abs() < TOL);
    }

    #[test]
    fn lmpar_damps_step_to_the_trust_region_boundary() {
        let n = 2;
        let delta = 1.0;
        let mut r = [1.0, 0.0, 0.0, 1.0];
        let ipvt = [0_usize, 1];
        let diag = [1.0, 1.0];
        let qtb = [3.0, 4.0];
        let mut par = 0.0;
        let (mut x, mut sdiag, mut wa1, mut wa2) = ([0.0; 2], [0.0; 2], [0.0; 2], [0.0; 2]);
        lmpar(
            n, &mut r, 2, &ipvt, &diag, &qtb, delta, &mut par, &mut x, &mut sdiag, &mut wa1,
            &mut wa2,
        );
        assert!(par > 0.0);
        assert!((enorm(&x) - delta).abs() <= 0.1 * delta);
    }

    struct LineFit;

    impl LmdifCostFunction for LineFit {
        fn fcn(&mut self, x: &[Real], fvec: &mut [Real]) {
            // residuals of y = a + b*t through (0, 1), (1, 3), (2, 5)
            for (i, fv) in fvec.iter_mut().enumerate() {
                let t = i as Real;
                *fv = x[0] + x[1] * t - (1.0 + 2.0 * t);
            }
        }
    }

    fn run_lmdif(m: usize, n: usize, fcn: &mut dyn LmdifCostFunction) -> (Vec<Real>, i32) {
        let mut x = vec![0.0; n];
        let mut fvec = vec![0.0; m];
        let info = lmdif(
            m, n, &mut x, &mut fvec, 1e-10, 1e-10, 1e-10, 400, 1e-8, 100.0, fcn,
        );
        (x, info)
    }

    #[test]
    fn lmdif_solves_square_linear_system() {
        let (x, info) = run_lmdif(
            2,
            2,
            &mut LinearSystem {
                with_jacobian: false,
            },
        );
        assert!((1..=4).contains(&info), "info = {info}");
        assert!((x[0] - 2.0).abs() < 1e-8, "x0 = {}", x[0]);
        assert!((x[1] - 1.0).abs() < 1e-8, "x1 = {}", x[1]);
    }

    #[test]
    fn lmdif_uses_the_supplied_jacobian() {
        let (x, info) = run_lmdif(
            2,
            2,
            &mut LinearSystem {
                with_jacobian: true,
            },
        );
        assert!((1..=4).contains(&info), "info = {info}");
        assert!((x[0] - 2.0).abs() < 1e-8);
        assert!((x[1] - 1.0).abs() < 1e-8);
    }

    #[test]
    fn lmdif_fits_overdetermined_least_squares() {
        let (x, info) = run_lmdif(3, 2, &mut LineFit);
        assert!((1..=4).contains(&info), "info = {info}");
        assert!((x[0] - 1.0).abs() < 1e-8);
        assert!((x[1] - 2.0).abs() < 1e-8);
    }

    #[test]
    fn lmdif_rejects_improper_input() {
        let mut fcn = LineFit;
        let mut x = vec![0.0; 2];
        let mut fvec = vec![0.0; 3];
        // fewer functions than variables
        assert_eq!(
            lmdif(
                1, 2, &mut x, &mut fvec, 1e-10, 1e-10, 1e-10, 400, 1e-8, 100.0, &mut fcn
            ),
            0
        );
        // negative tolerance
        assert_eq!(
            lmdif(
                3, 2, &mut x, &mut fvec, -1.0, 1e-10, 1e-10, 400, 1e-8, 100.0, &mut fcn
            ),
            0
        );
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
