//! Uzbekistan calendar.
//!
//! Port of `ql/time/calendars/uzbekistan.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::calendars::islamicholidays::moon_sighting::{is_eid_al_adha, is_eid_al_fitr};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Uzbekistan's Islamic (Eid) holidays are tabulated via
/// `moon_sighting` (matching QuantLib's data). Queries beyond this year cannot
/// be answered reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2040;

/// Market handled by the Uzbekistan calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Market {
    /// Uzbekistan Stock Exchange.
    UZSE,
}

/// The Uzbekistan calendar. The default market is [`Market::UZSE`].
///
/// # Accuracy
///
/// Uzbekistan's Islamic (Eid) holidays are tabulated (from QuantLib, via the
/// `moon_sighting` tables) only through 2040. Querying a date after 2040 panics
/// rather than silently returning an unreliable business-day result.
pub struct Uzbekistan;

impl Uzbekistan {
    /// Builds an Uzbekistan calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::UZSE => shared(Impl),
        };
        Calendar::from_impl(imp)
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Uzbekistan Stock Exchange".to_string()
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
            "Uzbekistan Islamic (Eid) holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        !(is_weekend_sat_sun(w)
            || is_eid_al_fitr(date)
            || is_eid_al_adha(date)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // International Womens Day
            || (d == 8 && m == Month::March)
            // Navruz(Persian New Year)
            || (d == 21 && m == Month::March)
            // Day of Remembrance and Honors
            || (d == 9 && m == Month::May)
            // Independence Day
            || (d == 1 && m == Month::September)
            // Teachers Day
            || (d == 1 && m == Month::October)
            // Constitution Day
            || (d == 8 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            Uzbekistan::new(Market::UZSE).name(),
            "Uzbekistan Stock Exchange"
        );
    }

    #[test]
    fn fixed_holidays() {
        let c = Uzbekistan::new(Market::UZSE);
        for (d, m) in [
            (1, Month::January),   // New Year's Day
            (8, Month::March),     // International Women's Day
            (21, Month::March),    // Navruz
            (9, Month::May),       // Day of Remembrance and Honors
            (1, Month::September), // Independence Day
            (1, Month::October),   // Teachers' Day
            (8, Month::December),  // Constitution Day
        ] {
            assert!(c.is_holiday(Date::new(d, m, 2020)), "{d} {m} 2020");
        }
    }

    #[test]
    fn weekend_rule() {
        let c = Uzbekistan::new(Market::UZSE);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Friday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic. New Year's Day is
        // unconditional.
        let c = Uzbekistan::new(Market::UZSE);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Uzbekistan::new(Market::UZSE);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
