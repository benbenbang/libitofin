//! Taiwanese calendars.
//!
//! Port of `ql/time/calendars/taiwan.{hpp,cpp}`.
//!
//! The per-year holiday lists and nested `if (y == ....)` blocks are kept
//! verbatim from the C++ source (day ranges as `>= .. && <= ..`, one nested
//! `if` per year), so the matching style lints are allowed module-wide.
#![allow(
    clippy::manual_range_contains,
    clippy::nonminimal_bool,
    clippy::collapsible_if
)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Taiwan's public/lunar holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2026;

/// Taiwanese markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Taiwan stock exchange.
    Tsec,
}

/// The Taiwanese calendars.
///
/// # Accuracy
///
/// Taiwan's public/lunar holidays are tabulated (from QuantLib) only through
/// 2026. Querying a date after 2026 panics rather than silently returning an
/// unreliable business-day result.
pub struct Taiwan;

impl Taiwan {
    /// Builds a Taiwanese calendar for the given `market`.
    ///
    /// QuantLib defaults the market to `TSEC`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Tsec => shared(TsecImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct TsecImpl;

impl CalendarImpl for TsecImpl {
    fn name(&self) -> String {
        "Taiwan stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "Taiwan public/lunar holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        if is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Peace Memorial Day
            || (d == 28 && m == Month::February)
            // Labor Day
            || (d == 1 && m == Month::May)
            // Double Tenth
            || (d == 10 && m == Month::October)
        {
            return false;
        }

        if y == 2002 {
            // Dragon Boat Festival and Moon Festival fall on Saturday
            if
            // Chinese Lunar New Year
            (d >= 9 && d <= 17 && m == Month::February)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
            {
                return false;
            }
        }

        if y == 2003 {
            // Tomb Sweeping Day falls on Saturday
            if
            // Chinese Lunar New Year
            ((d >= 31 && m == Month::January) || (d <= 5 && m == Month::February))
                // Dragon Boat Festival
                || (d == 4 && m == Month::June)
                // Moon Festival
                || (d == 11 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2004 {
            // Tomb Sweeping Day falls on Sunday
            if
            // Chinese Lunar New Year
            (d >= 21 && d <= 26 && m == Month::January)
                // Dragon Boat Festival
                || (d == 22 && m == Month::June)
                // Moon Festival
                || (d == 28 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2005 {
            // Dragon Boat and Moon Festival fall on Saturday or Sunday
            if
            // Chinese Lunar New Year
            (d >= 6 && d <= 13 && m == Month::February)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // make up for Labor Day, not seen in other years
                || (d == 2 && m == Month::May)
            {
                return false;
            }
        }

        if y == 2006 {
            // Dragon Boat and Moon Festival fall on Saturday or Sunday
            if
            // Chinese Lunar New Year
            ((d >= 28 && m == Month::January) || (d <= 5 && m == Month::February))
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Dragon Boat Festival
                || (d == 31 && m == Month::May)
                // Moon Festival
                || (d == 6 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2007 {
            if
            // Chinese Lunar New Year
            (d >= 17 && d <= 25 && m == Month::February)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // adjusted holidays
                || (d == 6 && m == Month::April)
                || (d == 18 && m == Month::June)
                // Dragon Boat Festival
                || (d == 19 && m == Month::June)
                // adjusted holiday
                || (d == 24 && m == Month::September)
                // Moon Festival
                || (d == 25 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2008 {
            if
            // Chinese Lunar New Year
            (d >= 4 && d <= 11 && m == Month::February)
                // Tomb Sweeping Day
                || (d == 4 && m == Month::April)
            {
                return false;
            }
        }

        if y == 2009 {
            if
            // Public holiday
            (d == 2 && m == Month::January)
                // Chinese Lunar New Year
                || (d >= 24 && m == Month::January)
                // Tomb Sweeping Day
                || (d == 4 && m == Month::April)
                // Dragon Boat Festival
                || ((d == 28 || d == 29) && m == Month::May)
                // Moon Festival
                || (d == 3 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2010 {
            if
            // Chinese Lunar New Year
            (d >= 13 && d <= 21 && m == Month::January)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Dragon Boat Festival
                || (d == 16 && m == Month::May)
                // Moon Festival
                || (d == 22 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2011 {
            if
            // Spring Festival
            (d >= 2 && d <= 7 && m == Month::February)
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Labour Day
                || (d == 2 && m == Month::May)
                // Dragon Boat Festival
                || (d == 6 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 12 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2012 {
            if
            // Spring Festival
            (d >= 23 && d <= 27 && m == Month::January)
                // Peace Memorial Day
                || (d == 27 && m == Month::February)
                // Children's Day
                // Tomb Sweeping Day
                || (d == 4 && m == Month::April)
                // Labour Day
                || (d == 1 && m == Month::May)
                // Dragon Boat Festival
                || (d == 23 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 30 && m == Month::September)
                // Memorial Day:
                // Founding of the Republic of China
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2013 {
            if
            // Spring Festival
            (d >= 10 && d <= 15 && m == Month::February)
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Labour Day
                || (d == 1 && m == Month::May)
                // Dragon Boat Festival
                || (d == 12 && m == Month::June)
                // Mid-Autumn Festival
                || (d >= 19 && d <= 20 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2014 {
            if
            // Lunar New Year
            (d >= 28 && d <= 30 && m == Month::January)
                // Spring Festival
                || ((d == 31 && m == Month::January) || (d <= 4 && m == Month::February))
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Dragon Boat Festival
                || (d == 2 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 8 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2015 {
            if
            // adjusted holidays
            (d == 2 && m == Month::January)
                // Lunar New Year
                || (d >= 18 && d <= 23 && m == Month::February)
                // adjusted holidays
                || (d == 27 && m == Month::February)
                // adjusted holidays
                || (d == 3 && m == Month::April)
                // adjusted holidays
                || (d == 6 && m == Month::April)
                // adjusted holidays
                || (d == 19 && m == Month::June)
                // adjusted holidays
                || (d == 28 && m == Month::September)
                // adjusted holidays
                || (d == 9 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2016 {
            if
            // Lunar New Year
            (d >= 8 && d <= 12 && m == Month::February)
                // adjusted holidays
                || (d == 29 && m == Month::February)
                // Children's Day
                || (d == 4 && m == Month::April)
                // adjusted holidays
                || (d == 5 && m == Month::April)
                // adjusted holidays
                || (d == 2 && m == Month::May)
                // Dragon Boat Festival
                || (d == 9 && m == Month::June)
                // adjusted holidays
                || (d == 10 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 15 && m == Month::September)
                // adjusted holidays
                || (d == 16 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2017 {
            if
            // adjusted holidays
            (d == 2 && m == Month::January)
                // Lunar New Year
                || ((d >= 27 && m == Month::January) || (d == 1 && m == Month::February))
                // adjusted holidays
                || (d == 27 && m == Month::February)
                // adjusted holidays
                || (d == 3 && m == Month::April)
                // Children's Day
                || (d == 4 && m == Month::April)
                // adjusted holidays
                || (d == 29 && m == Month::May)
                // Dragon Boat Festival
                || (d == 30 && m == Month::May)
                // Mid-Autumn Festival
                || (d == 4 && m == Month::October)
                // adjusted holidays
                || (d == 9 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2018 {
            if
            // Lunar New Year
            (d >= 15 && d <= 20 && m == Month::February)
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // adjusted holidays
                || (d == 6 && m == Month::April)
                // Dragon Boat Festival
                || (d == 18 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 24 && m == Month::September)
                // adjusted holidays
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2019 {
            if
            // Lunar New Year
            (d >= 4 && d <= 8 && m == Month::February)
                // adjusted holidays
                || (d == 1 && m == Month::March)
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Dragon Boat Festival
                || (d == 7 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 13 && m == Month::September)
                // adjusted holidays
                || (d == 11 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2020 {
            if
            // adjusted holiday
            (d == 23 && m == Month::January)
                // Lunar New Year
                || (d >= 24 && d <= 29 && m == Month::January)
                // adjusted holiday
                || (d == 2 && m == Month::April)
                // adjusted holiday
                || (d == 3 && m == Month::April)
                // Dragon Boat Festival
                || (d == 25 && m == Month::June)
                // adjusted holiday
                || (d == 26 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 1 && m == Month::October)
                // adjusted holiday
                || (d == 2 && m == Month::October)
                // adjusted holiday
                || (d == 9 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2021 {
            // Tomb Sweeping Day falls on Sunday
            if
            // adjusted holiday
            (d == 10 && m == Month::February)
                // Lunar New Year
                || (d >= 11 && d <= 16 && m == Month::February)
                // adjusted holiday
                || (d == 1 && m == Month::March)
                // Children's Day
                || (d == 2 && m == Month::April)
                // adjusted holiday
                || (d == 5 && m == Month::April)
                // adjusted holiday
                || (d == 30 && m == Month::April)
                // Dragon Boat Festival
                || (d == 14 && m == Month::June)
                // adjusted holiday
                || (d == 20 && m == Month::September)
                // Mid-Autumn Festival
                || (d == 21 && m == Month::September)
                // adjusted holiday
                || (d == 11 && m == Month::October)
                // adjusted holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2022 {
            // Mid-Autumn Festival falls on Saturday
            if
            // Lunar New Year
            ((d == 31 && m == Month::January) || (d <= 4 && m == Month::February))
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // adjusted holiday
                || (d == 2 && m == Month::May)
                // Dragon Boat Festival
                || (d == 3 && m == Month::June)
                // adjusted holiday
                || (d == 9 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2023 {
            if
            // adjusted holiday
            (d == 2 && m == Month::January)
                // adjusted holiday
                || (d == 20 && m == Month::January)
                // Lunar New Year
                || (d >= 21 && d <= 24 && m == Month::January)
                // adjusted holiday
                || (d >= 25 && d <= 27 && m == Month::January)
                // adjusted holiday
                || (d == 27 && m == Month::February)
                // adjusted holiday
                || (d == 3 && m == Month::April)
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb Sweeping Day
                || (d == 5 && m == Month::April)
                // Dragon Boat Festival
                || (d == 22 && m == Month::June)
                // adjusted holiday
                || (d == 23 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 29 && m == Month::September)
                // adjusted holiday
                || (d == 9 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2024 {
            if
            // adjusted holiday
            (d == 8 && m == Month::February)
                // Lunar New Year
                || (d >= 9 && d <= 12 && m == Month::February)
                // adjusted holiday
                || (d >= 13 && d <= 14 && m == Month::February)
                // Children's Day
                || (d == 4 && m == Month::April)
                // Tomb-sweeping Day
                || (d == 5 && m == Month::April)
                // Dragon Boat Festival
                || (d == 10 && m == Month::June)
                // Mid-autumn/Moon Festival
                || (d == 17 && m == Month::September)
            {
                return false;
            }
        }

        if y == 2025 {
            // Dragon Boat Festival falls on Saturday
            if
            // adjusted holiday
            (d >= 23 && d <= 24 && m == Month::January)
                // Lunar New Year
                || (d >= 27 && d <= 31 && m == Month::January)
                // adjusted holiday
                || (d == 3 && m == Month::April)
                // Children's Day & Tomb-sweeping Day
                || (d == 4 && m == Month::April)
                // adjusted holiday
                || (d == 30 && m == Month::May)
                // Mid-Autumn Festival
                || (d == 6 && m == Month::October)
            {
                return false;
            }
        }

        if y == 2026 {
            if
            // adjusted holiday
            (d >= 12 && d <= 13 && m == Month::February)
                // Lunar New Year
                || (d >= 16 && d <= 20 && m == Month::February)
                // adjusted holiday (Peace Memorial Day falls on Saturday)
                || (d == 27 && m == Month::February)
                // adjusted holiday
                || (d == 3 && m == Month::April)
                // adjusted holiday (Tomb-sweeping Day falls on Sunday)
                || (d == 6 && m == Month::April)
                // Dragon Boat Festival
                || (d == 19 && m == Month::June)
                // Mid-Autumn Festival
                || (d == 25 && m == Month::September)
                // adjusted holiday (National Day falls on Saturday)
                || (d == 9 && m == Month::October)
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

    // Spot-checks against known Taiwanese holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Taiwan::new(Market::Tsec).name(), "Taiwan stock exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Taiwan::new(Market::Tsec);
        // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        // Peace Memorial Day
        assert!(c.is_holiday(Date::new(28, Month::February, 2019)));
        // Labor Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        // Double Tenth
        assert!(c.is_holiday(Date::new(10, Month::October, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Taiwan::new(Market::Tsec);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = Taiwan::new(Market::Tsec);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Taiwan::new(Market::Tsec);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
