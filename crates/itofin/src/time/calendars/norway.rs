//! Norwegian calendar.
//!
//! Port of `ql/time/calendars/norway.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Norwegian calendar.
pub struct Norway;

impl Norway {
    /// Builds a Norwegian calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Norway".to_string()
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
            // Holy Thursday
            || (dd == em - 4)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Ascension Thursday
            || (dd == em + 38)
            // Whit Monday
            || (dd == em + 49)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // May Day
            || (d == 1 && m == Month::May)
            // National Independence Day
            || (d == 17 && m == Month::May)
            // Christmas Eve
            || (d == 24 && m == Month::December && y >= 2002)
            // Christmas
            || (d == 25 && m == Month::December)
            // Boxing Day
            || (d == 26 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Norway::new().name(), "Norway");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Norway::new();
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // May Day
        assert!(c.is_holiday(Date::new(17, Month::May, 2019))); // National Independence Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // Boxing Day
    }

    #[test]
    fn weekend_rule() {
        let c = Norway::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
