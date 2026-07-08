//! Base option definitions.
//!
//! Port of the `Option::Type` subset of `ql/option.hpp`. The `Option`
//! instrument base class, its `arguments` and the `Greeks`/`MoreGreeks`
//! results follow with the instrument and pricing-engine framework.

use std::fmt;

/// Call/put flag of an option.
///
/// The integer discriminants match QuantLib's `Option::Type` enum
/// (`Put = -1`, `Call = 1`), whose sign some pricing formulas rely on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum OptionType {
    /// Right to sell at the strike.
    Put = -1,
    /// Right to buy at the strike.
    Call = 1,
}

impl fmt::Display for OptionType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OptionType::Call => f.write_str("Call"),
            OptionType::Put => f.write_str("Put"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_match_quantlib() {
        assert_eq!(OptionType::Put as i32, -1);
        assert_eq!(OptionType::Call as i32, 1);
    }

    #[test]
    fn display_matches_quantlib_output() {
        assert_eq!(OptionType::Call.to_string(), "Call");
        assert_eq!(OptionType::Put.to_string(), "Put");
    }
}
