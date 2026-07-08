//! Czech calendars.
//!
//! Port of `ql/time/calendars/czechrepublic.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Czech markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Prague stock exchange.
    Pse,
}

/// Czech calendars. Defaults to [`Market::Pse`] in QuantLib.
pub struct CzechRepublic;

impl CzechRepublic {
    /// Builds a Czech calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Pse => shared(PseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct PseImpl;

impl CalendarImpl for PseImpl {
    fn name(&self) -> String {
        "Prague stock exchange".to_string()
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
            // Good Friday
            || (dd == em - 3 && y >= 2016)
            // Easter Monday
            || (dd == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Liberation Day
            || (d == 8 && m == Month::May)
            // SS. Cyril and Methodius
            || (d == 5 && m == Month::July)
            // Jan Hus Day
            || (d == 6 && m == Month::July)
            // Czech Statehood Day
            || (d == 28 && m == Month::September)
            // Independence Day
            || (d == 28 && m == Month::October)
            // Struggle for Freedom and Democracy Day
            || (d == 17 && m == Month::November)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen
            || (d == 26 && m == Month::December)
            // unidentified closing days for stock exchange
            || (d == 2 && m == Month::January && y == 2004)
            || (d == 31 && m == Month::December && y == 2004))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            CzechRepublic::new(Market::Pse).name(),
            "Prague stock exchange"
        );
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = CzechRepublic::new(Market::Pse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(8, Month::May, 2019))); // Liberation Day
        assert!(c.is_holiday(Date::new(5, Month::July, 2019))); // SS. Cyril and Methodius
        assert!(c.is_holiday(Date::new(6, Month::July, 2019))); // Jan Hus Day
        assert!(c.is_holiday(Date::new(28, Month::September, 2019))); // Czech Statehood Day
        assert!(c.is_holiday(Date::new(28, Month::October, 2019))); // Independence Day
        assert!(c.is_holiday(Date::new(17, Month::November, 2019))); // Struggle for Freedom
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // St. Stephen
    }

    #[test]
    fn weekend_rule() {
        let c = CzechRepublic::new(Market::Pse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
