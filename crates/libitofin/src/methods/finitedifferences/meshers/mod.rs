//! Finite-difference meshers.
//!
//! Port of `ql/methods/finitedifferences/meshers/`. A mesher dresses the index
//! space of an
//! [`FdmLinearOpLayout`](super::operators::FdmLinearOpLayout) with the
//! coordinates of the grid points and the spacings between them, which is what
//! turns an index-space stencil into a numerical derivative.

mod fdmmesher;
mod uniformgridmesher;

pub use fdmmesher::FdmMesher;
pub use uniformgridmesher::UniformGridMesher;
