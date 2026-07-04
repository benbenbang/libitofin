//! Turkish calendar.
//!
//! Port of `ql/time/calendars/turkey.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Turkey's Islamic holidays are tabulated (matching
/// QuantLib's data). Queries beyond this year cannot be answered reliably and
/// panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2034;

/// The Turkish calendar (Istanbul Stock Exchange).
///
/// # Accuracy
///
/// Turkey's Islamic holidays are tabulated (from QuantLib) only through 2034.
/// Querying a date after 2034 panics rather than silently returning an
/// unreliable business-day result.
pub struct Turkey;

impl Turkey {
    /// Builds a Turkish calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Turkey".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    #[allow(clippy::manual_range_contains)]
    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "Turkey Islamic holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        if is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // 23 nisan / National Holiday
            || (d == 23 && m == Month::April)
            // 1 may/ National Holiday
            || (d == 1 && m == Month::May)
            // 19 may/ National Holiday
            || (d == 19 && m == Month::May)
            // 15 july / National Holiday (since 2017)
            || (d == 15 && m == Month::July && y >= 2017)
            // 30 aug/ National Holiday
            || (d == 30 && m == Month::August)
            // 29 ekim  National Holiday
            || (d == 29 && m == Month::October)
        {
            return false;
        }

        // Local Holidays
        if y == 2004 {
            // Kurban
            if (m == Month::February && d <= 4)
                // Ramadan
                || (m == Month::November && d >= 14 && d <= 16)
            {
                return false;
            }
        } else if y == 2005 {
            // Kurban
            if (m == Month::January && d >= 19 && d <= 21)
                // Ramadan
                || (m == Month::November && d >= 2 && d <= 5)
            {
                return false;
            }
        } else if y == 2006 {
            // Kurban
            if (m == Month::January && d >= 10 && d <= 13)
                // Ramadan
                || (m == Month::October && d >= 23 && d <= 25)
                // Kurban
                || (m == Month::December && d == 31)
            {
                return false;
            }
        } else if y == 2007 {
            // Kurban
            if (m == Month::January && d <= 3)
                // Ramadan
                || (m == Month::October && d >= 12 && d <= 14)
                // Kurban
                || (m == Month::December && d >= 20 && d <= 23)
            {
                return false;
            }
        } else if y == 2008 {
            // Ramadan
            if (m == Month::September && d == 30)
                || (m == Month::October && d <= 2)
                // Kurban
                || (m == Month::December && d >= 8 && d <= 11)
            {
                return false;
            }
        } else if y == 2009 {
            // Ramadan
            if (m == Month::September && d >= 20 && d <= 22)
                // Kurban
                || (m == Month::November && d >= 27 && d <= 30)
            {
                return false;
            }
        } else if y == 2010 {
            // Ramadan
            if (m == Month::September && d >= 9 && d <= 11)
                // Kurban
                || (m == Month::November && d >= 16 && d <= 19)
            {
                return false;
            }
        } else if y == 2011 {
            // not clear from borsainstanbul.com
            if (m == Month::October && d == 1) || (m == Month::November && d >= 9 && d <= 13) {
                return false;
            }
        } else if y == 2012 {
            // Ramadan
            if (m == Month::August && d >= 18 && d <= 21)
                // Kurban
                || (m == Month::October && d >= 24 && d <= 28)
            {
                return false;
            }
        } else if y == 2013 {
            // Ramadan
            if (m == Month::August && d >= 7 && d <= 10)
                // Kurban
                || (m == Month::October && d >= 14 && d <= 18)
                // additional holiday for Republic Day
                || (m == Month::October && d == 28)
            {
                return false;
            }
        } else if y == 2014 {
            // Ramadan
            if (m == Month::July && d >= 27 && d <= 30)
                // Kurban
                || (m == Month::October && d >= 4 && d <= 7)
                // additional holiday for Republic Day
                || (m == Month::October && d == 29)
            {
                return false;
            }
        } else if y == 2015 {
            // Ramadan
            if (m == Month::July && d >= 17 && d <= 19)
                // Kurban
                || (m == Month::October && d >= 24 && d <= 27)
            {
                return false;
            }
        } else if y == 2016 {
            // Ramadan
            if (m == Month::July && d >= 5 && d <= 7)
                // Kurban
                || (m == Month::September && d >= 12 && d <= 15)
            {
                return false;
            }
        } else if y == 2017 {
            // Ramadan
            if (m == Month::June && d >= 25 && d <= 27)
                // Kurban
                || (m == Month::September && d >= 1 && d <= 4)
            {
                return false;
            }
        } else if y == 2018 {
            // Ramadan
            if (m == Month::June && d >= 15 && d <= 17)
                // Kurban
                || (m == Month::August && d >= 21 && d <= 24)
            {
                return false;
            }
        } else if y == 2019 {
            // Ramadan
            if (m == Month::June && d >= 4 && d <= 6)
                // Kurban
                || (m == Month::August && d >= 11 && d <= 14)
            {
                return false;
            }
        } else if y == 2020 {
            // Ramadan
            if (m == Month::May && d >= 24 && d <= 26)
                // Kurban
                || (m == Month::July && d == 31)
                || (m == Month::August && d >= 1 && d <= 3)
            {
                return false;
            }
        } else if y == 2021 {
            // Ramadan
            if (m == Month::May && d >= 13 && d <= 15)
                // Kurban
                || (m == Month::July && d >= 20 && d <= 23)
            {
                return false;
            }
        } else if y == 2022 {
            // Ramadan
            if (m == Month::May && d >= 2 && d <= 4)
                // Kurban
                || (m == Month::July && d >= 9 && d <= 12)
            {
                return false;
            }
        } else if y == 2023 {
            // Ramadan
            if (m == Month::April && d >= 21 && d <= 23)
                // Kurban
                // July 1 is also a holiday but falls on a Saturday which is already flagged
                || (m == Month::June && d >= 28 && d <= 30)
            {
                return false;
            }
        } else if y == 2024 {
            // Note: Holidays >= 2024 are not yet officially anounced by borsaistanbul.com
            // and need further validation
            // Ramadan
            if (m == Month::April && d >= 10 && d <= 12)
                // Kurban
                || (m == Month::June && d >= 17 && d <= 19)
            {
                return false;
            }
        } else if y == 2025 {
            // Ramadan
            if (m == Month::March && d == 31)
                || (m == Month::April && d >= 1 && d <= 2)
                // Kurban
                || (m == Month::June && d >= 6 && d <= 9)
            {
                return false;
            }
        } else if y == 2026 {
            // Ramadan
            if (m == Month::March && d >= 20 && d <= 22)
                // Kurban
                || (m == Month::May && d >= 26 && d <= 29)
            {
                return false;
            }
        } else if y == 2027 {
            // Ramadan
            if (m == Month::March && d >= 10 && d <= 12)
                // Kurban
                || (m == Month::May && d >= 16 && d <= 19)
            {
                return false;
            }
        } else if y == 2028 {
            // Ramadan
            if (m == Month::February && d >= 27 && d <= 29)
                // Kurban
                || (m == Month::May && d >= 4 && d <= 7)
            {
                return false;
            }
        } else if y == 2029 {
            // Ramadan
            if (m == Month::February && d >= 15 && d <= 17)
                // Kurban
                || (m == Month::April && d >= 23 && d <= 26)
            {
                return false;
            }
        } else if y == 2030 {
            // Ramadan
            if (m == Month::February && d >= 5 && d <= 7)
                // Kurban
                || (m == Month::April && d >= 13 && d <= 16)
            {
                return false;
            }
        } else if y == 2031 {
            // Ramadan
            if (m == Month::January && d >= 25 && d <= 27)
                // Kurban
                || (m == Month::April && d >= 2 && d <= 5)
            {
                return false;
            }
        } else if y == 2032 {
            // Ramadan
            if (m == Month::January && d >= 14 && d <= 16)
                // Kurban
                || (m == Month::March && d >= 21 && d <= 24)
            {
                return false;
            }
        } else if y == 2033 {
            // Ramadan
            if (m == Month::January && d >= 3 && d <= 5)
                || (m == Month::December && d == 23)
                // Kurban
                || (m == Month::March && d >= 11 && d <= 14)
            {
                return false;
            }
        } else if y == 2034 {
            // Ramadan
            if (m == Month::December && d >= 12 && d <= 14)
                // Kurban
                || (m == Month::February && d == 28)
                || (m == Month::March && d >= 1 && d <= 3)
            {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Turkey::new().name(), "Turkey");
    }

    #[test]
    fn fixed_holidays() {
        let c = Turkey::new();
        for (d, m) in [
            (1, Month::January),  // New Year's Day
            (23, Month::April),   // National Sovereignty and Children's Day
            (1, Month::May),      // Labour and Solidarity Day
            (19, Month::May),     // Youth and Sports Day
            (30, Month::August),  // Victory Day
            (29, Month::October), // Republic Day
        ] {
            assert!(c.is_holiday(Date::new(d, m, 2019)), "{d} {m} 2019");
        }
    }

    #[test]
    fn weekend_rule() {
        let c = Turkey::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Friday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic. New Year's Day is
        // unconditional.
        let c = Turkey::new();
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Turkey::new();
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
