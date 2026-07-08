//! Option payoff contract.
//!
//! Port of `ql/payoff.hpp`: the [`Payoff`] trait is the abstract base class
//! for option payoffs. The concrete payoff hierarchy lives in
//! [`instruments`](crate::instruments).
//!
//! QuantLib's `accept(AcyclicVisitor&)` hook is not ported: Rust callers
//! dispatch on the payoff traits (e.g.
//! [`StrikedTypePayoff`](crate::instruments::StrikedTypePayoff)) instead of
//! visiting by dynamic type.

use crate::types::Real;

/// Abstract base class for option payoffs.
pub trait Payoff {
    /// A name describing the payoff type.
    ///
    /// Used for output and comparison between payoffs; not meant for writing
    /// switch-on-type code.
    fn name(&self) -> String;

    /// A description of the payoff, including its parameters.
    fn description(&self) -> String;

    /// The payoff value at the given underlying price
    /// (`operator()(Real price)` in QuantLib).
    fn value(&self, price: Real) -> Real;
}
