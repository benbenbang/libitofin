//! New Zealand calendar.
//!
//! Port of `ql/time/calendars/newzealand.{hpp,cpp}`.

// Range clauses are kept in the verbatim C++ `d >= a && d <= b` form.
#![allow(clippy::manual_range_contains)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// New Zealand markets.
///
/// QuantLib defaults to [`Market::Wellington`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Wellington anniversary calendar.
    Wellington,
    /// Auckland anniversary calendar.
    Auckland,
}

/// Last year for which New Zealand's Matariki holiday is tabulated (matching
/// QuantLib's data). Matariki has no fixed formula - its dates are announced
/// and tabulated per year - so queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting Matariki.
const HOLIDAY_HORIZON: Year = 2052;

/// The New Zealand calendar.
///
/// # Accuracy
///
/// New Zealand's Matariki holiday is tabulated (from QuantLib) only through
/// 2052. Querying a date after 2052 panics rather than silently returning an
/// unreliable business-day result.
pub struct NewZealand;

impl NewZealand {
    /// Builds a New Zealand calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Wellington => shared(WellingtonImpl),
            Market::Auckland => shared(AucklandImpl),
        };
        Calendar::from_impl(imp)
    }
}

/// Common New Zealand holidays shared by both markets.
fn common_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let dd = date.day_of_year();
    let m = date.month();
    let y = date.year();
    assert!(
        y <= HOLIDAY_HORIZON,
        "New Zealand's Matariki holiday is tabulated only through {HOLIDAY_HORIZON} \
         (matching QuantLib); year {y} is beyond the supported horizon"
    );
    let em = western_easter_monday(y);
    !(is_weekend_sat_sun(w)
        // New Year's Day (possibly moved to Monday or Tuesday)
        || ((d == 1 || (d == 3 && (w == Weekday::Monday || w == Weekday::Tuesday)))
            && m == Month::January)
        // Day after New Year's Day (possibly moved to Mon or Tuesday)
        || ((d == 2 || (d == 4 && (w == Weekday::Monday || w == Weekday::Tuesday)))
            && m == Month::January)
        // Waitangi Day. February 6th (possibly moved to Monday since 2013)
        || (d == 6 && m == Month::February)
        || ((d == 7 || d == 8) && w == Weekday::Monday && m == Month::February && y > 2013)
        // Good Friday
        || (dd == em - 3)
        // Easter Monday
        || (dd == em)
        // ANZAC Day. April 25th (possibly moved to Monday since 2013)
        || (d == 25 && m == Month::April)
        || ((d == 26 || d == 27) && w == Weekday::Monday && m == Month::April && y > 2013)
        // Queen's Birthday, first Monday in June
        || (d <= 7 && w == Weekday::Monday && m == Month::June)
        // Labour Day, fourth Monday in October
        || ((d >= 22 && d <= 28) && w == Weekday::Monday && m == Month::October)
        // Christmas, December 25th (possibly Monday or Tuesday)
        || ((d == 25 || (d == 27 && (w == Weekday::Monday || w == Weekday::Tuesday)))
            && m == Month::December)
        // Boxing Day, December 26th (possibly Monday or Tuesday)
        || ((d == 26 || (d == 28 && (w == Weekday::Monday || w == Weekday::Tuesday)))
            && m == Month::December)
        // Matariki, it happens on Friday in June or July
        // official calendar released by the NZ government for the
        // next 30 years
        || (d == 20 && m == Month::June && y == 2025)
        || (d == 21 && m == Month::June && (y == 2030 || y == 2052))
        || (d == 24 && m == Month::June && (y == 2022 || y == 2033 || y == 2044))
        || (d == 25 && m == Month::June && (y == 2027 || y == 2038 || y == 2049))
        || (d == 28 && m == Month::June && y == 2024)
        || (d == 29 && m == Month::June && (y == 2035 || y == 2046))
        || (d == 30 && m == Month::June && y == 2051)
        || (d == 2 && m == Month::July && y == 2032)
        || (d == 3 && m == Month::July && (y == 2043 || y == 2048))
        || (d == 6 && m == Month::July && (y == 2029 || y == 2040))
        || (d == 7 && m == Month::July && (y == 2034 || y == 2045))
        || (d == 10 && m == Month::July && (y == 2026 || y == 2037))
        || (d == 11 && m == Month::July && (y == 2031 || y == 2042))
        || (d == 14 && m == Month::July && (y == 2023 || y == 2028))
        || (d == 15 && m == Month::July && (y == 2039 || y == 2050))
        || (d == 18 && m == Month::July && y == 2036)
        || (d == 19 && m == Month::July && (y == 2041 || y == 2047))
        // Queen Elizabeth's funeral
        || (d == 26 && m == Month::September && y == 2022))
}

struct WellingtonImpl;

impl CalendarImpl for WellingtonImpl {
    fn name(&self) -> String {
        "New Zealand (Wellington)".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        if !common_is_business_day(date) {
            return false;
        }
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        // Anniversary Day, Monday nearest January 22nd
        !((d >= 19 && d <= 25) && w == Weekday::Monday && m == Month::January)
    }
}

struct AucklandImpl;

impl CalendarImpl for AucklandImpl {
    fn name(&self) -> String {
        "New Zealand (Auckland)".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        if !common_is_business_day(date) {
            return false;
        }
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        // Anniversary Day, Monday nearest January 29nd
        !((d >= 26 && w == Weekday::Monday && m == Month::January)
            || (d == 1 && w == Weekday::Monday && m == Month::February))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            NewZealand::new(Market::Wellington).name(),
            "New Zealand (Wellington)"
        );
        assert_eq!(
            NewZealand::new(Market::Auckland).name(),
            "New Zealand (Auckland)"
        );
    }

    #[test]
    fn unconditional_holidays() {
        let c = NewZealand::new(Market::Wellington);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(2, Month::January, 2019))); // Day after New Year's
        assert!(c.is_holiday(Date::new(6, Month::February, 2019))); // Waitangi Day
        assert!(c.is_holiday(Date::new(25, Month::April, 2019))); // ANZAC Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
    }

    #[test]
    fn weekend_rule() {
        let c = NewZealand::new(Market::Auckland);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = NewZealand::new(Market::Wellington);
        // A date at the horizon is still answerable.
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = NewZealand::new(Market::Wellington);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
