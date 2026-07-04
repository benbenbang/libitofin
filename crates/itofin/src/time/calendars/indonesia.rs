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

/// Last year for which Indonesia's public holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2014;

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
/// Querying a date after 2014 panics rather than silently returning an
/// unreliable business-day result.
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

        assert!(
            y <= HOLIDAY_HORIZON,
            "Indonesia public holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        let em = western_easter_monday(y);

        if is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Ascension Thursday
            || (dd == em + 38)
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
                // Labour Day
                || (d == 1 && m == Month::May)
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
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Indonesia::new(Market::Idx);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
