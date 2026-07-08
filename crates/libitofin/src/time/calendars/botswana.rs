//! Botswana calendar.
//!
//! Port of `ql/time/calendars/botswana.{hpp,cpp}`.

// Range clauses are kept in the verbatim C++ `d >= a && d <= b` form.
#![allow(clippy::manual_range_contains)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Botswana calendar.
pub struct Botswana;

impl Botswana {
    /// Builds a Botswana calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Botswana".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let dd = date.day_of_year();
        let m = date.month();
        let y = date.year();
        let em = western_easter_monday(y);
        !(is_weekend_sat_sun(w)
            // New Year's Day (possibly moved to Monday or Tuesday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday) || (d == 3 && w == Weekday::Tuesday))
                && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Labour Day, May 1st (possibly moved to Monday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::May)
            // Ascension
            || (dd == em + 38)
            // Sir Seretse Khama Day, July 1st (possibly moved to Monday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::July)
            // Presidents' Day (third Monday of July)
            || ((d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::July)
            // Independence Day, September 30th (possibly moved to Monday)
            || ((d == 30 && m == Month::September)
                || (d == 1 && w == Weekday::Monday && m == Month::October))
            // Botswana Day, October 1st (possibly moved to Monday or Tuesday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday) || (d == 3 && w == Weekday::Tuesday))
                && m == Month::October)
            // Christmas
            || (d == 25 && m == Month::December)
            // Boxing Day (possibly moved to Monday)
            || ((d == 26 || (d == 27 && w == Weekday::Monday)) && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Botswana::new().name(), "Botswana");
    }

    #[test]
    fn unconditional_holidays() {
        let c = Botswana::new();
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
    }

    #[test]
    fn weekend_rule() {
        let c = Botswana::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
