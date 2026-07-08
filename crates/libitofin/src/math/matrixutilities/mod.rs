//! Matrix decompositions ported from `ql/math/matrixutilities/`.

pub mod choleskydecomposition;
pub mod pseudosqrt;
pub mod qrdecomposition;
pub mod svd;
pub mod symmetricschurdecomposition;
pub mod tqreigendecomposition;

pub use choleskydecomposition::{cholesky_decomposition, cholesky_solve_for};
pub use pseudosqrt::{SalvagingAlgorithm, pseudo_sqrt, rank_reduced_sqrt};
pub use qrdecomposition::{qr_decomposition, qr_solve};
pub use svd::Svd;
pub use symmetricschurdecomposition::SymmetricSchurDecomposition;
pub use tqreigendecomposition::{EigenVectorCalculation, ShiftStrategy, TqrEigenDecomposition};

#[cfg(test)]
pub(crate) mod testsupport;
