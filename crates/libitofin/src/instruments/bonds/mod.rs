//! Concrete bond instruments.
//!
//! Port of `ql/instruments/bonds/`: the derived bonds built on the
//! [`Bond`](crate::instruments::Bond) base.

mod fixedratebond;

pub use fixedratebond::FixedRateBond;
