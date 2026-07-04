//! Icelandic calendars.
//!
//! Port of `ql/time/calendars/iceland.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Icelandic markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Iceland stock exchange.
    Icex,
}

/// The Icelandic calendar.
pub struct Iceland;

impl Iceland {
    /// Builds an Icelandic calendar for the given `market`.
    ///
    /// QuantLib defaults the market to [`Market::Icex`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Icex => shared(IcexImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct IcexImpl;

impl CalendarImpl for IcexImpl {
    fn name(&self) -> String {
        "Iceland stock exchange".to_string()
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
            // Easter Monday
            || (dd == em)
            // First day of Summer
            || ((19..=25).contains(&d) && w == Weekday::Thursday && m == Month::April)
            // Ascension Thursday
            || (dd == em + 38)
            // Pentecost Monday
            || (dd == em + 49)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Independence Day
            || (d == 17 && m == Month::June)
            // Commerce Day
            || (d <= 7 && w == Weekday::Monday && m == Month::August)
            // Christmas
            || (d == 25 && m == Month::December)
            // Boxing Day
            || (d == 26 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Iceland::new(Market::Icex).name(), "Iceland stock exchange");
    }

    #[test]
    fn unconditional_fixed_holidays() {
        let c = Iceland::new(Market::Icex);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(17, Month::June, 2019))); // Independence Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // Boxing Day
    }

    #[test]
    fn weekend_rule() {
        let c = Iceland::new(Market::Icex);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
