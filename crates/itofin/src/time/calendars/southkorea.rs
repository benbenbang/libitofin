//! South Korean calendars.
//!
//! Port of `ql/time/calendars/southkorea.{hpp,cpp}`.
//!
//! The `>= .. && <= ..` day-range checks are kept verbatim from the C++ source
//! rather than rewritten as `RangeInclusive::contains`, so the corresponding
//! clippy lint is allowed module-wide.
#![allow(clippy::manual_range_contains)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which South Korea's public/lunar holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2050;

/// South Korean markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Public holidays.
    Settlement,
    /// Korea exchange.
    Krx,
}

/// The South Korean calendars.
///
/// # Accuracy
///
/// South Korea's public and lunar holidays are tabulated (from QuantLib) only
/// through 2050. Querying a date after 2050 panics rather than silently
/// returning an unreliable business-day result.
pub struct SouthKorea;

impl SouthKorea {
    /// Builds a South Korean calendar for the given `market`.
    ///
    /// QuantLib defaults the market to `KRX`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Krx => shared(KrxImpl),
        };
        Calendar::from_impl(imp)
    }
}

/// Public-holiday schedule shared by both the settlement and KRX calendars.
fn settlement_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let m = date.month();
    let y = date.year();

    assert!(
        y <= HOLIDAY_HORIZON,
        "South Korea public/lunar holidays are tabulated only through {HOLIDAY_HORIZON} \
         (matching QuantLib); year {y} is beyond the supported horizon"
    );

    !(is_weekend_sat_sun(w)
        // New Year's Day
        || (d == 1 && m == Month::January)
        // Independence Day
        || (d == 1 && m == Month::March)
        || (w == Weekday::Monday && (d == 2 || d == 3) && m == Month::March && y > 2021)
        // Arbour Day
        || (d == 5 && m == Month::April && y <= 2005)
        // Labour Day
        || (d == 1 && m == Month::May)
        // Children's Day
        || (d == 5 && m == Month::May)
        || (w == Weekday::Monday && (d == 6 || d == 7) && m == Month::May && y > 2013)
        // Memorial Day
        || (d == 6 && m == Month::June)
        // Constitution Day
        || (d == 17 && m == Month::July && y <= 2007)
        // Liberation Day
        || (d == 15 && m == Month::August)
        || (w == Weekday::Monday && (d == 16 || d == 17) && m == Month::August && y > 2020)
        // National Foundation Day
        || (d == 3 && m == Month::October)
        || (w == Weekday::Monday && (d == 4 || d == 5) && m == Month::October && y > 2020)
        // Christmas Day
        || (d == 25 && m == Month::December)
        || (w == Weekday::Monday && (d == 26 || d == 27) && m == Month::December && y > 2022)

        // Lunar New Year
        || ((d == 21 || d == 22 || d == 23) && m == Month::January && y == 2004)
        || ((d == 8 || d == 9 || d == 10) && m == Month::February && y == 2005)
        || ((d == 28 || d == 29 || d == 30) && m == Month::January && y == 2006)
        || (d == 19 && m == Month::February && y == 2007)
        || ((d == 6 || d == 7 || d == 8) && m == Month::February && y == 2008)
        || ((d == 25 || d == 26 || d == 27) && m == Month::January && y == 2009)
        || ((d == 13 || d == 14 || d == 15) && m == Month::February && y == 2010)
        || ((d == 2 || d == 3 || d == 4) && m == Month::February && y == 2011)
        || ((d == 23 || d == 24) && m == Month::January && y == 2012)
        || (d == 11 && m == Month::February && y == 2013)
        || ((d == 30 || d == 31) && m == Month::January && y == 2014)
        || ((d == 18 || d == 19 || d == 20) && m == Month::February && y == 2015)
        || ((d >= 7 && d <= 10) && m == Month::February && y == 2016)
        || ((d >= 27 && d <= 30) && m == Month::January && y == 2017)
        || ((d == 15 || d == 16 || d == 17) && m == Month::February && y == 2018)
        || ((d == 4 || d == 5 || d == 6) && m == Month::February && y == 2019)
        || ((d >= 24 && d <= 27) && m == Month::January && y == 2020)
        || ((d == 11 || d == 12 || d == 13) && m == Month::February && y == 2021)
        || (((d == 31 && m == Month::January)
            || ((d == 1 || d == 2) && m == Month::February))
            && y == 2022)
        || ((d == 23 || d == 24) && m == Month::January && y == 2023)
        || ((d >= 9 && d <= 12) && m == Month::February && y == 2024)
        || ((d == 28 || d == 29 || d == 30) && m == Month::January && y == 2025)
        || ((d == 16 || d == 17 || d == 18) && m == Month::February && y == 2026)
        || ((d == 8 || d == 9) && m == Month::February && y == 2027)
        || ((d == 26 || d == 27 || d == 28) && m == Month::January && y == 2028)
        || ((d == 12 || d == 13 || d == 14) && m == Month::February && y == 2029)
        || ((d == 4 || d == 5) && m == Month::February && y == 2030)
        || ((d == 22 || d == 23 || d == 24) && m == Month::January && y == 2031)
        || ((d == 10 || d == 11 || d == 12) && m == Month::February && y == 2032)
        || (((d == 31 && m == Month::January)
            || ((d == 1 || d == 2) && m == Month::February))
            && y == 2033)
        || ((d == 20 || d == 21) && m == Month::February && y == 2034)
        || ((d == 7 || d == 8 || d == 9) && m == Month::February && y == 2035)
        || ((d == 28 || d == 29 || d == 30) && m == Month::January && y == 2036)
        || ((d == 16 || d == 17) && m == Month::February && y == 2037)
        || ((d == 3 || d == 4 || d == 5) && m == Month::February && y == 2038)
        || ((d == 24 || d == 25 || d == 26) && m == Month::January && y == 2039)
        || ((d == 13 || d == 14) && m == Month::February && y == 2040)
        || (((d == 31 && m == Month::January)
            || ((d == 1 || d == 2) && m == Month::February))
            && y == 2041)
        || ((d == 21 || d == 22 || d == 23) && m == Month::January && y == 2042)
        || ((d == 9 || d == 10 || d == 11) && m == Month::February && y == 2043)
        || ((((d == 29 || d == 30 || d == 31) && m == Month::January)
            || (d == 1 && m == Month::February))
            && y == 2044)
        || ((d == 16 || d == 17 || d == 18) && m == Month::February && y == 2045)
        || ((d == 5 || d == 6 || d == 7) && m == Month::February && y == 2046)
        || ((d >= 25 && d <= 28) && m == Month::January && y == 2047)
        || ((d == 13 || d == 14 || d == 15) && m == Month::February && y == 2048)
        || ((d == 1 || d == 2 || d == 3) && m == Month::February && y == 2049)
        || ((d == 24 || d == 25) && m == Month::January && y == 2050)

        // Election Days
        || (d == 15 && m == Month::April && y == 2004) // National Assembly
        || (d == 31 && m == Month::May && y == 2006) // Regional election
        || (d == 19 && m == Month::December && y == 2007) // Presidency
        || (d == 9 && m == Month::April && y == 2008) // National Assembly
        || (d == 2 && m == Month::June && y == 2010) // Local election
        || (d == 11 && m == Month::April && y == 2012) // National Assembly
        || (d == 19 && m == Month::December && y == 2012) // Presidency
        || (d == 4 && m == Month::June && y == 2014) // Local election
        || (d == 13 && m == Month::April && y == 2016) // National Assembly
        || (d == 9 && m == Month::May && y == 2017) // Presidency
        || (d == 13 && m == Month::June && y == 2018) // Local election
        || (d == 15 && m == Month::April && y == 2020) // National Assembly
        || (d == 9 && m == Month::March && y == 2022) // Presidency
        || (d == 1 && m == Month::June && y == 2022) // Local election
        || (d == 10 && m == Month::April && y == 2024) // National Assembly
        // Buddha's birthday
        || (d == 26 && m == Month::May && y == 2004)
        || (d == 15 && m == Month::May && y == 2005)
        || (d == 5 && m == Month::May && y == 2006)
        || (d == 24 && m == Month::May && y == 2007)
        || (d == 12 && m == Month::May && y == 2008)
        || (d == 2 && m == Month::May && y == 2009)
        || (d == 21 && m == Month::May && y == 2010)
        || (d == 10 && m == Month::May && y == 2011)
        || (d == 28 && m == Month::May && y == 2012)
        || (d == 17 && m == Month::May && y == 2013)
        || (d == 6 && m == Month::May && y == 2014)
        || (d == 25 && m == Month::May && y == 2015)
        || (d == 14 && m == Month::May && y == 2016)
        || (d == 3 && m == Month::May && y == 2017)
        || (d == 22 && m == Month::May && y == 2018)
        || (d == 12 && m == Month::May && y == 2019)
        || (d == 30 && m == Month::April && y == 2020)
        || (d == 19 && m == Month::May && y == 2021)
        || (d == 8 && m == Month::May && y == 2022)
        || (d == 29 && m == Month::May && y == 2023) // Substitute holiday
        || (d == 15 && m == Month::May && y == 2024)
        || (d == 6 && m == Month::May && y == 2025)
        || (d == 25 && m == Month::May && y == 2026) // Substitute holiday
        || (d == 13 && m == Month::May && y == 2027)
        || (d == 2 && m == Month::May && y == 2028)
        || (d == 21 && m == Month::May && y == 2029) // Substitute holiday
        || (d == 9 && m == Month::May && y == 2030)
        || (d == 28 && m == Month::May && y == 2031)
        || (d == 17 && m == Month::May && y == 2032) // Substitute holiday
        || (d == 6 && m == Month::May && y == 2033)
        || (d == 25 && m == Month::May && y == 2034)
        || (d == 15 && m == Month::May && y == 2035)
        || (d == 6 && m == Month::May && y == 2036) // Substitute holiday
        || (d == 22 && m == Month::May && y == 2037)
        || (d == 11 && m == Month::May && y == 2038)
        || (d == 2 && m == Month::May && y == 2039) // Substitute holiday
        || (d == 18 && m == Month::May && y == 2040)
        || (d == 7 && m == Month::May && y == 2041)
        || (d == 26 && m == Month::May && y == 2042)
        || (d == 18 && m == Month::May && y == 2043) // Substitute holiday
        || (d == 6 && m == Month::May && y == 2044)
        || (d == 24 && m == Month::May && y == 2045)
        || (d == 14 && m == Month::May && y == 2046) // Substitute holiday
        || (d == 2 && m == Month::May && y == 2047)
        || (d == 20 && m == Month::May && y == 2048)
        || (d == 10 && m == Month::May && y == 2049) // Substitute holiday
        || (d == 30 && m == Month::May && y == 2050) // Substitute holiday

        // Special holiday: 70 years from Independence Day
        || (d == 14 && m == Month::August && y == 2015)
        // Special temporary holiday
        || (d == 17 && m == Month::August && y == 2020)
        || (d == 2 && m == Month::October && y == 2023)
        || (d == 1 && m == Month::October && y == 2024)
        || (d == 27 && m == Month::January && y == 2025)

        // Harvest Moon Day
        || ((d == 27 || d == 28 || d == 29) && m == Month::September && y == 2004)
        || ((d == 17 || d == 18 || d == 19) && m == Month::September && y == 2005)
        || ((d == 5 || d == 6 || d == 7) && m == Month::October && y == 2006)
        || ((d == 24 || d == 25 || d == 26) && m == Month::September && y == 2007)
        || ((d == 13 || d == 14 || d == 15) && m == Month::September && y == 2008)
        || ((d == 2 || d == 3 || d == 4) && m == Month::October && y == 2009)
        || ((d == 21 || d == 22 || d == 23) && m == Month::September && y == 2010)
        || ((d == 12 || d == 13) && m == Month::September && y == 2011)
        || (d == 1 && m == Month::October && y == 2012)
        || ((d == 18 || d == 19 || d == 20) && m == Month::September && y == 2013)
        || ((d == 8 || d == 9 || d == 10) && m == Month::September && y == 2014)
        || ((d == 28 || d == 29) && m == Month::September && y == 2015)
        || ((d == 14 || d == 15 || d == 16) && m == Month::September && y == 2016)
        || ((d >= 3 && d <= 6) && m == Month::October && y == 2017)
        || ((d >= 23 && d <= 26) && m == Month::September && y == 2018)
        || ((d == 12 || d == 13 || d == 14) && m == Month::September && y == 2019)
        || (((d == 30 && m == Month::September)
            || ((d == 1 || d == 2) && m == Month::October))
            && y == 2020)
        || ((d == 20 || d == 21 || d == 22) && m == Month::September && y == 2021)
        || ((d == 9 || d == 10 || d == 11) && m == Month::September && y == 2022)
        || ((d >= 9 && d <= 12) && m == Month::September && y == 2022)
        || ((d == 28 || d == 29 || d == 30) && m == Month::September && y == 2023)
        || ((d == 16 || d == 17 || d == 18) && m == Month::September && y == 2024)
        || ((d == 6 || d == 7 || d == 8) && m == Month::October && y == 2025)
        || ((d == 24 || d == 25 || d == 26) && m == Month::September && y == 2026)
        || ((d == 14 || d == 15 || d == 16) && m == Month::September && y == 2027)
        || ((d >= 2 && d <= 5) && m == Month::October && y == 2028)
        || ((d >= 21 && d <= 24) && m == Month::September && y == 2029)
        || ((d == 11 || d == 12 || d == 13) && m == Month::September && y == 2030)
        || (((d == 30 && m == Month::September)
            || ((d == 1 || d == 2) && m == Month::October))
            && y == 2031)
        || ((d == 20 || d == 21) && m == Month::September && y == 2032)
        || ((d == 7 || d == 8 || d == 9) && m == Month::September && y == 2033)
        || ((d == 26 || d == 27 || d == 28) && m == Month::September && y == 2034)
        || ((d == 17 || d == 18) && m == Month::September && y == 2035)
        || ((d >= 3 && d <= 7) && m == Month::October && y == 2036)
        || ((d == 23 || d == 24 || d == 25) && m == Month::September && y == 2037)
        || ((d == 13 || d == 14 || d == 15) && m == Month::September && y == 2038)
        || ((d == 3 || d == 4 || d == 5) && m == Month::October && y == 2039)
        || ((d == 20 || d == 21 || d == 22) && m == Month::September && y == 2040)
        || ((d == 9 || d == 10 || d == 11) && m == Month::September && y == 2041)
        || ((d == 29 || d == 30) && m == Month::September && y == 2042)
        || ((d == 16 || d == 17 || d == 18) && m == Month::September && y == 2043)
        || ((d == 4 || d == 5 || d == 6) && m == Month::October && y == 2044)
        || ((d == 25 || d == 26 || d == 27) && m == Month::September && y == 2045)
        || ((d >= 14 && d <= 17) && m == Month::September && y == 2046)
        || ((d == 4 || d == 5 || d == 7) && m == Month::October && y == 2047)
        || ((d == 21 || d == 22 || d == 23) && m == Month::September && y == 2048)
        || ((d >= 10 && d <= 13) && m == Month::September && y == 2049)
        || ((((d == 29 || d == 30) && m == Month::September)
            || (d == 1 && m == Month::October))
            && y == 2050)

        // Hangul Proclamation of Korea
        || (d == 9 && m == Month::October && y >= 2013)
        || (w == Weekday::Monday && (d == 10 || d == 11) && m == Month::October && y > 2020))
}

struct SettlementImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "South-Korean settlement".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        settlement_is_business_day(date)
    }
}

struct KrxImpl;

impl CalendarImpl for KrxImpl {
    fn name(&self) -> String {
        "South-Korea exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        // public holidays
        if !settlement_is_business_day(date) {
            return false;
        }

        let d = date.day_of_month();
        let w = date.weekday();
        let m = date.month();
        let y = date.year();

        // Year-end closing
        if (((d == 29 || d == 30) && w == Weekday::Friday) || d == 31) && m == Month::December {
            return false;
        }
        // occasional closing days (KRX day)
        if (d == 6 && m == Month::May && y == 2016) || (d == 2 && m == Month::October && y == 2017)
        {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against known South Korean holidays; not a full transcription
    // of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            SouthKorea::new(Market::Settlement).name(),
            "South-Korean settlement"
        );
        assert_eq!(SouthKorea::new(Market::Krx).name(), "South-Korea exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = SouthKorea::new(Market::Settlement);
        // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        // Independence Day
        assert!(c.is_holiday(Date::new(1, Month::March, 2019)));
        // Labour Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        // Children's Day
        assert!(c.is_holiday(Date::new(5, Month::May, 2019)));
        // Memorial Day
        assert!(c.is_holiday(Date::new(6, Month::June, 2019)));
        // Liberation Day
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        // National Foundation Day
        assert!(c.is_holiday(Date::new(3, Month::October, 2019)));
        // Christmas Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = SouthKorea::new(Market::Krx);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic.
        let c = SouthKorea::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = SouthKorea::new(Market::Settlement);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
