//! TARGET calendar.
//!
//! Port of `ql/time/calendars/target.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The TARGET calendar (Trans-European Automated Real-time Gross settlement
/// Express Transfer system), used for the euro since 2000.
pub struct Target;

impl Target {
    /// Builds a TARGET calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "TARGET".to_string()
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
            // Good Friday
            || (dd == em - 3 && y >= 2000)
            // Easter Monday
            || (dd == em && y >= 2000)
            // Labour Day
            || (d == 1 && m == Month::May && y >= 2000)
            // Christmas
            || (d == 25 && m == Month::December)
            // Day of Goodwill
            || (d == 26 && m == Month::December && y >= 2000)
            // December 31st, 1998, 1999, and 2001 only
            || (d == 31 && m == Month::December && (y == 1998 || y == 1999 || y == 2001)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against known TARGET holidays and business days. This is not
    // an exhaustive transcription of QuantLib's test-suite/calendars.cpp; it
    // covers the weekend rule, the fixed holidays and the Easter-linked ones.
    #[test]
    fn known_holidays_2018() {
        let c = Target::new();
        for (d, m) in [
            (1, Month::January),   // New Year's Day
            (30, Month::March),    // Good Friday
            (2, Month::April),     // Easter Monday
            (1, Month::May),       // Labour Day
            (25, Month::December), // Christmas
            (26, Month::December), // Day of Goodwill
        ] {
            assert!(c.is_holiday(Date::new(d, m, 2018)), "{d} {m} 2018");
        }
    }

    #[test]
    fn business_days_and_weekends() {
        let c = Target::new();
        assert!(c.is_business_day(Date::new(2, Month::January, 2018)));
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_holiday(Date::new(6, Month::January, 2018))); // Saturday
    }

    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Target::new().name(), "TARGET");
    }
}
