//! Duration conventions.
//!
//! Port of `ql/cashflows/duration.hpp`, whose `Duration` struct exists only to
//! scope the `Type` enum. The port drops the wrapper and keeps the enum.

use std::fmt;

/// The duration convention [`CashFlows::duration`](super::CashFlows::duration)
/// computes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Duration {
    /// `sum(t_i c_i B(t_i)) / sum(c_i B(t_i))`, the discounted-time average.
    Simple,
    /// `(1 + y / N)` times the [`Modified`](Self::Modified) duration, defined
    /// only for a compounded yield.
    Macaulay,
    /// `-(1 / P) dP/dy`.
    Modified,
}

impl fmt::Display for Duration {
    /// Renders the QuantLib label of `duration.cpp`'s `operator<<`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Duration::Simple => "Simple",
            Duration::Macaulay => "Macaulay",
            Duration::Modified => "Modified",
        };
        f.write_str(name)
    }
}
