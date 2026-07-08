//! Slovenia calendars.
//!
//! Port of `ql/time/calendars/slovenia.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Slovenian markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Ljubljana stock exchange.
    Lse,
}

/// Slovenian calendars. Defaults to [`Market::Lse`] in QuantLib.
pub struct Slovenia;

impl Slovenia {
    /// Builds a Slovenian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Lse => shared(LseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct LseImpl;

impl CalendarImpl for LseImpl {
    fn name(&self) -> String {
        "Ljubljana stock exchange".to_string()
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
            // New Year's Holiday
            || (d == 2 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // May Day
            || (d == 1 && m == Month::May)
            // May Day Holiday
            || (d == 2 && m == Month::May)
            // Statehood Day
            || (d == 25 && m == Month::June)
            // Assumption of Mary
            || (d == 15 && m == Month::August)
            // Reformation Day
            || (d == 31 && m == Month::October)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen
            || (d == 26 && m == Month::December)
            // New Year's Eve
            || (d == 31 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            Slovenia::new(Market::Lse).name(),
            "Ljubljana stock exchange"
        );
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Slovenia::new(Market::Lse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(2, Month::January, 2019))); // New Year's Holiday
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // May Day
        assert!(c.is_holiday(Date::new(2, Month::May, 2019))); // May Day Holiday
        assert!(c.is_holiday(Date::new(25, Month::June, 2019))); // Statehood Day
        assert!(c.is_holiday(Date::new(15, Month::August, 2019))); // Assumption of Mary
        assert!(c.is_holiday(Date::new(31, Month::October, 2019))); // Reformation Day
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // St. Stephen
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // New Year's Eve
    }

    #[test]
    fn weekend_rule() {
        let c = Slovenia::new(Market::Lse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
