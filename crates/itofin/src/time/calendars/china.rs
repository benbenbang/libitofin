//! Chinese calendar.
//!
//! Port of `ql/time/calendars/china.{hpp,cpp}`.

// Range clauses are kept in the verbatim C++ `d >= a && d <= b` form.
#![allow(clippy::manual_range_contains)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which China's public/lunar holidays are tabulated
/// (matching QuantLib's data). Queries beyond this year cannot be answered
/// reliably and panic rather than silently omitting holidays.
const HOLIDAY_HORIZON: Year = 2026;

/// Chinese markets.
///
/// QuantLib defaults to [`Market::Sse`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Shanghai stock exchange.
    Sse,
    /// Interbank calendar.
    Ib,
}

/// The Chinese calendar.
///
/// # Accuracy
///
/// China's public/lunar holidays are tabulated (from QuantLib) only through
/// 2026. Querying a date after 2026 panics rather than silently returning an
/// unreliable business-day result.
pub struct China;

impl China {
    /// Builds a Chinese calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Sse => shared(SseImpl),
            Market::Ib => shared(IbImpl),
        };
        Calendar::from_impl(imp)
    }
}

/// Chinese weekend rule (Saturday and Sunday).
fn is_weekend(w: Weekday) -> bool {
    w == Weekday::Saturday || w == Weekday::Sunday
}

/// SSE business-day rule, reused by the interbank calendar.
fn sse_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let m = date.month();
    let y = date.year();

    assert!(
        y <= HOLIDAY_HORIZON,
        "China public/lunar holidays are tabulated only through {HOLIDAY_HORIZON} \
         (matching QuantLib); year {y} is beyond the supported horizon"
    );

    !(is_weekend(w)
        // New Year's Day
        || (d == 1 && m == Month::January)
        || (y == 2005 && d == 3 && m == Month::January)
        || (y == 2006 && (d == 2 || d == 3) && m == Month::January)
        || (y == 2007 && d <= 3 && m == Month::January)
        || (y == 2007 && d == 31 && m == Month::December)
        || (y == 2009 && d == 2 && m == Month::January)
        || (y == 2011 && d == 3 && m == Month::January)
        || (y == 2012 && (d == 2 || d == 3) && m == Month::January)
        || (y == 2013 && d <= 3 && m == Month::January)
        || (y == 2014 && d == 1 && m == Month::January)
        || (y == 2015 && d <= 3 && m == Month::January)
        || (y == 2017 && d == 2 && m == Month::January)
        || (y == 2018 && d == 1 && m == Month::January)
        || (y == 2018 && d == 31 && m == Month::December)
        || (y == 2019 && d == 1 && m == Month::January)
        || (y == 2020 && d == 1 && m == Month::January)
        || (y == 2021 && d == 1 && m == Month::January)
        || (y == 2022 && d == 3 && m == Month::January)
        || (y == 2023 && d == 2 && m == Month::January)
        || (y == 2026 && (d == 1 || d == 2) && m == Month::January)
        // Chinese New Year
        || (y == 2004 && d >= 19 && d <= 28 && m == Month::January)
        || (y == 2005 && d >= 7 && d <= 15 && m == Month::February)
        || (y == 2006 && ((d >= 26 && m == Month::January) || (d <= 3 && m == Month::February)))
        || (y == 2007 && d >= 17 && d <= 25 && m == Month::February)
        || (y == 2008 && d >= 6 && d <= 12 && m == Month::February)
        || (y == 2009 && d >= 26 && d <= 30 && m == Month::January)
        || (y == 2010 && d >= 15 && d <= 19 && m == Month::February)
        || (y == 2011 && d >= 2 && d <= 8 && m == Month::February)
        || (y == 2012 && d >= 23 && d <= 28 && m == Month::January)
        || (y == 2013 && d >= 11 && d <= 15 && m == Month::February)
        || (y == 2014 && d >= 31 && m == Month::January)
        || (y == 2014 && d <= 6 && m == Month::February)
        || (y == 2015 && d >= 18 && d <= 24 && m == Month::February)
        || (y == 2016 && d >= 8 && d <= 12 && m == Month::February)
        || (y == 2017 && ((d >= 27 && m == Month::January) || (d <= 2 && m == Month::February)))
        || (y == 2018 && (d >= 15 && d <= 21 && m == Month::February))
        || (y == 2019 && d >= 4 && d <= 8 && m == Month::February)
        || (y == 2020 && (d == 24 || (d >= 27 && d <= 31)) && m == Month::January)
        || (y == 2021
            && (d == 11 || d == 12 || d == 15 || d == 16 || d == 17)
            && m == Month::February)
        || (y == 2022 && ((d == 31 && m == Month::January) || (d <= 4 && m == Month::February)))
        || (y == 2023 && d >= 23 && d <= 27 && m == Month::January)
        || (y == 2024 && (d == 9 || (d >= 12 && d <= 16)) && m == Month::February)
        || (y == 2025
            && ((d >= 28 && d <= 31 && m == Month::January)
                || (d >= 3 && d <= 4 && m == Month::February)))
        || (y == 2026 && ((d >= 16 && d <= 20) || d == 23) && m == Month::February)
        // Ching Ming Festival
        || (y <= 2008 && d == 4 && m == Month::April)
        || (y == 2009 && d == 6 && m == Month::April)
        || (y == 2010 && d == 5 && m == Month::April)
        || (y == 2011 && d >= 3 && d <= 5 && m == Month::April)
        || (y == 2012 && d >= 2 && d <= 4 && m == Month::April)
        || (y == 2013 && d >= 4 && d <= 5 && m == Month::April)
        || (y == 2014 && d == 7 && m == Month::April)
        || (y == 2015 && d >= 5 && d <= 6 && m == Month::April)
        || (y == 2016 && d == 4 && m == Month::April)
        || (y == 2017 && d >= 3 && d <= 4 && m == Month::April)
        || (y == 2018 && d >= 5 && d <= 6 && m == Month::April)
        || (y == 2019 && d == 5 && m == Month::April)
        || (y == 2020 && d == 6 && m == Month::April)
        || (y == 2021 && d == 5 && m == Month::April)
        || (y == 2022 && d >= 4 && d <= 5 && m == Month::April)
        || (y == 2023 && d == 5 && m == Month::April)
        || (y == 2024 && d >= 4 && d <= 5 && m == Month::April)
        || (y == 2025 && d == 4 && m == Month::April)
        || (y == 2026 && d == 6 && m == Month::April)
        // Labor Day
        || (y <= 2007 && d >= 1 && d <= 7 && m == Month::May)
        || (y == 2008 && d >= 1 && d <= 2 && m == Month::May)
        || (y == 2009 && d == 1 && m == Month::May)
        || (y == 2010 && d == 3 && m == Month::May)
        || (y == 2011 && d == 2 && m == Month::May)
        || (y == 2012 && ((d == 30 && m == Month::April) || (d == 1 && m == Month::May)))
        || (y == 2013 && ((d >= 29 && m == Month::April) || (d == 1 && m == Month::May)))
        || (y == 2014 && d >= 1 && d <= 3 && m == Month::May)
        || (y == 2015 && d == 1 && m == Month::May)
        || (y == 2016 && d >= 1 && d <= 2 && m == Month::May)
        || (y == 2017 && d == 1 && m == Month::May)
        || (y == 2018 && ((d == 30 && m == Month::April) || (d == 1 && m == Month::May)))
        || (y == 2019 && d >= 1 && d <= 3 && m == Month::May)
        || (y == 2020 && (d == 1 || d == 4 || d == 5) && m == Month::May)
        || (y == 2021 && (d == 3 || d == 4 || d == 5) && m == Month::May)
        || (y == 2022 && d >= 2 && d <= 4 && m == Month::May)
        || (y == 2023 && d >= 1 && d <= 3 && m == Month::May)
        || (y == 2024 && d >= 1 && d <= 3 && m == Month::May)
        || (y == 2025 && (d == 1 || d == 2 || d == 5) && m == Month::May)
        || (y == 2026 && (d == 1 || d == 4 || d == 5) && m == Month::May)
        // Tuen Ng Festival
        || (y <= 2008 && d == 9 && m == Month::June)
        || (y == 2009 && (d == 28 || d == 29) && m == Month::May)
        || (y == 2010 && d >= 14 && d <= 16 && m == Month::June)
        || (y == 2011 && d >= 4 && d <= 6 && m == Month::June)
        || (y == 2012 && d >= 22 && d <= 24 && m == Month::June)
        || (y == 2013 && d >= 10 && d <= 12 && m == Month::June)
        || (y == 2014 && d == 2 && m == Month::June)
        || (y == 2015 && d == 22 && m == Month::June)
        || (y == 2016 && d >= 9 && d <= 10 && m == Month::June)
        || (y == 2017 && d >= 29 && d <= 30 && m == Month::May)
        || (y == 2018 && d == 18 && m == Month::June)
        || (y == 2019 && d == 7 && m == Month::June)
        || (y == 2020 && d >= 25 && d <= 26 && m == Month::June)
        || (y == 2021 && d == 14 && m == Month::June)
        || (y == 2022 && d == 3 && m == Month::June)
        || (y == 2023 && d >= 22 && d <= 23 && m == Month::June)
        || (y == 2024 && d == 10 && m == Month::June)
        || (y == 2025 && d == 2 && m == Month::June)
        || (y == 2026 && d == 19 && m == Month::June)
        // Mid-Autumn Festival
        || (y <= 2008 && d == 15 && m == Month::September)
        || (y == 2010 && d >= 22 && d <= 24 && m == Month::September)
        || (y == 2011 && d >= 10 && d <= 12 && m == Month::September)
        || (y == 2012 && d == 30 && m == Month::September)
        || (y == 2013 && d >= 19 && d <= 20 && m == Month::September)
        || (y == 2014 && d == 8 && m == Month::September)
        || (y == 2015 && d == 27 && m == Month::September)
        || (y == 2016 && d >= 15 && d <= 16 && m == Month::September)
        || (y == 2018 && d == 24 && m == Month::September)
        || (y == 2019 && d == 13 && m == Month::September)
        || (y == 2021 && (d == 20 || d == 21) && m == Month::September)
        || (y == 2022 && d == 12 && m == Month::September)
        || (y == 2023 && d == 29 && m == Month::September)
        || (y == 2024 && d >= 16 && d <= 17 && m == Month::September)
        || (y == 2026 && d == 25 && m == Month::September)
        // National Day
        || (y <= 2007 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2008 && ((d >= 29 && m == Month::September) || (d <= 3 && m == Month::October)))
        || (y == 2009 && d >= 1 && d <= 8 && m == Month::October)
        || (y == 2010 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2011 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2012 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2013 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2014 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2015 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2016 && d >= 3 && d <= 7 && m == Month::October)
        || (y == 2017 && d >= 2 && d <= 6 && m == Month::October)
        || (y == 2018 && d >= 1 && d <= 5 && m == Month::October)
        || (y == 2019 && d >= 1 && d <= 7 && m == Month::October)
        || (y == 2020 && d >= 1 && d <= 2 && m == Month::October)
        || (y == 2020 && d >= 5 && d <= 8 && m == Month::October)
        || (y == 2021
            && (d == 1 || d == 4 || d == 5 || d == 6 || d == 7)
            && m == Month::October)
        || (y == 2022 && d >= 3 && d <= 7 && m == Month::October)
        || (y == 2023 && d >= 2 && d <= 6 && m == Month::October)
        || (y == 2024 && ((d >= 1 && d <= 4) || d == 7) && m == Month::October)
        || (y == 2025 && ((d >= 1 && d <= 3) || (d >= 6 && d <= 8)) && m == Month::October)
        || (y == 2026 && ((d >= 1 && d <= 2) || (d >= 5 && d <= 7)) && m == Month::October)
        // 70th anniversary of the victory of anti-Japaneses war
        || (y == 2015 && d >= 3 && d <= 4 && m == Month::September))
}

struct SseImpl;

impl CalendarImpl for SseImpl {
    fn name(&self) -> String {
        "Shanghai stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        sse_is_business_day(date)
    }
}

struct IbImpl;

impl CalendarImpl for IbImpl {
    fn name(&self) -> String {
        "China inter bank market".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let working_weekends = [
            // 2005
            Date::new(5, Month::February, 2005),
            Date::new(6, Month::February, 2005),
            Date::new(30, Month::April, 2005),
            Date::new(8, Month::May, 2005),
            Date::new(8, Month::October, 2005),
            Date::new(9, Month::October, 2005),
            Date::new(31, Month::December, 2005),
            //2006
            Date::new(28, Month::January, 2006),
            Date::new(29, Month::April, 2006),
            Date::new(30, Month::April, 2006),
            Date::new(30, Month::September, 2006),
            Date::new(30, Month::December, 2006),
            Date::new(31, Month::December, 2006),
            // 2007
            Date::new(17, Month::February, 2007),
            Date::new(25, Month::February, 2007),
            Date::new(28, Month::April, 2007),
            Date::new(29, Month::April, 2007),
            Date::new(29, Month::September, 2007),
            Date::new(30, Month::September, 2007),
            Date::new(29, Month::December, 2007),
            // 2008
            Date::new(2, Month::February, 2008),
            Date::new(3, Month::February, 2008),
            Date::new(4, Month::May, 2008),
            Date::new(27, Month::September, 2008),
            Date::new(28, Month::September, 2008),
            // 2009
            Date::new(4, Month::January, 2009),
            Date::new(24, Month::January, 2009),
            Date::new(1, Month::February, 2009),
            Date::new(31, Month::May, 2009),
            Date::new(27, Month::September, 2009),
            Date::new(10, Month::October, 2009),
            // 2010
            Date::new(20, Month::February, 2010),
            Date::new(21, Month::February, 2010),
            Date::new(12, Month::June, 2010),
            Date::new(13, Month::June, 2010),
            Date::new(19, Month::September, 2010),
            Date::new(25, Month::September, 2010),
            Date::new(26, Month::September, 2010),
            Date::new(9, Month::October, 2010),
            // 2011
            Date::new(30, Month::January, 2011),
            Date::new(12, Month::February, 2011),
            Date::new(2, Month::April, 2011),
            Date::new(8, Month::October, 2011),
            Date::new(9, Month::October, 2011),
            Date::new(31, Month::December, 2011),
            // 2012
            Date::new(21, Month::January, 2012),
            Date::new(29, Month::January, 2012),
            Date::new(31, Month::March, 2012),
            Date::new(1, Month::April, 2012),
            Date::new(28, Month::April, 2012),
            Date::new(29, Month::September, 2012),
            // 2013
            Date::new(5, Month::January, 2013),
            Date::new(6, Month::January, 2013),
            Date::new(16, Month::February, 2013),
            Date::new(17, Month::February, 2013),
            Date::new(7, Month::April, 2013),
            Date::new(27, Month::April, 2013),
            Date::new(28, Month::April, 2013),
            Date::new(8, Month::June, 2013),
            Date::new(9, Month::June, 2013),
            Date::new(22, Month::September, 2013),
            Date::new(29, Month::September, 2013),
            Date::new(12, Month::October, 2013),
            // 2014
            Date::new(26, Month::January, 2014),
            Date::new(8, Month::February, 2014),
            Date::new(4, Month::May, 2014),
            Date::new(28, Month::September, 2014),
            Date::new(11, Month::October, 2014),
            // 2015
            Date::new(4, Month::January, 2015),
            Date::new(15, Month::February, 2015),
            Date::new(28, Month::February, 2015),
            Date::new(6, Month::September, 2015),
            Date::new(10, Month::October, 2015),
            // 2016
            Date::new(6, Month::February, 2016),
            Date::new(14, Month::February, 2016),
            Date::new(12, Month::June, 2016),
            Date::new(18, Month::September, 2016),
            Date::new(8, Month::October, 2016),
            Date::new(9, Month::October, 2016),
            // 2017
            Date::new(22, Month::January, 2017),
            Date::new(4, Month::February, 2017),
            Date::new(1, Month::April, 2017),
            Date::new(27, Month::May, 2017),
            Date::new(30, Month::September, 2017),
            // 2018
            Date::new(11, Month::February, 2018),
            Date::new(24, Month::February, 2018),
            Date::new(8, Month::April, 2018),
            Date::new(28, Month::April, 2018),
            Date::new(29, Month::September, 2018),
            Date::new(30, Month::September, 2018),
            Date::new(29, Month::December, 2018),
            // 2019
            Date::new(2, Month::February, 2019),
            Date::new(3, Month::February, 2019),
            Date::new(28, Month::April, 2019),
            Date::new(5, Month::May, 2019),
            Date::new(29, Month::September, 2019),
            Date::new(12, Month::October, 2019),
            // 2020
            Date::new(19, Month::January, 2020),
            Date::new(26, Month::April, 2020),
            Date::new(9, Month::May, 2020),
            Date::new(28, Month::June, 2020),
            Date::new(27, Month::September, 2020),
            Date::new(10, Month::October, 2020),
            // 2021
            Date::new(7, Month::February, 2021),
            Date::new(20, Month::February, 2021),
            Date::new(25, Month::April, 2021),
            Date::new(8, Month::May, 2021),
            Date::new(18, Month::September, 2021),
            Date::new(26, Month::September, 2021),
            Date::new(9, Month::October, 2021),
            // 2022
            Date::new(29, Month::January, 2022),
            Date::new(30, Month::January, 2022),
            Date::new(2, Month::April, 2022),
            Date::new(24, Month::April, 2022),
            Date::new(7, Month::May, 2022),
            Date::new(8, Month::October, 2022),
            Date::new(9, Month::October, 2022),
            // 2023
            Date::new(28, Month::January, 2023),
            Date::new(29, Month::January, 2023),
            Date::new(23, Month::April, 2023),
            Date::new(6, Month::May, 2023),
            Date::new(25, Month::June, 2023),
            Date::new(7, Month::October, 2023),
            Date::new(8, Month::October, 2023),
            // 2024
            Date::new(4, Month::February, 2024),
            Date::new(9, Month::February, 2024),
            Date::new(18, Month::February, 2024),
            Date::new(7, Month::April, 2024),
            Date::new(28, Month::April, 2024),
            Date::new(11, Month::May, 2024),
            Date::new(14, Month::September, 2024),
            Date::new(29, Month::September, 2024),
            Date::new(12, Month::October, 2024),
            // 2025
            Date::new(26, Month::January, 2025),
            Date::new(8, Month::February, 2025),
            Date::new(27, Month::April, 2025),
            Date::new(28, Month::September, 2025),
            Date::new(11, Month::October, 2025),
            // 2026
            Date::new(4, Month::January, 2026),
            Date::new(14, Month::February, 2026),
            Date::new(28, Month::February, 2026),
            Date::new(9, Month::May, 2026),
            Date::new(20, Month::September, 2026),
            Date::new(10, Month::October, 2026),
        ];

        // If it is already a SSE business day, it must be a IB business day
        sse_is_business_day(date) || working_weekends.contains(&date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(China::new(Market::Sse).name(), "Shanghai stock exchange");
        assert_eq!(China::new(Market::Ib).name(), "China inter bank market");
    }

    #[test]
    fn unconditional_holidays() {
        let c = China::new(Market::Sse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
    }

    #[test]
    fn ib_working_weekend_is_business_day() {
        // 5 February 2005 is a Saturday but an IB working weekend.
        let ib = China::new(Market::Ib);
        let sse = China::new(Market::Sse);
        let date = Date::new(5, Month::February, 2005);
        assert!(ib.is_business_day(date));
        assert!(sse.is_holiday(date));
    }

    #[test]
    fn weekend_rule() {
        let c = China::new(Market::Sse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }

    #[test]
    fn in_horizon_query_works() {
        let c = China::new(Market::Sse);
        assert!(c.is_holiday(Date::new(1, Month::January, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = China::new(Market::Sse);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics_ib() {
        let ib = China::new(Market::Ib);
        let _ = ib.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
