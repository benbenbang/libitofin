//! Indonesian calendars.
//!
//! Port of `ql/time/calendars/indonesia.{hpp,cpp}`.
//!
//! The holiday clauses are transcribed verbatim from QuantLib; the style lints
//! below are allowed so the date logic can mirror the C++ clause-for-clause.
#![allow(
    clippy::collapsible_if,
    clippy::manual_range_contains,
    clippy::nonminimal_bool
)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Indonesia's public holidays are tabulated (matching
/// QuantLib's data). Past this year the calendar falls through to its weekend
/// and recurring rules, exactly like QuantLib (which never checks the year).
pub const HOLIDAY_HORIZON: Year = 2014;

/// Indonesian markets.
///
/// QuantLib defaults to [`Market::Idx`]. `BEJ`, `JSX` and `IDX` all share the
/// same holiday schedule (BEJ and JSX were merged into IDX).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Jakarta stock exchange (merged into IDX).
    Bej,
    /// Jakarta stock exchange (merged into IDX).
    Jsx,
    /// Indonesia stock exchange.
    Idx,
}

/// Indonesian calendars.
///
/// # Accuracy
///
/// Indonesia's public holidays are tabulated (from QuantLib) only through 2014.
/// After 2014 the tabulated holidays are not observed and the calendar falls
/// through to its weekend and recurring rules, exactly like QuantLib (which
/// never checks the year).
pub struct Indonesia;

impl Indonesia {
    /// Builds an Indonesian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Bej | Market::Jsx | Market::Idx => shared(BejImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct BejImpl;

impl CalendarImpl for BejImpl {
    fn holiday_horizon(&self) -> Option<Year> {
        Some(HOLIDAY_HORIZON)
    }

    fn name(&self) -> String {
        "Jakarta stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        let dd = date.day_of_year();

        let em = western_easter_monday(y);

        if is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Ascension Thursday
            || (dd == em + 38)
            // Labour Day (national holiday since 2014)
            || (d == 1 && m == Month::May && y >= 2014)
            // Pancasila Day (national holiday since 2017)
            || (d == 1 && m == Month::June && y >= 2017)
            // Independence Day
            || (d == 17 && m == Month::August)
            // Christmas
            || (d == 25 && m == Month::December)
        {
            return false;
        }

        if y == 2005 {
            if
            // Idul Adha
            (d == 21 && m == Month::January)
                // Imlek
                || (d == 9 && m == Month::February)
                // Moslem's New Year Day
                || (d == 10 && m == Month::February)
                // Nyepi
                || (d == 11 && m == Month::March)
                // Birthday of Prophet Muhammad SAW
                || (d == 22 && m == Month::April)
                // Waisak
                || (d == 24 && m == Month::May)
                // Ascension of Prophet Muhammad SAW
                || (d == 2 && m == Month::September)
                // Idul Fitri
                || ((d == 3 || d == 4) && m == Month::November)
                // National leaves
                || ((d == 2 || d == 7 || d == 8) && m == Month::November)
                || (d == 26 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2006 {
            if
            // Idul Adha
            (d == 10 && m == Month::January)
                // Moslem's New Year Day
                || (d == 31 && m == Month::January)
                // Nyepi
                || (d == 30 && m == Month::March)
                // Birthday of Prophet Muhammad SAW
                || (d == 10 && m == Month::April)
                // Ascension of Prophet Muhammad SAW
                || (d == 21 && m == Month::August)
                // Idul Fitri
                || ((d == 24 || d == 25) && m == Month::October)
                // National leaves
                || ((d == 23 || d == 26 || d == 27) && m == Month::October)
            {
                return false;
            }
        }

        if y == 2007 {
            if
            // Nyepi
            (d == 19 && m == Month::March)
                // Waisak
                || (d == 1 && m == Month::June)
                // Ied Adha
                || (d == 20 && m == Month::December)
                // National leaves
                || (d == 18 && m == Month::May)
                || ((d == 12 || d == 15 || d == 16) && m == Month::October)
                || ((d == 21 || d == 24) && m == Month::October)
            {
                return false;
            }
        }

        if y == 2008 {
            if
            // Islamic New Year
            ((d == 10 || d == 11) && m == Month::January)
                // Chinese New Year
                || ((d == 7 || d == 8) && m == Month::February)
                // Saka's New Year
                || (d == 7 && m == Month::March)
                // Birthday of the prophet Muhammad SAW
                || (d == 20 && m == Month::March)
                // Vesak Day
                || (d == 20 && m == Month::May)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 30 && m == Month::July)
                // National leave
                || (d == 18 && m == Month::August)
                // Ied Fitr
                || (d == 30 && m == Month::September)
                || ((d == 1 || d == 2 || d == 3) && m == Month::October)
                // Ied Adha
                || (d == 8 && m == Month::December)
                // Islamic New Year
                || (d == 29 && m == Month::December)
                // New Year's Eve
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2009 {
            if
            // Public holiday
            (d == 2 && m == Month::January)
                // Chinese New Year
                || (d == 26 && m == Month::January)
                // Birthday of the prophet Muhammad SAW
                || (d == 9 && m == Month::March)
                // Saka's New Year
                || (d == 26 && m == Month::March)
                // National leave
                || (d == 9 && m == Month::April)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 20 && m == Month::July)
                // Ied Fitr
                || (d >= 18 && d <= 23 && m == Month::September)
                // Ied Adha
                || (d == 27 && m == Month::November)
                // Islamic New Year
                || (d == 18 && m == Month::December)
                // Public Holiday
                || (d == 24 && m == Month::December)
                // Trading holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2010 {
            if
            // Birthday of the prophet Muhammad SAW
            (d == 26 && m == Month::February)
                // Saka's New Year
                || (d == 16 && m == Month::March)
                // Birth of Buddha
                || (d == 28 && m == Month::May)
                // Ied Fitr
                || (d >= 8 && d <= 14 && m == Month::September)
                // Ied Adha
                || (d == 17 && m == Month::November)
                // Islamic New Year
                || (d == 7 && m == Month::December)
                // Public Holiday
                || (d == 24 && m == Month::December)
                // Trading holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2011 {
            if
            // Chinese New Year
            (d == 3 && m == Month::February)
                // Birthday of the prophet Muhammad SAW
                || (d == 15 && m == Month::February)
                // Birth of Buddha
                || (d == 17 && m == Month::May)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 29 && m == Month::June)
                // Ied Fitr
                || (d >= 29 && m == Month::August)
                || (d <= 2 && m == Month::September)
                // Public Holiday
                || (d == 26 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2012 {
            if
            // Chinese New Year
            (d == 23 && m == Month::January)
                // Saka New Year
                || (d == 23 && m == Month::March)
                // Ied ul-Fitr
                || (d >= 20 && d <= 22 && m == Month::August)
                // Eid ul-Adha
                || (d == 26 && m == Month::October)
                // Islamic New Year
                || (d >= 15 && d <= 16 && m == Month::November)
                // Public Holiday
                || (d == 24 && m == Month::December)
                // Trading Holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2013 {
            if
            // Birthday of the prophet Muhammad SAW
            (d == 24 && m == Month::January)
                // Saka New Year
                || (d == 12 && m == Month::March)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 6 && m == Month::June)
                // Ied ul-Fitr
                || (d >= 5 && d <= 9 && m == Month::August)
                // Eid ul-Adha
                || (d >= 14 && d <= 15 && m == Month::October)
                // Islamic New Year
                || (d == 5 && m == Month::November)
                // Public Holiday
                || (d == 26 && m == Month::December)
                // Trading Holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2014 {
            if
            // Birthday of the prophet Muhammad SAW
            (d == 14 && m == Month::January)
                // Chinese New Year
                || (d == 31 && m == Month::January)
                // Saka New Year
                || (d == 31 && m == Month::March)
                // Birth of Buddha
                || (d == 15 && m == Month::May)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 27 && m == Month::May)
                // Ascension Day of Jesus Christ
                || (d == 29 && m == Month::May)
                // Ied ul-Fitr
                || ((d >= 28 && m == Month::July) || (d == 1 && m == Month::August))
                // Public Holiday
                || (d == 26 && m == Month::December)
                // Trading Holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2015 {
            if
            // Chinese New Year
            (d == 19 && m == Month::February)
                // Birth of Buddha
                || (d == 2 && m == Month::June)
                // Ied ul-Fitr
                || ((d == 16 || d == 17) && m == Month::July)
                || ((d == 20 || d == 21) && m == Month::July)
                // Eid ul-Adha
                || (d == 24 && m == Month::September)
                // Islamic New Year
                || (d == 14 && m == Month::October)
                // Regional elections
                || (d == 9 && m == Month::December)
                // Birthday of the prophet Muhammad SAW
                || (d == 24 && m == Month::December)
                // Trading Holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2016 {
            if
            // Chinese New Year
            (d == 8 && m == Month::February)
                // Saka New Year
                || (d == 9 && m == Month::March)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 6 && m == Month::May)
                // Ied ul-Fitr
                || (d >= 4 && d <= 8 && m == Month::July)
                // Eid ul-Adha
                || (d == 12 && m == Month::September)
                // Birthday of the prophet Muhammad SAW
                || (d == 12 && m == Month::December)
                // National leave
                || (d == 26 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2017 {
            if
            // National leave
            (d == 2 && m == Month::January)
                // Regional elections
                || (d == 15 && m == Month::February)
                // Saka New Year
                || (d == 28 && m == Month::March)
                // Regional elections (Jakarta runoff)
                || (d == 19 && m == Month::April)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 24 && m == Month::April)
                // Birth of Buddha
                || (d == 11 && m == Month::May)
                // Ied ul-Fitr
                || (d == 23 && m == Month::June)
                || (d >= 26 && d <= 30 && m == Month::June)
                // Eid ul-Adha
                || (d == 1 && m == Month::September)
                // Islamic New Year
                || (d == 21 && m == Month::September)
                // Birthday of the prophet Muhammad SAW
                || (d == 1 && m == Month::December)
                // National leave
                || (d == 26 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2018 {
            if
            // Chinese New Year
            (d == 16 && m == Month::February)
                // Birth of Buddha
                || (d == 29 && m == Month::May)
                // Ied ul-Fitr
                || (d >= 11 && d <= 15 && m == Month::June)
                || ((d == 18 || d == 19) && m == Month::June)
                // Eid ul-Adha
                || (d == 22 && m == Month::August)
                // Islamic New Year
                || (d == 11 && m == Month::September)
                // Birthday of the prophet Muhammad SAW
                || (d == 20 && m == Month::November)
                // National leave
                || (d == 24 && m == Month::December)
                // Trading Holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2019 {
            if
            // Chinese New Year
            (d == 5 && m == Month::February)
                // Saka New Year
                || (d == 7 && m == Month::March)
                // Isra' Mi'raj of the prophet Muhammad SAW
                || (d == 3 && m == Month::April)
                // General elections
                || (d == 17 && m == Month::April)
                // Ied ul-Fitr
                || (d >= 3 && d <= 7 && m == Month::June)
                // National leave
                || (d == 24 && m == Month::December)
                // Trading Holiday
                || (d == 31 && m == Month::December)
            {
                return false;
            }
        }

        if y == 2020 {
            if
            // Saka New Year
            (d == 25 && m == Month::March)
                // Birth of Buddha
                || (d == 7 && m == Month::May)
                // Ied ul-Fitr
                || (d == 22 && m == Month::May)
                || (d == 25 && m == Month::May)
                // Eid ul-Adha
                || (d == 31 && m == Month::July)
                // Islamic New Year
                || (d == 20 && m == Month::August)
                // National leave
                || (d == 21 && m == Month::August)
                || (d == 28 && m == Month::October)
                // Birthday of the prophet Muhammad SAW
                || (d == 29 && m == Month::October)
                // National leave
                || (d == 30 && m == Month::October)
                // Regional elections
                || (d == 9 && m == Month::December)
                // National leave
                || (d == 24 && m == Month::December)
                // Ied ul-Fitr
                || (d == 31 && m == Month::December)
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
        assert_eq!(Indonesia::new(Market::Idx).name(), "Jakarta stock exchange");
        assert_eq!(Indonesia::new(Market::Bej).name(), "Jakarta stock exchange");
        assert_eq!(Indonesia::new(Market::Jsx).name(), "Jakarta stock exchange");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Indonesia::new(Market::Idx);
        // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::January, 2014)));
        // Independence Day
        assert!(c.is_holiday(Date::new(17, Month::August, 2014)));
        // Christmas
        assert!(c.is_holiday(Date::new(25, Month::December, 2014)));
    }

    #[test]
    fn weekend_rule() {
        let c = Indonesia::new(Market::Idx);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = Indonesia::new(Market::Idx);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    fn labour_and_pancasila_days_are_rule_based() {
        // indonesia.cpp (v1.44): Labour Day from 2014, Pancasila Day from 2017.
        let c = Indonesia::new(Market::Idx);
        assert!(c.is_business_day(Date::new(1, Month::May, 2013)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2014)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2015)));
        assert!(c.is_business_day(Date::new(1, Month::June, 2016)));
        assert!(c.is_holiday(Date::new(1, Month::June, 2017)));
    }

    #[test]
    fn tables_2015_to_2020_follow_quantlib_v1_44() {
        let c = Indonesia::new(Market::Idx);
        for (d, m, y) in [
            (19, Month::February, 2015),
            (21, Month::July, 2015),
            (8, Month::February, 2016),
            (2, Month::January, 2017),
            (16, Month::February, 2018),
            (5, Month::February, 2019),
            (25, Month::March, 2020),
            (31, Month::December, 2020),
        ] {
            assert!(c.is_holiday(Date::new(d, m, y)), "{d} {m:?} {y}");
        }
        assert!(c.is_business_day(Date::new(18, Month::February, 2015)));
    }

    #[test]
    fn beyond_horizon_falls_through() {
        // QuantLib never checks the year: past the tables only weekends and
        // recurring rules apply, and tabulated dates do not recur.
        let c = Indonesia::new(Market::Idx);
        assert!(c.is_holiday(Date::new(17, Month::August, 2026)));
        assert!(c.is_holiday(Date::new(14, Month::May, 2026)));
        assert!(c.is_business_day(Date::new(14, Month::January, 2026)));
        assert!(c.is_business_day(Date::new(18, Month::August, 2026)));
    }
}
