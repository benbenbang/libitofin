//! Output formatters.
//!
//! Port of the low-cost subset of `ql/utilities/dataformatters.hpp`. QuantLib's
//! `io::` manipulators are tied to C++ `std::ostream`; the genuinely useful,
//! cheap ones are ported here as functions returning `String`. The container
//! `sequence` / `power_of_two` manipulators are deferred (low value to the
//! core).

use crate::types::{Rate, Real, Size, Volatility};
use crate::utilities::null::Null;

/// Formats a value, emitting `"null"` for the type's [`Null`] sentinel.
pub fn check_null<T: Null + std::fmt::Display>(value: T) -> String {
    if value.is_null() {
        "null".to_string()
    } else {
        value.to_string()
    }
}

/// Formats a count as an English ordinal: `1st`, `2nd`, `3rd`, `4th`, ...
pub fn ordinal(n: Size) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Formats a real as a percentage, e.g. `0.05` → `"5.000000 %"`.
pub fn percent(value: Real) -> String {
    format!("{} %", value * 100.0)
}

/// Formats a rate as a percentage.
pub fn rate(r: Rate) -> String {
    percent(r)
}

/// Formats a volatility as a percentage.
pub fn volatility(v: Volatility) -> String {
    percent(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Integer;

    #[test]
    fn check_null_emits_null_for_sentinel() {
        assert_eq!(check_null(Integer::MAX), "null");
        assert_eq!(check_null(7_i32), "7");
    }

    #[test]
    fn ordinals() {
        assert_eq!(ordinal(1), "1st");
        assert_eq!(ordinal(2), "2nd");
        assert_eq!(ordinal(3), "3rd");
        assert_eq!(ordinal(4), "4th");
        assert_eq!(ordinal(11), "11th");
        assert_eq!(ordinal(12), "12th");
        assert_eq!(ordinal(13), "13th");
        assert_eq!(ordinal(21), "21st");
        assert_eq!(ordinal(112), "112th");
    }

    #[test]
    fn percentages() {
        assert_eq!(percent(0.05), "5 %");
        assert_eq!(rate(0.05), "5 %");
        assert_eq!(volatility(0.2), "20 %");
    }
}
