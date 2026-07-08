//! Argentinian calendars.
//!
//! Port of `ql/time/calendars/argentina.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Argentinian calendar markets.
///
/// QuantLib defaults this to [`Market::Merval`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Buenos Aires stock exchange calendar.
    Merval,
}

/// The Argentinian calendar.
pub struct Argentina;

impl Argentina {
    /// Builds an Argentinian calendar for the given market. QuantLib defaults
    /// to [`Market::Merval`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Merval => shared(MervalImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct MervalImpl;

impl CalendarImpl for MervalImpl {
    fn name(&self) -> String {
        "Buenos Aires stock exchange".to_string()
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
        let em = western_easter_monday(y);
        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Holy Thursday
            || (dd == em - 4)
            // Good Friday
            || (dd == em - 3)
            // Labour Day
            || (d == 1 && m == Month::May)
            // May Revolution
            || (d == 25 && m == Month::May)
            // Death of General Manuel Belgrano
            || ((15..=21).contains(&d) && w == Weekday::Monday && m == Month::June)
            // Independence Day
            || (d == 9 && m == Month::July)
            // Death of General José de San Martín
            || ((15..=21).contains(&d) && w == Weekday::Monday && m == Month::August)
            // Columbus Day
            || ((d == 10 || d == 11 || d == 12 || d == 15 || d == 16)
                && w == Weekday::Monday
                && m == Month::October)
            // Immaculate Conception
            || (d == 8 && m == Month::December)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // New Year's Eve
            || ((d == 31 || (d == 30 && w == Weekday::Friday)) && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against fixed Argentinian holidays; not a full transcription
    // of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            Argentina::new(Market::Merval).name(),
            "Buenos Aires stock exchange"
        );
    }

    #[test]
    fn fixed_holidays() {
        let c = Argentina::new(Market::Merval);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(9, Month::July, 2019)));
        assert!(c.is_holiday(Date::new(8, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(24, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Argentina::new(Market::Merval);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
