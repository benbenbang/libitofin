//! Ukrainian calendars.
//!
//! Port of `ql/time/calendars/ukraine.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, orthodox_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Market handled by the Ukrainian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Market {
    /// Ukrainian stock exchange.
    USE,
}

/// The Ukrainian calendar. The default market is [`Market::USE`].
pub struct Ukraine;

impl Ukraine {
    /// Builds a Ukrainian calendar for the given `market`.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::USE => shared(UseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct UseImpl;

impl CalendarImpl for UseImpl {
    fn name(&self) -> String {
        "Ukrainian stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let dd = date.day_of_year();
        let m = date.month();
        let y = date.year();
        let em = orthodox_easter_monday(y);
        !(is_weekend_sat_sun(w)
            // New Year's Day (possibly moved to Monday)
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday))
                && m == Month::January)
            // Orthodox Christmas
            || ((d == 7 || ((d == 8 || d == 9) && w == Weekday::Monday))
                && m == Month::January)
            // Women's Day
            || ((d == 8 || ((d == 9 || d == 10) && w == Weekday::Monday))
                && m == Month::March)
            // Orthodox Easter Monday
            || (dd == em)
            // Holy Trinity Day
            || (dd == em + 49)
            // Workers' Solidarity Days
            || ((d == 1 || d == 2 || (d == 3 && w == Weekday::Monday)) && m == Month::May)
            // Victory Day
            || ((d == 9 || ((d == 10 || d == 11) && w == Weekday::Monday)) && m == Month::May)
            // Constitution Day
            || (d == 28 && m == Month::June)
            // Independence Day
            || (d == 24 && m == Month::August)
            // Defender's Day (since 2015)
            || (d == 14 && m == Month::October && y >= 2015))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Ukraine::new(Market::USE).name(), "Ukrainian stock exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Ukraine::new(Market::USE);
        for (d, m) in [
            (28, Month::June),   // Constitution Day
            (24, Month::August), // Independence Day
        ] {
            assert!(c.is_holiday(Date::new(d, m, 2019)), "{d} {m} 2019");
        }
    }

    #[test]
    fn weekend_rule() {
        let c = Ukraine::new(Market::USE);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Friday));
    }
}
