//! Matrix decompositions ported from `ql/math/matrixutilities/`.

pub mod svd;
pub mod symmetricschurdecomposition;

pub use svd::Svd;
pub use symmetricschurdecomposition::SymmetricSchurDecomposition;

#[cfg(test)]
pub(crate) mod testsupport;
