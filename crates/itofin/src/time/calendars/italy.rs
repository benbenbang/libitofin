//! Italian calendars.
//!
//! Port of `ql/time/calendars/italy.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Italian calendar markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Milan stock-exchange calendar.
    Exchange,
}

/// The Italian calendar.
pub struct Italy;

impl Italy {
    /// Builds an Italian calendar for the given market.
    ///
    /// QuantLib defaults to [`Market::Settlement`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Exchange => shared(ExchangeImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct SettlementImpl;
struct ExchangeImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "Italian settlement".to_string()
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
            // Easter Monday
            || (dd == em)
            // Liberation Day
            || (d == 25 && m == Month::April)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Republic Day
            || (d == 2 && m == Month::June && y >= 2000)
            // Assumption
            || (d == 15 && m == Month::August)
            // All Saints' Day
            || (d == 1 && m == Month::November)
            // Immaculate Conception
            || (d == 8 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen
            || (d == 26 && m == Month::December)
            // December 31st, 1999 only
            || (d == 31 && m == Month::December && y == 1999))
    }
}

impl CalendarImpl for ExchangeImpl {
    fn name(&self) -> String {
        "Milan stock exchange".to_string()
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
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Assumption
            || (d == 15 && m == Month::August)
            // Christmas' Eve
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
    fn names_match_quantlib() {
        assert_eq!(Italy::new(Market::Settlement).name(), "Italian settlement");
        assert_eq!(Italy::new(Market::Exchange).name(), "Milan stock exchange");
    }

    #[test]
    fn settlement_fixed_holidays() {
        let c = Italy::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(6, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::April, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(8, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn exchange_fixed_holidays() {
        let c = Italy::new(Market::Exchange);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(24, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Italy::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
