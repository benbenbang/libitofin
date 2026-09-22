//! Stochastic processes for specific models.
//!
//! Port of `ql/processes/`: concrete implementations of the
//! [`StochasticProcess1D`](crate::stochasticprocess::StochasticProcess1D)
//! contract. The generalized Black-Scholes process (with its Merton
//! convenience) is the first resident; the sibling conveniences
//! (`BlackScholesProcess`, `BlackProcess`, `GarmanKohlagenProcess`) and the
//! other process conveniences follow as noted on
//! [`GeneralizedBlackScholesProcess`].

mod blackscholesprocess;
pub mod discretization;
mod hestonprocess;
mod ornsteinuhlenbeckprocess;
mod stochasticprocessarray;

pub use blackscholesprocess::{BlackScholesMertonProcess, GeneralizedBlackScholesProcess};
pub use discretization::{
    DiscretizedProcess1D, EulerDiscretization, ProcessDiscretization, ProcessDiscretization1D,
};
pub use hestonprocess::HestonProcess;
pub use ornsteinuhlenbeckprocess::OrnsteinUhlenbeckProcess;
pub use stochasticprocessarray::StochasticProcessArray;
