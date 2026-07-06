//! Matrix decompositions ported from `ql/math/matrixutilities/`.

pub mod choleskydecomposition;
pub mod qrdecomposition;
pub mod svd;
pub mod symmetricschurdecomposition;

pub use choleskydecomposition::{cholesky_decomposition, cholesky_solve_for};
pub use qrdecomposition::{qr_decomposition, qr_solve};
pub use svd::Svd;
pub use symmetricschurdecomposition::SymmetricSchurDecomposition;

#[cfg(test)]
pub(crate) mod testsupport;
