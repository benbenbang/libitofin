//! Danish calendar.
//!
//! Port of `ql/time/calendars/denmark.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Danish calendar.
pub struct Denmark;

impl Denmark {
    /// Builds a Danish calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Denmark".to_string()
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
            // Maunday Thursday
            || (dd == em - 4)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // General Prayer Day
            || (dd == em + 25 && y <= 2023)
            // Ascension
            || (dd == em + 38)
            // Day after Ascension
            || (dd == em + 39 && y >= 2009)
            // Whit Monday
            || (dd == em + 49)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Constitution Day, June 5th
            || (d == 5 && m == Month::June)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas
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
        assert_eq!(Denmark::new().name(), "Denmark");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Denmark::new();
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(5, Month::June, 2019))); // Constitution Day
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // Boxing Day
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // New Year's Eve
    }

    #[test]
    fn weekend_rule() {
        let c = Denmark::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
