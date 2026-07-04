//! Croatia calendars.
//!
//! Port of `ql/time/calendars/croatia.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Croatian markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Zagreb stock exchange.
    Zse,
}

/// Croatian calendars. Defaults to [`Market::Zse`] in QuantLib.
pub struct Croatia;

impl Croatia {
    /// Builds a Croatian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Zse => shared(ZseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct ZseImpl;

impl CalendarImpl for ZseImpl {
    fn name(&self) -> String {
        "Zagreb stock exchange".to_string()
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
        // Croatian holidays
        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Epiphany
            || (d == 6 && m == Month::January)
            // Good Friday
            || (dd == em - 3 && y >= 2016)
            // Easter Monday
            || (dd == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // National Day
            || (d == 30 && m == Month::May)
            // Corpus Christi
            || (dd == em + 59)
            // Anti-Fascist Struggle Day
            || (d == 22 && m == Month::June)
            // Victory and Homeland Thanksgiving Day and the Day of Croatian Defenders
            || (d == 5 && m == Month::August)
            // Assumption of Mary
            || (d == 15 && m == Month::August)
            // Remembrance Day for the Victims of the Vukovar and Skabrnja War Memorials
            || (d == 18 && m == Month::November)
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
        assert_eq!(Croatia::new(Market::Zse).name(), "Zagreb stock exchange");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Croatia::new(Market::Zse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(6, Month::January, 2019))); // Epiphany
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(30, Month::May, 2019))); // National Day
        assert!(c.is_holiday(Date::new(22, Month::June, 2019))); // Anti-Fascist Struggle Day
        assert!(c.is_holiday(Date::new(5, Month::August, 2019))); // Victory Day
        assert!(c.is_holiday(Date::new(15, Month::August, 2019))); // Assumption of Mary
        assert!(c.is_holiday(Date::new(18, Month::November, 2019))); // Remembrance Day
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // St. Stephen
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // New Year's Eve
    }

    #[test]
    fn weekend_rule() {
        let c = Croatia::new(Market::Zse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
