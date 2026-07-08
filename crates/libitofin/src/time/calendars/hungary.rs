//! Hungarian calendar.
//!
//! Port of `ql/time/calendars/hungary.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Hungarian calendar.
pub struct Hungary;

impl Hungary {
    /// Builds a Hungarian calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Hungary".to_string()
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
            // Good Friday (since 2017)
            || (dd == em - 3 && y >= 2017)
            // Easter Monday
            || (dd == em)
            // Whit Monday
            || (dd == em + 49)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // National Day
            || (d == 15 && m == Month::March)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Constitution Day
            || (d == 20 && m == Month::August)
            // Republic Day
            || (d == 23 && m == Month::October)
            // All Saints Day
            || (d == 1 && m == Month::November)
            // Christmas
            || (d == 25 && m == Month::December)
            // 2nd Day of Christmas
            || (d == 26 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against fixed Hungarian holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Hungary::new().name(), "Hungary");
    }

    #[test]
    fn fixed_holidays() {
        let c = Hungary::new();
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::March, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(20, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(23, Month::October, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Hungary::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
