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
//! - [`nnls`](nnls::nnls): nonnegative least squares by the active-set method.
//! - [`ldp`](lsei::ldp), [`lsi`](lsei::lsi) and [`lsei`](lsei::lsei): least
//!   distance, inequality-constrained and equality-and-inequality-constrained
//!   least squares, each reduced to the one before it.
//!
//! Reference: Lawson, C. L. and Hanson, R. J. (1974), Solving Least Squares
//! Problems, Prentice-Hall (SIAM Classics reprint 1995): the Householder
//! construction and application (Algorithms H1 and H2), the pivoted
//! triangularization behind HFTI (Chapter 14), NNLS (Algorithm 23.10), LDP
//! (23.27) and the LSI and LSEI reductions (Chapter 23).

pub(crate) mod householder;
pub(crate) mod lsei;
pub(crate) mod nnls;

#[cfg(test)]
mod tests;

/// Why a kernel returned no solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum Failure {
    /// The active-set loop reached its iteration cap.
    #[error("the active-set iteration cap was reached")]
    IterationCap,
    /// The inequality constraints admit no point.
    #[error("the inequality constraints are infeasible")]
    Infeasible,
    /// A matrix that must have full rank does not.
    #[error("a matrix that must have full rank is rank deficient")]
    RankDeficient,
}

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
