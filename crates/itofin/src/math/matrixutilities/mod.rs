//! Matrix decompositions ported from `ql/math/matrixutilities/`.

pub mod symmetricschurdecomposition;

pub use symmetricschurdecomposition::SymmetricSchurDecomposition;

#[cfg(test)]
pub(crate) mod testsupport;
