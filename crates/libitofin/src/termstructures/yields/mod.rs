//! Yield term structures.
//!
//! Port of `ql/termstructures/yield/` (named `yields` because `yield` is a
//! Rust keyword); concrete curves implementing
//! [`YieldTermStructure`](super::yieldtermstructure::YieldTermStructure).

mod flatforward;

pub use flatforward::FlatForward;
