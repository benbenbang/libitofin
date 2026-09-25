//! Least distance programming and the constrained least-squares problems
//! reduced to it.

use super::householder::Qr;
use super::nnls::nnls;
use super::{Failure, MatRef, norm};

/// The smallest `1 - h' u` the LDP dual may leave before the inequalities are
/// declared infeasible. At a solution `1 - h' u = 1 / (1 + ||x||^2)`, so this
/// admits minimum-norm points up to `||x||` of about `2e6`.
const INFEASIBILITY: f64 = 1e3 * f64::EPSILON;

/// A constrained least-squares solution and its residual norm `||E x - f||`.
#[derive(Debug, Clone)]
pub(crate) struct Lsq {
    pub(crate) x: Vec<f64>,
    pub(crate) residual_norm: f64,
}

/// Solves LDP, `min ||x||` subject to `G x >= h`, through NNLS on its dual
/// (Lawson and Hanson 23.27): with `E = [G'; h']` and `f = (0, ..., 0, 1)`,
/// the NNLS residual `r = E u - f` gives `x = r_{1..n} / (-r_{n+1})`.
///
/// # Errors
///
/// [`Failure::Infeasible`] when `-r_{n+1} = 1 - h' u` is at most
/// [`INFEASIBILITY`], and [`Failure::IterationCap`] from NNLS.
pub(crate) fn ldp(g: MatRef<'_>, h: &[f64]) -> Result<Vec<f64>, Failure> {
    let (m, n) = (g.rows, g.cols);
    let mut e = vec![0.0; (n + 1) * m];
    for i in 0..m {
        for j in 0..n {
            e[j * m + i] = g.at(i, j);
        }
        e[n * m + i] = h[i];
    }
    let mut f = vec![0.0; n + 1];
    f[n] = 1.0;
    let dual = MatRef {
        data: &e,
        rows: n + 1,
        cols: m,
        stride: m,
    };
    let u = nnls(dual, &f)?.x;
    let r: Vec<f64> = (0..=n)
        .map(|j| (0..m).map(|i| dual.at(j, i) * u[i]).sum::<f64>() - f[j])
        .collect();
    let denominator = -r[n];
    if denominator <= INFEASIBILITY {
        return Err(Failure::Infeasible);
    }
    Ok(r[..n].iter().map(|r| r / denominator).collect())
}

/// Solves LSI, `min ||E x - f||` subject to `G x >= h`, for `E` of full column
/// rank. With `E P = Q R` and `Q' f = (f_1, f_2)`, the substitution
/// `u = R P' x - f_1` turns it into LDP on `G P R^-1 u >= h - G P R^-1 f_1`.
///
/// # Errors
///
/// [`Failure::RankDeficient`] when `E` has numerical rank below its column
/// count, and the failures of [`ldp`].
pub(crate) fn lsi(e: MatRef<'_>, f: &[f64], g: MatRef<'_>, h: &[f64]) -> Result<Lsq, Failure> {
    let n = e.cols;
    let qr = Qr::factor(e);
    if qr.rank() < n {
        return Err(Failure::RankDeficient);
    }
    let mut qf = f.to_vec();
    qr.apply_qt(&mut qf);
    let mut reduced = Vec::with_capacity(g.rows * n);
    let mut bound = Vec::with_capacity(g.rows);
    for (i, h) in h.iter().enumerate() {
        let mut row: Vec<f64> = qr.perm().iter().map(|&j| g.at(i, j)).collect();
        qr.solve_rt(&mut row);
        bound.push(h - dot(&row, &qf[..n]));
        reduced.extend_from_slice(&row);
    }
    let reduced = MatRef {
        data: &reduced,
        rows: g.rows,
        cols: n,
        stride: n,
    };
    let u = ldp(reduced, &bound)?;
    let mut z: Vec<f64> = u.iter().zip(&qf).map(|(u, f)| u + f).collect();
    qr.solve_r(&mut z);
    let mut x = vec![0.0; n];
    for (k, &j) in qr.perm().iter().enumerate() {
        x[j] = z[k];
    }
    let residual_norm = norm(u.iter().copied()).hypot(norm(qf[n..].iter().copied()));
    Ok(Lsq { x, residual_norm })
}

/// Solves LSEI, `min ||E x - f||` subject to `C x = d` and `G x >= h`. With
/// `C' P = Q R`, the substitution `x = Q (y_1, y_2)` fixes `y_1` by
/// `R' y_1 = P' d` and leaves LSI in `y_2` on the rows of `E Q` and `G Q`.
///
/// # Errors
///
/// [`Failure::RankDeficient`] when `C` has numerical rank below its row count,
/// even if `C x = d` is consistent, and the failures of [`lsi`].
pub(crate) fn lsei(
    c: MatRef<'_>,
    d: &[f64],
    e: MatRef<'_>,
    f: &[f64],
    g: MatRef<'_>,
    h: &[f64],
) -> Result<Lsq, Failure> {
    let (mc, n) = (c.rows, c.cols);
    let ct: Vec<f64> = (0..n)
        .flat_map(|j| (0..mc).map(move |i| c.at(i, j)))
        .collect();
    let qr = Qr::factor(MatRef {
        data: &ct,
        rows: n,
        cols: mc,
        stride: mc,
    });
    if qr.rank() < mc {
        return Err(Failure::RankDeficient);
    }
    let mut y: Vec<f64> = qr.perm().iter().map(|&i| d[i]).collect();
    qr.solve_rt(&mut y);
    let reduce = |a: MatRef<'_>, rhs: &[f64]| {
        let mut tail = Vec::with_capacity(a.rows * (n - mc));
        let mut shifted = Vec::with_capacity(a.rows);
        for (i, rhs) in rhs.iter().enumerate() {
            let mut row: Vec<f64> = (0..n).map(|j| a.at(i, j)).collect();
            qr.apply_qt(&mut row);
            shifted.push(rhs - dot(&row[..mc], &y));
            tail.extend_from_slice(&row[mc..]);
        }
        (tail, shifted)
    };
    let (e_tail, f_shifted) = reduce(e, f);
    let (g_tail, h_shifted) = reduce(g, h);
    let tail = |data, rows| MatRef {
        data,
        rows,
        cols: n - mc,
        stride: n - mc,
    };
    let reduced = lsi(
        tail(&e_tail, e.rows),
        &f_shifted,
        tail(&g_tail, g.rows),
        &h_shifted,
    )?;
    y.extend_from_slice(&reduced.x);
    qr.apply_q(&mut y);
    Ok(Lsq {
        x: y,
        residual_norm: reduced.residual_norm,
    })
}

fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
