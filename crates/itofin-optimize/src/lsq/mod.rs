//! Dense least-squares kernels for the SLSQP subproblem.
//!
//! Every kernel reads its matrices through [`MatRef`], a row-major `f64` slice
//! with an explicit row stride, and works on private copies, so the caller's
//! data is never modified.
//!
//! - [`Qr`](householder::Qr): Householder QR with column pivoting. Rank detection stops the
//!   factorization at step `k` when the largest remaining column norm is at
//!   most `tau = max(m, n) * f64::EPSILON * |R_11|`, where `|R_11|` is the
//!   largest column norm of the input.
//!
//! Reference: Lawson, C. L. and Hanson, R. J. (1974), Solving Least Squares
//! Problems, Prentice-Hall (SIAM Classics reprint 1995): the Householder
//! construction and application (Algorithms H1 and H2), the pivoted
//! triangularization behind HFTI (Chapter 14) and NNLS (Algorithm 23.10).
//!
//! The allowance for unused items is temporary: OPT-10 (#1088) removes it once
//! the SLSQP driver calls these kernels.
#![cfg_attr(not(test), allow(dead_code))]

pub(crate) mod householder;

#[cfg(test)]
mod tests;

/// A read-only dense matrix stored row-major: entry `(i, j)` sits at
/// `data[i * stride + j]`, with `stride >= cols`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MatRef<'a> {
    pub(crate) data: &'a [f64],
    pub(crate) rows: usize,
    pub(crate) cols: usize,
    pub(crate) stride: usize,
}

impl MatRef<'_> {
    /// The entry in row `i` and column `j`.
    pub(crate) fn at(&self, i: usize, j: usize) -> f64 {
        self.data[i * self.stride + j]
    }
}

/// The Euclidean norm, scaled by the largest magnitude so that squaring
/// neither overflows nor underflows.
fn norm(values: impl Iterator<Item = f64> + Clone) -> f64 {
    let scale = values.clone().fold(0.0_f64, |acc, v| acc.max(v.abs()));
    if scale == 0.0 {
        return 0.0;
    }
    scale * values.map(|v| (v / scale).powi(2)).sum::<f64>().sqrt()
}
