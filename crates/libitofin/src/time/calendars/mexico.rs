//! Mexican calendars.
//!
//! Port of `ql/time/calendars/mexico.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Mexican calendar markets.
///
/// QuantLib defaults this to [`Market::Bmv`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Mexican stock exchange.
    Bmv,
}

/// The Mexican calendar.
pub struct Mexico;

impl Mexico {
    /// Builds a Mexican calendar for the given market. QuantLib defaults to
    /// [`Market::Bmv`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Bmv => shared(BmvImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct BmvImpl;

impl CalendarImpl for BmvImpl {
    fn name(&self) -> String {
        "Mexican stock exchange".to_string()
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
            // Constitution Day
            || (y <= 2005 && d == 5 && m == Month::February)
            || (y >= 2006 && d <= 7 && w == Weekday::Monday && m == Month::February)
            // Birthday of Benito Juarez
            || (y <= 2005 && d == 21 && m == Month::March)
            || (y >= 2006 && (15..=21).contains(&d) && w == Weekday::Monday && m == Month::March)
            // Holy Thursday
            || (dd == em - 4)
            // Good Friday
            || (dd == em - 3)
            // Labour Day
            || (d == 1 && m == Month::May)
            // National Day
            || (d == 16 && m == Month::September)
            // Inauguration Day
            || (d == 1 && m == Month::October && y >= 2024 && (y - 2024) % 6 == 0)
            // All Souls Day
            || (d == 2 && m == Month::November)
            // Revolution Day
            || (y <= 2005 && d == 20 && m == Month::November)
            || (y >= 2006 && (15..=21).contains(&d) && w == Weekday::Monday && m == Month::November)
            // Our Lady of Guadalupe
            || (d == 12 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against fixed Mexican holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn name_matches_quantlib() {
        assert_eq!(Mexico::new(Market::Bmv).name(), "Mexican stock exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Mexico::new(Market::Bmv);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(16, Month::September, 2019)));
        assert!(c.is_holiday(Date::new(2, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(12, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Mexico::new(Market::Bmv);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
