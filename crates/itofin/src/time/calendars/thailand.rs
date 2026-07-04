//! Thailand calendars.
//!
//! Port of `ql/time/calendars/thailand.{hpp,cpp}`.
//!
//! The holiday clauses are kept verbatim from the C++ source rather than
//! factored for minimality, so the corresponding clippy lint is allowed
//! module-wide.
#![allow(clippy::nonminimal_bool)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which Thailand's public/lunar holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2025;

/// The Thailand calendar (Stock Exchange of Thailand).
///
/// # Accuracy
///
/// Thailand's public/lunar holidays are tabulated (from QuantLib) only through
/// 2025. Querying a date after 2025 panics rather than silently returning an
/// unreliable business-day result.
pub struct Thailand;

impl Thailand {
    /// Builds a Thailand calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(SetImpl))
    }
}

struct SetImpl;

impl CalendarImpl for SetImpl {
    fn name(&self) -> String {
        "Thailand stock exchange".to_string()
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
            "Thailand public/lunar holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        if is_weekend_sat_sun(w)
            // New Year's Day
            || ((d == 1 || (d == 3 && w == Weekday::Monday)) && m == Month::January)
            // Chakri Memorial Day
            || ((d == 6 || ((d == 7 || d == 8) && w == Weekday::Monday)) && m == Month::April)
            // Songkran Festival (was cancelled in 2020 due to the Covid-19 Pandamic)
            || ((d == 13 || d == 14 || d == 15) && m == Month::April && y != 2020)
            // Substitution Songkran Festival, usually not more than 5 days in total (was cancelled
            // in 2020 due to the Covid-19 Pandamic)
            || (d == 16 && (w == Weekday::Monday || w == Weekday::Tuesday) && m == Month::April && y != 2020)
            // Labor Day
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday)) && m == Month::May)
            // Coronation Day
            || ((d == 4 || ((d == 5 || d == 6) && w == Weekday::Monday)) && m == Month::May && y >= 2019)
            // H.M.Queen Suthida Bajrasudhabimalalakshana's Birthday
            || ((d == 3 || ((d == 4 || d == 5) && w == Weekday::Monday)) && m == Month::June && y >= 2019)
            // H.M. King Maha Vajiralongkorn Phra Vajiraklaochaoyuhua's Birthday
            || ((d == 28 || ((d == 29 || d == 30) && w == Weekday::Monday)) && m == Month::July && y >= 2017)
            // H.M. Queen Sirikit The Queen Mother's Birthday / Mother's Day
            || ((d == 12 || ((d == 13 || d == 14) && w == Weekday::Monday)) && m == Month::August)
            // H.M. King Bhumibol Adulyadej The Great Memorial Day
            || ((d == 13 || ((d == 14 || d == 15) && w == Weekday::Monday)) && m == Month::October && y >= 2017)
            // Chulalongkorn Day
            || ((d == 23 || ((d == 24 || d == 25) && w == Weekday::Monday)) && m == Month::October && y != 2021)  // Moved 2021, see below
            // H.M. King Bhumibol Adulyadej The Great's Birthday/ National Day / Father's Day
            || ((d == 5 || ((d == 6 || d == 7) && w == Weekday::Monday)) && m == Month::December)
            // Constitution Day
            || ((d == 10 || ((d == 11 || d == 12) && w == Weekday::Monday)) && m == Month::December)
            // New Year's Eve
            || ((d == 31 && m == Month::December) || (d == 2 && w == Weekday::Monday && m == Month::January && y != 2024))
        // Moved 2024
        {
            return false;
        }

        if (y == 2000)
            && ((d == 21 && m == Month::February)  // Makha Bucha Day (Substitution Day)
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 17 && m == Month::May)       // Wisakha Bucha Day
                || (d == 17 && m == Month::July)      // Buddhist Lent Day
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2001)
            && ((d == 8 && m == Month::February) // Makha Bucha Day
                || (d == 7 && m == Month::May)      // Wisakha Bucha Day
                || (d == 8 && m == Month::May)      // Coronation Day (Substitution Day)
                || (d == 6 && m == Month::July)     // Buddhist Lent Day
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        // 2002, 2003 and 2004 are missing

        if (y == 2005)
            && ((d == 23 && m == Month::February) // Makha Bucha Day
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 23 && m == Month::May)      // Wisakha Bucha Day (Substitution Day for Sunday 22 May)
                || (d == 1 && m == Month::July)      // Mid Year Closing Day
                || (d == 22 && m == Month::July)     // Buddhist Lent Day
                || (d == 24 && m == Month::October))
        // Chulalongkorn Day (Substitution Day for Sunday 23 October)
        {
            return false;
        }

        if (y == 2006)
            && ((d == 13 && m == Month::February) // Makha Bucha Day
                || (d == 19 && m == Month::April)    // Special Holiday
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 12 && m == Month::May)      // Wisakha Bucha Day
                || (d == 12 && m == Month::June)     // Special Holidays (Due to the auspicious occasion of the
                                                     // celebration of 60th Anniversary of His Majesty's Accession
                                                     // to the throne. For Bangkok, Samut Prakan, Nonthaburi,
                                                     // Pathumthani and Nakhon Pathom province)
                || (d == 13 && m == Month::June)     // Special Holidays (as above)
                || (d == 11 && m == Month::July)     // Buddhist Lent Day
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2007)
            && ((d == 5 && m == Month::March)     // Makha Bucha Day (Substitution Day for Saturday 3 March)
                || (d == 7 && m == Month::May)       // Coronation Day (Substitution Day for Saturday 5 May)
                || (d == 31 && m == Month::May)      // Wisakha Bucha Day
                || (d == 30 && m == Month::July)     // Asarnha Bucha Day (Substitution Day for Sunday 29 July)
                || (d == 23 && m == Month::October)  // Chulalongkorn Day
                || (d == 24 && m == Month::December))
        // Special Holiday
        {
            return false;
        }

        if (y == 2008)
            && ((d == 21 && m == Month::February) // Makha Bucha Day
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 19 && m == Month::May)      // Wisakha Bucha Day
                || (d == 1 && m == Month::July)      // Mid Year Closing Day
                || (d == 17 && m == Month::July)     // Asarnha Bucha Day
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2009)
            && ((d == 2 && m == Month::January)  // Special Holiday
                || (d == 9 && m == Month::February) // Makha Bucha Day
                || (d == 5 && m == Month::May)      // Coronation Day
                || (d == 8 && m == Month::May)      // Wisakha Bucha Day
                || (d == 1 && m == Month::July)     // Mid Year Closing Day
                || (d == 6 && m == Month::July)     // Special Holiday
                || (d == 7 && m == Month::July)     // Asarnha Bucha Day
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2010)
            && ((d == 1 && m == Month::March)    // Substitution for Makha Bucha Day(Sunday 28 February)
                || (d == 5 && m == Month::May)      // Coronation Day
                || (d == 20 && m == Month::May)     // Special Holiday
                || (d == 21 && m == Month::May)     // Special Holiday
                || (d == 28 && m == Month::May)     // Wisakha Bucha Day
                || (d == 1 && m == Month::July)     // Mid Year Closing Day
                || (d == 26 && m == Month::July)    // Asarnha Bucha Day
                || (d == 13 && m == Month::August)  // Special Holiday
                || (d == 25 && m == Month::October))
        // Substitution for Chulalongkorn Day(Saturday 23 October)
        {
            return false;
        }

        if (y == 2011)
            && ((d == 18 && m == Month::February) // Makha Bucha Day
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 16 && m == Month::May)      // Special Holiday
                || (d == 17 && m == Month::May)      // Wisakha Bucha Day
                || (d == 1 && m == Month::July)      // Mid Year Closing Day
                || (d == 15 && m == Month::July)     // Asarnha Bucha Day
                || (d == 24 && m == Month::October))
        // Substitution for Chulalongkorn Day(Sunday 23 October)
        {
            return false;
        }

        if (y == 2012)
            && ((d == 3 && m == Month::January)  // Special Holiday
                || (d == 7 && m == Month::March)    // Makha Bucha Day 2/
                || (d == 9 && m == Month::April)    // Special Holiday
                || (d == 7 && m == Month::May)      // Substitution for Coronation Day(Saturday 5 May)
                || (d == 4 && m == Month::June)     // Wisakha Bucha Day
                || (d == 2 && m == Month::August)   // Asarnha Bucha Day
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2013)
            && ((d == 25 && m == Month::February) // Makha Bucha Day
                || (d == 6 && m == Month::May)       // Substitution for Coronation Day(Sunday 5 May)
                || (d == 24 && m == Month::May)      // Wisakha Bucha Day
                || (d == 1 && m == Month::July)      // Mid Year Closing Day
                || (d == 22 && m == Month::July)     // Asarnha Bucha Day 2/
                || (d == 23 && m == Month::October)  // Chulalongkorn Day
                || (d == 30 && m == Month::December))
        // Special Holiday
        {
            return false;
        }

        if (y == 2014)
            && ((d == 14 && m == Month::February) // Makha Bucha Day
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 13 && m == Month::May)      // Wisakha Bucha Day
                || (d == 1 && m == Month::July)      // Mid Year Closing Day
                || (d == 11 && m == Month::July)     // Asarnha Bucha Day 1/
                || (d == 11 && m == Month::August)   // Special Holiday
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2015)
            && ((d == 2 && m == Month::January)  // Special Holiday
                || (d == 4 && m == Month::March)    // Makha Bucha Day
                || (d == 4 && m == Month::May)      // Special Holiday
                || (d == 5 && m == Month::May)      // Coronation Day
                || (d == 1 && m == Month::June)     // Wisakha Bucha Day
                || (d == 1 && m == Month::July)     // Mid Year Closing Day
                || (d == 30 && m == Month::July)    // Asarnha Bucha Day 1/
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2016)
            && ((d == 22 && m == Month::February) // Makha Bucha Day
                || (d == 5 && m == Month::May)       // Coronation Day
                || (d == 6 && m == Month::May)       // Special Holiday
                || (d == 20 && m == Month::May)      // Wisakha Bucha Day
                || (d == 1 && m == Month::July)      //  Mid Year Closing Day
                || (d == 18 && m == Month::July)     // Special Holiday
                || (d == 19 && m == Month::July)     // Asarnha Bucha Day 1/
                || (d == 24 && m == Month::October))
        // Substitution for Chulalongkorn Day (Sunday 23rd October)
        {
            return false;
        }

        if (y == 2017)
            && ((d == 13 && m == Month::February)  // Makha Bucha Day
                || (d == 10 && m == Month::May)       // Wisakha Bucha Day
                || (d == 10 && m == Month::July)      // Asarnha Bucha Day
                || (d == 23 && m == Month::October)   // Chulalongkorn Day
                || (d == 26 && m == Month::October))
        // Special Holiday
        {
            return false;
        }

        if (y == 2018)
            && ((d == 1 && m == Month::March)    // Makha Bucha Day
                || (d == 29 && m == Month::May)     // Wisakha Bucha Day
                || (d == 27 && m == Month::July)    // Asarnha Bucha Day1
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2019)
            && ((d == 19 && m == Month::February) // Makha Bucha Day
                || (d == 6 && m == Month::May)    // Special Holiday
                || (d == 20 && m == Month::May)   // Wisakha Bucha Day
                || (d == 16 && m == Month::July))
        // Asarnha Bucha Day
        {
            return false;
        }

        if (y == 2020)
            && ((d == 10 && m == Month::February)    // Makha Bucha Day
                || (d == 6 && m == Month::May)       // Wisakha Bucha Day
                || (d == 6 && m == Month::July)      // Asarnha Bucha Day
                || (d == 27 && m == Month::July)     // Substitution for Songkran Festival
                || (d == 4 && m == Month::September) // Substitution for Songkran Festival
                || (d == 7 && m == Month::September) // Substitution for Songkran Festival
                || (d == 11 && m == Month::December))
        // Special Holiday
        {
            return false;
        }

        if (y == 2021)
            && ((d == 12 && m == Month::February)     // Special Holiday
                || (d == 26 && m == Month::February)  // Makha Bucha Day
                || (d == 26 && m == Month::May)       // Wisakha Bucha Day
                || (d == 26 && m == Month::July)      // Substitution for Asarnha Bucha Day (Saturday 24th July 2021)
                || (d == 24 && m == Month::September) // Special Holiday
                || (d == 22 && m == Month::October))
        // Substitution for Chulalongkorn Day
        {
            return false;
        }

        if (y == 2022)
            && ((d == 16 && m == Month::February)   // Makha Bucha Day
                || (d == 16 && m == Month::May)     // Substitution for Wisakha Bucha Day (Sunday 15th May 2022)
                || (d == 13 && m == Month::July)    // Asarnha Bucha Day
                || (d == 29 && m == Month::July)    // Additional special holiday (added)
                || (d == 14 && m == Month::October) // Additional special holiday (added)
                || (d == 24 && m == Month::October))
        // Substitution for Chulalongkorn Day (Sunday 23rd October 2022)
        {
            return false;
        }

        if (y == 2023)
            && ((d == 6 && m == Month::March)        // Makha Bucha Day
                || (d == 5 && m == Month::May)       // Additional special holiday (added)
                || (d == 5 && m == Month::June)      // Substitution for H.M. Queen's birthday and Wisakha Bucha Day (Saturday 3rd June 2022)
                || (d == 1 && m == Month::August)    // Asarnha Bucha Day
                || (d == 23 && m == Month::October)  // Chulalongkorn Day
                || (d == 29 && m == Month::December))
        // Substitution for New Year's Eve (Sunday 31st December 2023) (added)
        {
            return false;
        }

        if (y == 2024)
            && ((d == 26 && m == Month::February)    // Substitution for Makha Bucha Day (Saturday 24th February 2024)
                || (d == 8 && m == Month::April)     // Substitution for Chakri Memorial Day (Saturday 6th April 2024)
                || (d == 12 && m == Month::April)    // Additional holiday in relation to the Songkran festival
                || (d == 6 && m == Month::May)       // Substitution for Coronation Day (Saturday 4th May 2024)
                || (d == 22 && m == Month::May)      // Wisakha Bucha Day
                || (d == 22 && m == Month::July)     // Substitution for Asarnha Bucha Day (Saturday 20th July 2024)
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        if (y == 2025)
            && ((d == 12 && m == Month::February)    // Substitution for Makha Bucha Day (Wednesday 12th February 2025)
                || (d == 7 && m == Month::April)     // Substitution for Chakri Memorial Day (Sunday 6th April 2025)
                || (d == 5 && m == Month::May)       // Substitution for Coronation Day (Sunday 4th May 2025)
                || (d == 12 && m == Month::May)      // Wisakha Bucha Day
                || (d == 10 && m == Month::July)     // Substitution for Asarnha Bucha Day (Tuesday 20th July 2025)
                || (d == 23 && m == Month::October))
        // Chulalongkorn Day
        {
            return false;
        }

        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against known Thai holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Thailand::new().name(), "Thailand stock exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Thailand::new();
        // Chakri Memorial Day, April 6th
        assert!(c.is_holiday(Date::new(6, Month::April, 2019)));
        // Songkran Festival, April 13th-15th (not 2020)
        assert!(c.is_holiday(Date::new(13, Month::April, 2019)));
        assert!(c.is_holiday(Date::new(14, Month::April, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::April, 2019)));
        // H.M. Queen Sirikit's Birthday / Mother's Day, August 12th
        assert!(c.is_holiday(Date::new(12, Month::August, 2019)));
        // Constitution Day, December 10th
        assert!(c.is_holiday(Date::new(10, Month::December, 2019)));
        // New Year's Eve, December 31st
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Thailand::new();
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = Thailand::new();
        // Chakri Memorial Day at the horizon year does not panic.
        assert!(c.is_holiday(Date::new(6, Month::April, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = Thailand::new();
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
