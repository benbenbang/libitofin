//! Finite-difference meshers.
//!
//! Port of `ql/methods/finitedifferences/meshers/`. A mesher dresses the index
//! space of an
//! [`FdmLinearOpLayout`](super::operators::FdmLinearOpLayout) with the
//! coordinates of the grid points and the spacings between them, which is what
//! turns an index-space stencil into a numerical derivative.
//!
//! `FdmMesherComposite` is deferred to the rest of batch 2 of the FDM
//! sub-umbrella (#635), together with the `testFdmMesherIntegral` oracle
//! (`fdmlinearop.cpp:1443`) that exercises it against
//! [`concentrating_1d_mesher`].

mod concentrating1dmesher;
mod fdm1dmesher;
mod fdmmesher;
mod uniform1dmesher;
mod uniformgridmesher;

pub use concentrating1dmesher::concentrating_1d_mesher;
pub use fdm1dmesher::Fdm1dMesher;
pub use fdmmesher::FdmMesher;
pub use uniform1dmesher::uniform_1d_mesher;
pub use uniformgridmesher::UniformGridMesher;
