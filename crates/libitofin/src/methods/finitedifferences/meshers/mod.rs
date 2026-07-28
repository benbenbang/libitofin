//! Finite-difference meshers.
//!
//! Port of `ql/methods/finitedifferences/meshers/`. A mesher dresses the index
//! space of an
//! [`FdmLinearOpLayout`](super::operators::FdmLinearOpLayout) with the
//! coordinates of the grid points and the spacings between them, which is what
//! turns an index-space stencil into a numerical derivative.
//!
mod concentrating1dmesher;
mod fdm1dmesher;
mod fdmmesher;
mod fdmmeshercomposite;
mod uniform1dmesher;
mod uniformgridmesher;

pub use concentrating1dmesher::concentrating_1d_mesher;
pub use fdm1dmesher::Fdm1dMesher;
pub use fdmmesher::FdmMesher;
pub use fdmmeshercomposite::FdmMesherComposite;
pub use uniform1dmesher::uniform_1d_mesher;
pub use uniformgridmesher::UniformGridMesher;
