//! Montenegro calendar.
//!
//! Port of `ql/time/calendars/montenegro.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Montenegrin markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Montenegro Stock Exchange.
    Mnse,
}

/// Montenegrin calendars. Defaults to [`Market::Mnse`] in QuantLib.
pub struct Montenegro;

impl Montenegro {
    /// Builds a Montenegrin calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Mnse => shared(MnseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct MnseImpl;

impl CalendarImpl for MnseImpl {
    fn name(&self) -> String {
        "Montenegro Stock Exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();

        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // New Year Holiday
            || (d == 2 && m == Month::January)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Labour Day Holiday
            || (d == 2 && m == Month::May)
            // Independence Day
            || (d == 21 && m == Month::May)
            // Independence Day Holiday
            || (d == 22 && m == Month::May)
            // Statehood Day
            || (d == 13 && m == Month::July)
            // Statehood Day Holiday
            || (d == 14 && m == Month::July)
            // Njegos Day
            || (d == 13 && m == Month::November)
            // Njegos Day Holiday
            || (d == 14 && m == Month::November))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            Montenegro::new(Market::Mnse).name(),
            "Montenegro Stock Exchange"
        );
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Montenegro::new(Market::Mnse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(2, Month::January, 2019))); // New Year Holiday
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(2, Month::May, 2019))); // Labour Day Holiday
        assert!(c.is_holiday(Date::new(21, Month::May, 2019))); // Independence Day
        assert!(c.is_holiday(Date::new(22, Month::May, 2019))); // Independence Day Holiday
        assert!(c.is_holiday(Date::new(13, Month::July, 2019))); // Statehood Day
        assert!(c.is_holiday(Date::new(14, Month::July, 2019))); // Statehood Day Holiday
        assert!(c.is_holiday(Date::new(13, Month::November, 2019))); // Njegos Day
        assert!(c.is_holiday(Date::new(14, Month::November, 2019))); // Njegos Day Holiday
    }

    #[test]
    fn weekend_rule() {
        let c = Montenegro::new(Market::Mnse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
