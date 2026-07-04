//! Saudi Arabian calendar.
//!
//! Port of `ql/time/calendars/saudiarabia.{hpp,cpp}`.
//!
//! The Tadawul weekend rule changed on 29th June 2013 (Thursday/Friday before,
//! Friday/Saturday afterwards). `is_weekend` mirrors QuantLib's `isWeekend`
//! override (Friday/Saturday), while `is_business_day` uses a date-dependent
//! `is_true_weekend` helper.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl};
use crate::time::date::{Date, Month, Year};
use crate::time::weekday::Weekday;

/// Last year for which *all* of Saudi Arabia's Islamic holiday tables are
/// complete (matching QuantLib's data). This is the MINIMUM across the required
/// moving-holiday tables, not the maximum: QuantLib's Eid al-Fitr table runs to
/// 2029 but its Eid al-Adha table stops at 2022, so from 2023 onward Eid al-Adha
/// would be silently omitted. Queries beyond this year cannot be answered
/// reliably and panic instead.
const HOLIDAY_HORIZON: Year = 2022;

/// Market handled by the Saudi Arabian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Tadawul financial market.
    Tadawul,
}

/// The Saudi Arabian calendar. The default market is [`Market::Tadawul`].
///
/// # Accuracy
///
/// Saudi Arabia's Islamic (Eid) holidays are tabulated (from QuantLib) only
/// through 2022 - the Eid al-Adha table ends there, even though Eid al-Fitr
/// extends to 2029. Querying a date after 2022 panics rather than silently
/// omitting Eid al-Adha and returning an unreliable business-day result.
pub struct SaudiArabia;

impl SaudiArabia {
    /// Builds a Saudi Arabian calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Tadawul => shared(TadawulImpl),
        };
        Calendar::from_impl(imp)
    }
}

/// The Saudi weekend rule, which changed from 29th June 2013.
fn is_true_weekend(d: Date) -> bool {
    let w = d.weekday();
    if d < Date::new(29, Month::June, 2013) {
        w == Weekday::Thursday || w == Weekday::Friday
    } else {
        w == Weekday::Friday || w == Weekday::Saturday
    }
}

// In 2015 and 2014, the Eid holidays of the Tadawul Exchange
// have been from Eid-1 to Eid+4.
// Sometimes, slightly longer holidays are observed.
// But conservatively, we take Eid-1 to Eid+4 as the holiday.

/// Whether `d` falls within a Tadawul Eid al-Adha holiday window (Eid-1 to Eid+4).
fn is_eid_al_adha(d: Date) -> bool {
    // Eid al Adha dates taken from:
    // https://en.wikipedia.org/wiki/Eid_al-Adha#Eid_al-Adha_in_the_Gregorian_calendar
    const EID_AL_ADHA: &[(i32, Month, i32)] = &[
        (7, Month::April, 1998),
        (27, Month::March, 1999),
        (16, Month::March, 2000),
        (5, Month::March, 2001),
        (23, Month::February, 2002),
        (12, Month::February, 2003),
        (1, Month::February, 2004),
        (21, Month::January, 2005),
        (10, Month::January, 2006),
        (31, Month::December, 2006),
        (20, Month::December, 2007),
        (8, Month::December, 2008),
        (27, Month::November, 2009),
        (16, Month::November, 2010),
        (6, Month::November, 2011),
        (26, Month::October, 2012),
        (15, Month::October, 2013),
        (4, Month::October, 2014),
        (24, Month::September, 2015),
        (11, Month::September, 2016),
        (1, Month::September, 2017),
        (23, Month::August, 2018),
        (12, Month::August, 2019),
        (31, Month::July, 2020),
        (20, Month::July, 2021),
        (10, Month::July, 2022),
    ];
    EID_AL_ADHA.iter().any(|&(day, m, y)| {
        let p = Date::new(day, m, y);
        d >= p - 1 && d <= p + 4
    })
}

/// Whether `d` falls within a Tadawul Eid al-Fitr holiday window (Eid-1 to Eid+4).
fn is_eid_al_fitr(d: Date) -> bool {
    // Eid al Fitr dates taken from:
    // https://en.wikipedia.org/wiki/Eid_al-Fitr#In_the_Gregorian_calendar
    const EID_AL_FITR: &[(i32, Month, i32)] = &[
        (16, Month::December, 2001),
        (5, Month::December, 2002),
        (25, Month::November, 2003),
        (13, Month::November, 2004),
        (3, Month::November, 2005),
        (23, Month::October, 2006),
        (12, Month::October, 2007),
        (30, Month::September, 2008),
        (20, Month::September, 2009),
        (10, Month::September, 2010),
        (30, Month::August, 2011),
        (19, Month::August, 2012),
        (8, Month::August, 2013),
        (28, Month::July, 2014),
        (17, Month::July, 2015),
        (6, Month::July, 2016),
        (25, Month::June, 2017),
        (15, Month::June, 2018),
        (4, Month::June, 2019),
        (24, Month::May, 2020),
        (13, Month::May, 2021),
        (2, Month::May, 2022),
        (21, Month::April, 2023),
        (10, Month::April, 2024),
        (30, Month::March, 2025),
        (20, Month::March, 2026),
        (9, Month::March, 2027),
        (26, Month::February, 2028),
        (14, Month::February, 2029),
    ];
    EID_AL_FITR.iter().any(|&(day, m, y)| {
        let p = Date::new(day, m, y);
        d >= p - 1 && d <= p + 4
    })
}

struct TadawulImpl;

impl CalendarImpl for TadawulImpl {
    fn name(&self) -> String {
        "Tadawul".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        w == Weekday::Friday || w == Weekday::Saturday
    }

    fn is_business_day(&self, date: Date) -> bool {
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();

        assert!(
            y <= HOLIDAY_HORIZON,
            "Saudi Arabia Islamic (Eid) holidays are tabulated only through {HOLIDAY_HORIZON} \
             (matching QuantLib); year {y} is beyond the supported horizon"
        );

        !(is_true_weekend(date)
            || is_eid_al_adha(date)
            || is_eid_al_fitr(date)
            // National Day
            || (d == 23 && m == Month::September)
            // other one-shot holidays
            || (d == 26 && m == Month::February && y == 2011)
            || (d == 19 && m == Month::March && y == 2011))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(SaudiArabia::new(Market::Tadawul).name(), "Tadawul");
    }

    #[test]
    fn national_day_is_holiday() {
        let c = SaudiArabia::new(Market::Tadawul);
        // National Day, September 23rd, unconditional.
        assert!(c.is_holiday(Date::new(23, Month::September, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = SaudiArabia::new(Market::Tadawul);
        // isWeekend override is Friday/Saturday.
        assert!(c.is_weekend(Weekday::Friday));
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(!c.is_weekend(Weekday::Sunday));
    }

    #[test]
    fn in_horizon_query_works() {
        // A query at the last tabulated year must not panic. National Day
        // (September 23rd) is unconditional.
        let c = SaudiArabia::new(Market::Tadawul);
        assert!(c.is_holiday(Date::new(23, Month::September, HOLIDAY_HORIZON)));
    }

    #[test]
    #[should_panic(expected = "beyond the supported horizon")]
    fn beyond_horizon_panics() {
        let c = SaudiArabia::new(Market::Tadawul);
        let _ = c.is_business_day(Date::new(1, Month::January, HOLIDAY_HORIZON + 1));
    }
}
