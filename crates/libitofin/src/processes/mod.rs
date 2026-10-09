//! Stochastic processes for specific models.
//!
//! Port of `ql/processes/`: concrete implementations of the
//! [`StochasticProcess1D`](crate::stochasticprocess::StochasticProcess1D)
//! contract. The generalized Black-Scholes process (with its Merton
//! convenience) is the first resident; the sibling conveniences
//! (`BlackScholesProcess`, `BlackProcess`, `GarmanKohlagenProcess`) and the
//! other process conveniences follow as noted on
//! [`GeneralizedBlackScholesProcess`].

mod batesprocess;
mod blackscholesprocess;
pub mod discretization;
pub mod forwardmeasureprocess;
mod geometricbrownianprocess;
mod gjrgarchprocess;
mod hestonprocess;
mod hullwhiteprocess;
mod merton76process;
mod ornsteinuhlenbeckprocess;
mod stochasticprocessarray;

pub use batesprocess::BatesProcess;
pub use blackscholesprocess::{BlackScholesMertonProcess, GeneralizedBlackScholesProcess};
pub use discretization::{
    DiscretizedProcess, DiscretizedProcess1D, EulerDiscretization, ProcessDiscretization,
    ProcessDiscretization1D,
};
pub use forwardmeasureprocess::{ForwardMeasureProcess1D, ForwardMeasureTime};
pub use geometricbrownianprocess::GeometricBrownianMotionProcess;
pub use gjrgarchprocess::{
    GjrGarchCoefficients, GjrGarchDiscretization, GjrGarchParameters, GjrGarchProcess,
};
pub use hestonprocess::{Discretization as HestonDiscretization, HestonProcess};
pub use hullwhiteprocess::HullWhiteForwardProcess;
pub use merton76process::Merton76Process;
pub use ornsteinuhlenbeckprocess::OrnsteinUhlenbeckProcess;
pub use stochasticprocessarray::StochasticProcessArray;
