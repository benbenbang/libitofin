//! Short or long position.
//!
//! Port of `ql/position.{hpp,cpp}`. The C++ `Position` struct exists only to
//! scope the `Type` enum; the port flattens it to a single [`Position`] enum,
//! with the stream operator carried as [`Display`](std::fmt::Display).

use std::fmt;

/// Long/short flag of a position (`Position::Type`, `ql/position.hpp:32`).
///
/// Distinct from [`SwapType`](crate::instruments::SwapType): a position is the
/// side taken in a contract (an FRA purchase or sale), not a leg direction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Position {
    /// A purchase (a future long loan, short deposit).
    Long,
    /// A sale (a future short loan, long deposit).
    Short,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Position::Long => f.write_str("Long"),
            Position::Short => f.write_str("Short"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_matches_quantlib_output() {
        assert_eq!(Position::Long.to_string(), "Long");
        assert_eq!(Position::Short.to_string(), "Short");
    }
}
