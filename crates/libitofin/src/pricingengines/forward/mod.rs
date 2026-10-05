//! Forward-product pricing engines.

mod replicatingvarianceswapengine;
mod varianceswapstrip;

pub use replicatingvarianceswapengine::ReplicatingVarianceSwapEngine;
pub use varianceswapstrip::MAX_VARIANCE_SWAP_STRIKES;

#[cfg(test)]
pub(crate) mod test_market;
#[cfg(test)]
mod variance_native_cases;
#[cfg(test)]
mod variance_native_tests;
#[cfg(test)]
mod variance_native_weights;
#[cfg(test)]
mod variance_numerical_tests;
#[cfg(test)]
mod variance_strip_tests;
