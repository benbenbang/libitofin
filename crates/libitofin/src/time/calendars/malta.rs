//! Maltese calendar.
//!
//! Port of `ql/time/calendars/malta.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Malta calendar markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Malta Stock Exchange.
    Mse,
}

/// The weekend for the Malta Stock Exchange is Friday and Saturday.
fn is_weekend(w: Weekday) -> bool {
    w == Weekday::Friday || w == Weekday::Saturday
}

/// The Maltese calendar.
pub struct Malta;

impl Malta {
    /// Builds a Maltese calendar for the given market.
    ///
    /// QuantLib defaults to [`Market::Mse`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Mse => shared(MseImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct MseImpl;

impl CalendarImpl for MseImpl {
    fn name(&self) -> String {
        "Malta Stock Exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        let em = western_easter_monday(y);
        let gf = em - 3; // Good Friday
        !(is_weekend(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // St. Paul's Shipwreck
            || (d == 10 && m == Month::February)
            // St. Joseph's Day
            || (d == 19 && m == Month::March)
            // Freedom Day
            || (d == 31 && m == Month::March)
            // Good Friday
            || (date.day_of_year() == gf)
            // Easter Monday (exchange holiday)
            || (date.day_of_year() == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Imnarja (Feast of Saints Peter & Paul)
            || (d == 29 && m == Month::June)
            // Assumption of Mary
            || (d == 15 && m == Month::August)
            // Our Lady of Victories (Nativity of Mary)
            || (d == 8 && m == Month::September)
            // Independence Day
            || (d == 21 && m == Month::September)
            // Immaculate Conception
            || (d == 8 && m == Month::December)
            // Republic Day
            || (d == 13 && m == Month::December)
            // Christmas Vigil
            || (d == 24 && m == Month::December)
            // Christmas Day
            || (d == 25 && m == Month::December)
            // Boxing Day (occasionally observed by MSE)
            || (d == 26 && m == Month::December)
            // New Year's Eve (non-trading)
            || (d == 31 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Malta::new(Market::Mse).name(), "Malta Stock Exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Malta::new(Market::Mse);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(10, Month::February, 2019)));
        assert!(c.is_holiday(Date::new(19, Month::March, 2019)));
        assert!(c.is_holiday(Date::new(31, Month::March, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(29, Month::June, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(8, Month::September, 2019)));
        assert!(c.is_holiday(Date::new(21, Month::September, 2019)));
        assert!(c.is_holiday(Date::new(8, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(13, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(24, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn friday_saturday_weekend() {
        let c = Malta::new(Market::Mse);
        assert!(c.is_weekend(Weekday::Friday));
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(!c.is_weekend(Weekday::Sunday));
    }
}
