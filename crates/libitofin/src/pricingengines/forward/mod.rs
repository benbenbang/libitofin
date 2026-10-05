//! Forward-product pricing engines.

mod replicatingvarianceswapengine;
mod varianceswapstrip;

pub use replicatingvarianceswapengine::ReplicatingVarianceSwapEngine;
pub use varianceswapstrip::MAX_VARIANCE_SWAP_STRIKES;
