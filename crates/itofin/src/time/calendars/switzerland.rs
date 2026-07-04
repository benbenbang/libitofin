//! Swiss calendar.
//!
//! Port of `ql/time/calendars/switzerland.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Swiss calendar.
pub struct Switzerland;

impl Switzerland {
    /// Builds a Swiss calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Switzerland".to_string()
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
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Berchtoldstag
            || (d == 2 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Ascension Day
            || (dd == em + 38)
            // Whit Monday
            || (dd == em + 49)
            // Labour Day
            || (d == 1 && m == Month::May)
            // National Day
            || (d == 1 && m == Month::August)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen's Day
            || (d == 26 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Switzerland::new().name(), "Switzerland");
    }

    #[test]
    fn fixed_holidays() {
        let c = Switzerland::new();
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(2, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Switzerland::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
