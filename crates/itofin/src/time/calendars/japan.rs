//! Japanese calendar.
//!
//! Port of `ql/time/calendars/japan.{hpp,cpp}`.
//!
//! The `>= .. && <= ..` day-range checks are kept verbatim from the C++ source
//! rather than rewritten as `RangeInclusive::contains`, so the corresponding
//! clippy lint is allowed module-wide.
#![allow(clippy::manual_range_contains)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// The Japanese calendar.
pub struct Japan;

impl Japan {
    /// Builds a Japanese calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Japan".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        // equinox calculation
        let exact_vernal_equinox_time = 20.69115;
        let exact_autumnal_equinox_time = 23.09;
        let diff_per_year = 0.242194;
        let moving_amount = (y - 2000) as f64 * diff_per_year;
        let number_of_leap_years = (y - 2000) / 4 + (y - 2000) / 100 - (y - 2000) / 400;
        // vernal equinox day
        let ve = (exact_vernal_equinox_time + moving_amount - number_of_leap_years as f64) as i32;
        // autumnal equinox day
        let ae = (exact_autumnal_equinox_time + moving_amount - number_of_leap_years as f64) as i32;
        // checks
        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Bank Holiday
            || (d == 2 && m == Month::January)
            // Bank Holiday
            || (d == 3 && m == Month::January)
            // Coming of Age Day (2nd Monday in January),
            // was January 15th until 2000
            || (w == Weekday::Monday && (d >= 8 && d <= 14) && m == Month::January
                && y >= 2000)
            || ((d == 15 || (d == 16 && w == Weekday::Monday)) && m == Month::January
                && y < 2000)
            // National Foundation Day
            || ((d == 11 || (d == 12 && w == Weekday::Monday)) && m == Month::February)
            // Emperor's Birthday (Emperor Naruhito)
            || ((d == 23 || (d == 24 && w == Weekday::Monday)) && m == Month::February
                && y >= 2020)
            // Emperor's Birthday (Emperor Akihito)
            || ((d == 23 || (d == 24 && w == Weekday::Monday)) && m == Month::December
                && (y >= 1989 && y < 2019))
            // Vernal Equinox
            || ((d == ve || (d == ve + 1 && w == Weekday::Monday)) && m == Month::March)
            // Greenery Day
            || ((d == 29 || (d == 30 && w == Weekday::Monday)) && m == Month::April)
            // Constitution Memorial Day
            || (d == 3 && m == Month::May)
            // Holiday for a Nation
            || (d == 4 && m == Month::May)
            // Children's Day
            || (d == 5 && m == Month::May)
            // any of the three above observed later if on Saturday or Sunday
            || (d == 6 && m == Month::May
                && (w == Weekday::Monday || w == Weekday::Tuesday || w == Weekday::Wednesday))
            // Marine Day (3rd Monday in July),
            // was July 20th until 2003, not a holiday before 1996,
            // July 23rd in 2020 due to Olympics games
            // July 22nd in 2021 due to Olympics games
            || (w == Weekday::Monday && (d >= 15 && d <= 21) && m == Month::July
                && ((y >= 2003 && y < 2020) || y >= 2022))
            || ((d == 20 || (d == 21 && w == Weekday::Monday)) && m == Month::July
                && y >= 1996 && y < 2003)
            || (d == 23 && m == Month::July && y == 2020)
            || (d == 22 && m == Month::July && y == 2021)
            // Mountain Day
            // (moved in 2020 due to Olympics games)
            // (moved in 2021 due to Olympics games)
            || ((d == 11 || (d == 12 && w == Weekday::Monday)) && m == Month::August
                && ((y >= 2016 && y < 2020) || y >= 2022))
            || (d == 10 && m == Month::August && y == 2020)
            || (d == 9 && m == Month::August && y == 2021)
            // Respect for the Aged Day (3rd Monday in September),
            // was September 15th until 2003
            || (w == Weekday::Monday && (d >= 15 && d <= 21) && m == Month::September
                && y >= 2003)
            || ((d == 15 || (d == 16 && w == Weekday::Monday)) && m == Month::September
                && y < 2003)
            // If a single day falls between Respect for the Aged Day
            // and the Autumnal Equinox, it is holiday
            || (w == Weekday::Tuesday && d + 1 == ae && d >= 16 && d <= 22
                && m == Month::September && y >= 2003)
            // Autumnal Equinox
            || ((d == ae || (d == ae + 1 && w == Weekday::Monday)) && m == Month::September)
            // Health and Sports Day (2nd Monday in October),
            // was October 10th until 2000,
            // July 24th in 2020 due to Olympics games
            // July 23rd in 2021 due to Olympics games
            || (w == Weekday::Monday && (d >= 8 && d <= 14) && m == Month::October
                && ((y >= 2000 && y < 2020) || y >= 2022))
            || ((d == 10 || (d == 11 && w == Weekday::Monday)) && m == Month::October
                && y < 2000)
            || (d == 24 && m == Month::July && y == 2020)
            || (d == 23 && m == Month::July && y == 2021)
            // National Culture Day
            || ((d == 3 || (d == 4 && w == Weekday::Monday)) && m == Month::November)
            // Labor Thanksgiving Day
            || ((d == 23 || (d == 24 && w == Weekday::Monday)) && m == Month::November)
            // Bank Holiday
            || (d == 31 && m == Month::December)
            // one-shot holidays
            // Marriage of Prince Akihito
            || (d == 10 && m == Month::April && y == 1959)
            // Rites of Imperial Funeral
            || (d == 24 && m == Month::February && y == 1989)
            // Enthronement Ceremony (Emperor Akihito)
            || (d == 12 && m == Month::November && y == 1990)
            // Marriage of Prince Naruhito
            || (d == 9 && m == Month::June && y == 1993)
            // Special holiday based on Japanese public holidays law
            || (d == 30 && m == Month::April && y == 2019)
            // Enthronement Day (Emperor Naruhito)
            || (d == 1 && m == Month::May && y == 2019)
            // Special holiday based on Japanese public holidays law
            || (d == 2 && m == Month::May && y == 2019)
            // Enthronement Ceremony (Emperor Naruhito)
            || (d == 22 && m == Month::October && y == 2019))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against known Japanese holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Japan::new().name(), "Japan");
    }

    #[test]
    fn fixed_holidays() {
        let c = Japan::new();
        // New Year's Day and bank holidays (unconditional every year)
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(2, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(3, Month::January, 2019)));
        // Constitution Memorial Day / Holiday for a Nation / Children's Day
        assert!(c.is_holiday(Date::new(3, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(4, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(5, Month::May, 2019)));
        // Bank Holiday, December 31st
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Japan::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
