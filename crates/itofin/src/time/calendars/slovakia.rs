//! Slovak calendars.
//!
//! Port of `ql/time/calendars/slovakia.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Slovak markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Bratislava stock exchange.
    Bsse,
}

/// Slovak calendars. Defaults to [`Market::Bsse`] in QuantLib.
pub struct Slovakia;

impl Slovakia {
    /// Builds a Slovak calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Bsse => shared(BsseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct BsseImpl;

impl CalendarImpl for BsseImpl {
    fn name(&self) -> String {
        "Bratislava stock exchange".to_string()
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
            // Epiphany
            || (d == 6 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // May Day
            || (d == 1 && m == Month::May)
            // Liberation of the Republic
            || (d == 8 && m == Month::May)
            // SS. Cyril and Methodius
            || (d == 5 && m == Month::July)
            // Slovak National Uprising
            || (d == 29 && m == Month::August)
            // Constitution of the Slovak Republic
            || (d == 1 && m == Month::September)
            // Our Lady of the Seven Sorrows
            || (d == 15 && m == Month::September)
            // All Saints Day
            || (d == 1 && m == Month::November)
            // Freedom and Democracy of the Slovak Republic
            || (d == 17 && m == Month::November)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen
            || (d == 26 && m == Month::December)
            // unidentified closing days for stock exchange
            || ((24..=31).contains(&d) && m == Month::December && y == 2004)
            || ((24..=31).contains(&d) && m == Month::December && y == 2005))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(
            Slovakia::new(Market::Bsse).name(),
            "Bratislava stock exchange"
        );
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Slovakia::new(Market::Bsse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(6, Month::January, 2019))); // Epiphany
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // May Day
        assert!(c.is_holiday(Date::new(8, Month::May, 2019))); // Liberation of the Republic
        assert!(c.is_holiday(Date::new(5, Month::July, 2019))); // SS. Cyril and Methodius
        assert!(c.is_holiday(Date::new(29, Month::August, 2019))); // Slovak National Uprising
        assert!(c.is_holiday(Date::new(1, Month::September, 2019))); // Constitution Day
        assert!(c.is_holiday(Date::new(15, Month::September, 2019))); // Our Lady of Seven Sorrows
        assert!(c.is_holiday(Date::new(1, Month::November, 2019))); // All Saints Day
        assert!(c.is_holiday(Date::new(17, Month::November, 2019))); // Freedom and Democracy
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // St. Stephen
    }

    #[test]
    fn weekend_rule() {
        let c = Slovakia::new(Market::Bsse);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
