//! Day-of-week enumeration.
//!
//! Port of `ql/time/weekday.hpp`. QuantLib defines `Weekday` as a day's serial
//! number modulo 7, numbered `Sunday = 1 .. Saturday = 7` (the same convention
//! as Excel's `WEEKDAY` function except that Excel maps Sunday to 7). The C++
//! `Sun`/`Mon`/... short aliases are dropped: Rust enum variants cannot share a
//! discriminant as separate names, and the long forms read clearly enough.

use std::fmt;

use crate::types::Integer;

/// A day of the week, numbered `Sunday = 1 .. Saturday = 7`.
///
/// The numeric values match a date's serial number modulo 7 (with 0 mapped to
/// Saturday), so the derived [`Ord`] agrees with QuantLib's integer comparison
/// of weekdays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(i32)]
pub enum Weekday {
    /// Sunday (1).
    Sunday = 1,
    /// Monday (2).
    Monday = 2,
    /// Tuesday (3).
    Tuesday = 3,
    /// Wednesday (4).
    Wednesday = 4,
    /// Thursday (5).
    Thursday = 5,
    /// Friday (6).
    Friday = 6,
    /// Saturday (7).
    Saturday = 7,
}

impl Weekday {
    /// Builds a weekday from its 1-based ordinal (`Sunday = 1 .. Saturday = 7`).
    ///
    /// # Panics
    ///
    /// Panics if `n` is outside `1..=7`; a weekday ordinal is derived from a
    /// serial number and is always in range, so an out-of-range value signals a
    /// bug in the caller.
    pub fn from_ordinal(n: Integer) -> Weekday {
        match n {
            1 => Weekday::Sunday,
            2 => Weekday::Monday,
            3 => Weekday::Tuesday,
            4 => Weekday::Wednesday,
            5 => Weekday::Thursday,
            6 => Weekday::Friday,
            7 => Weekday::Saturday,
            _ => panic!("weekday ordinal {n} outside range [1,7]"),
        }
    }

    /// The 1-based ordinal of the weekday (`Sunday = 1 .. Saturday = 7`).
    pub fn ordinal(self) -> Integer {
        self as Integer
    }
}

impl fmt::Display for Weekday {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Weekday::Sunday => "Sunday",
            Weekday::Monday => "Monday",
            Weekday::Tuesday => "Tuesday",
            Weekday::Wednesday => "Wednesday",
            Weekday::Thursday => "Thursday",
            Weekday::Friday => "Friday",
            Weekday::Saturday => "Saturday",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinal_round_trips() {
        for n in 1..=7 {
            assert_eq!(Weekday::from_ordinal(n).ordinal(), n);
        }
    }

    #[test]
    fn ordering_matches_serial_numbering() {
        assert!(Weekday::Sunday < Weekday::Monday);
        assert!(Weekday::Friday < Weekday::Saturday);
    }

    #[test]
    #[should_panic(expected = "outside range")]
    fn from_ordinal_rejects_out_of_range() {
        let _ = Weekday::from_ordinal(0);
    }

    #[test]
    fn display_is_long_form() {
        assert_eq!(Weekday::Wednesday.to_string(), "Wednesday");
    }
}
