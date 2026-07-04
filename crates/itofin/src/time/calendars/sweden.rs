//! Swedish calendar.
//!
//! Port of `ql/time/calendars/sweden.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Swedish calendar.
pub struct Sweden;

impl Sweden {
    /// Builds a Swedish calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Sweden".to_string()
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
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Ascension Thursday
            || (dd == em + 38)
            // Whit Monday (till 2004)
            || (dd == em + 49 && y < 2005)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Epiphany
            || (d == 6 && m == Month::January)
            // May Day
            || (d == 1 && m == Month::May)
            // National Day
            // Only a holiday since 2005
            || (d == 6 && m == Month::June && y >= 2005)
            // Midsummer Eve (Friday between June 19-25)
            || (w == Weekday::Friday && (19..=25).contains(&d) && m == Month::June)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas Day
            || (d == 25 && m == Month::December)
            // Boxing Day
            || (d == 26 && m == Month::December)
            // New Year's Eve
            || (d == 31 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Sweden::new().name(), "Sweden");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Sweden::new();
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(6, Month::January, 2019))); // Epiphany
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // May Day
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas Day
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // Boxing Day
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // New Year's Eve
    }

    #[test]
    fn weekend_rule() {
        let c = Sweden::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
