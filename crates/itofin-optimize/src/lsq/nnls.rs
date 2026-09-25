//! Nonnegative least squares, Lawson and Hanson Algorithm 23.10.

use super::householder::Qr;
use super::{Failure, MatRef, norm};

/// A solution of `min ||A x - b||` subject to `x >= 0`.
#[derive(Debug, Clone)]
pub(crate) struct Nnls {
    pub(crate) x: Vec<f64>,
    pub(crate) residual_norm: f64,
    pub(crate) iterations: usize,
}

/// Solves `min ||A x - b||` subject to `x >= 0` by the active-set method.
///
/// Each outer iteration moves into the passive set the index of largest
/// positive dual component `w = A' (b - A x)` and solves the unconstrained
/// problem on the passive columns; the inner loop then steps back towards the
/// feasible region and releases the indices that reach zero.
///
/// The finite-termination safeguard: an entering index whose column is
/// numerically dependent on the passive columns, or whose trial component is
/// not positive, is moved straight back and not re-added in the same outer
/// iteration. In exact arithmetic neither happens; in rounding it would
/// otherwise make the same index enter and leave forever.
///
/// # Errors
///
/// [`Failure::IterationCap`] after `3 n` outer iterations.
pub(crate) fn nnls(a: MatRef<'_>, b: &[f64]) -> Result<Nnls, Failure> {
    let n = a.cols;
    let mut x = vec![0.0; n];
    let mut passive = vec![false; n];
    let mut iterations = 0;
    loop {
        let w = dual(a, b, &x);
        let mut rejected = vec![false; n];
        let mut z = loop {
            let Some(t) = (0..n)
                .filter(|&j| !passive[j] && !rejected[j] && w[j] > 0.0)
                .max_by(|&i, &j| w[i].total_cmp(&w[j]))
            else {
                let residual_norm = norm(residual(a, b, &x).into_iter());
                return Ok(Nnls {
                    x,
                    residual_norm,
                    iterations,
                });
            };
            passive[t] = true;
            let (z, full_rank) = solve_passive(a, b, &passive);
            if full_rank && z[t] > 0.0 {
                break z;
            }
            passive[t] = false;
            rejected[t] = true;
        };
        iterations += 1;
        if iterations > 3 * n {
            return Err(Failure::IterationCap);
        }
        loop {
            let blocking = (0..n)
                .filter(|&j| passive[j] && z[j] <= 0.0)
                .map(|j| {
                    let gap = x[j] - z[j];
                    (j, if gap > 0.0 { x[j] / gap } else { 0.0 })
                })
                .min_by(|p, q| p.1.total_cmp(&q.1));
            let Some((q, alpha)) = blocking else {
                x = z;
                break;
            };
            for j in 0..n {
                if !passive[j] {
                    continue;
                }
                x[j] += alpha * (z[j] - x[j]);
                if j == q || x[j] <= 0.0 {
                    x[j] = 0.0;
                    passive[j] = false;
                }
            }
            z = solve_passive(a, b, &passive).0;
        }
    }
}

/// The residual `b - A x`.
fn residual(a: MatRef<'_>, b: &[f64], x: &[f64]) -> Vec<f64> {
    (0..a.rows)
        .map(|i| b[i] - (0..a.cols).map(|j| a.at(i, j) * x[j]).sum::<f64>())
        .collect()
}

/// The dual vector `w = A' (b - A x)`.
fn dual(a: MatRef<'_>, b: &[f64], x: &[f64]) -> Vec<f64> {
    let r = residual(a, b, x);
    (0..a.cols)
        .map(|j| (0..a.rows).map(|i| a.at(i, j) * r[i]).sum())
        .collect()
}

/// The least-squares solution on the passive columns, zero elsewhere, and
/// whether those columns have full numerical rank.
fn solve_passive(a: MatRef<'_>, b: &[f64], passive: &[bool]) -> (Vec<f64>, bool) {
    let columns: Vec<usize> = (0..a.cols).filter(|&j| passive[j]).collect();
    let data: Vec<f64> = (0..a.rows)
        .flat_map(|i| columns.iter().map(move |&j| a.at(i, j)))
        .collect();
    let qr = Qr::factor(MatRef {
        data: &data,
        rows: a.rows,
        cols: columns.len(),
        stride: columns.len(),
    });
    let (y, _) = qr.solve(b);
    let mut z = vec![0.0; a.cols];
    for (k, &j) in columns.iter().enumerate() {
        z[j] = y[k];
    }
    (z, qr.rank() == columns.len())
}
