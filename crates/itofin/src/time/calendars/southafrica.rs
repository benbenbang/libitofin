//! South-African calendar.
//!
//! Port of `ql/time/calendars/southafrica.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The South-African calendar.
pub struct SouthAfrica;

impl SouthAfrica {
    /// Builds a South-African calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "South Africa".to_string()
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
            // New Year's Day (possibly moved to Monday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Family Day
            || (dd == em)
            // Human Rights Day, March 21st (possibly moved to Monday)
            || ((d == 21 || (d == 22 && w == Weekday::Monday)) && m == Month::March)
            // Freedom Day, April 27th (possibly moved to Monday)
            || ((d == 27 || (d == 28 && w == Weekday::Monday)) && m == Month::April)
            // Election Day, April 14th 2004
            || (d == 14 && m == Month::April && y == 2004)
            // Workers Day, May 1st (possibly moved to Monday)
            || ((d == 1 || (d == 2 && w == Weekday::Monday)) && m == Month::May)
            // Youth Day, June 16th (possibly moved to Monday)
            || ((d == 16 || (d == 17 && w == Weekday::Monday)) && m == Month::June)
            // National Women's Day, August 9th (possibly moved to Monday)
            || ((d == 9 || (d == 10 && w == Weekday::Monday)) && m == Month::August)
            // Heritage Day, September 24th (possibly moved to Monday)
            || ((d == 24 || (d == 25 && w == Weekday::Monday)) && m == Month::September)
            // Day of Reconciliation, December 16th
            // (possibly moved to Monday)
            || ((d == 16 || (d == 17 && w == Weekday::Monday)) && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Day of Goodwill (possibly moved to Monday)
            || ((d == 26 || (d == 27 && w == Weekday::Monday)) && m == Month::December)
            // one-shot: Election day 2009
            || (d == 22 && m == Month::April && y == 2009)
            // one-shot: Election day 2016
            || (d == 3 && m == Month::August && y == 2016)
            // one-shot: Election day 2021
            || (d == 1 && m == Month::November && y == 2021)
            // one-shot: In lieu of Christmas falling on Sunday in 2022
            || (d == 27 && m == Month::December && y == 2022)
            // one-shot: Special holiday to celebrate winning of Rugby World Cup 2023
            || (d == 15 && m == Month::December && y == 2023)
            // one-shot: Election day 2024
            || (d == 29 && m == Month::May && y == 2024))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(SouthAfrica::new().name(), "South Africa");
    }

    #[test]
    fn unconditional_holidays() {
        let c = SouthAfrica::new();
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
    }

    #[test]
    fn weekend_rule() {
        let c = SouthAfrica::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
