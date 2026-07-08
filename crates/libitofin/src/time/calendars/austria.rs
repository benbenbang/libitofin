//! Austrian calendars.
//!
//! Port of `ql/time/calendars/austria.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Austrian markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Vienna stock-exchange calendar.
    Exchange,
}

/// The Austrian calendar.
pub struct Austria;

impl Austria {
    /// Builds an Austrian calendar for the given `market`.
    ///
    /// QuantLib defaults the market to [`Market::Settlement`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Exchange => shared(ExchangeImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct SettlementImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "Austrian settlement".to_string()
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
            // Ascension Thurday
            || (dd == em + 38)
            // Whit Monday
            || (dd == em + 49)
            // Corpus Christi
            || (dd == em + 59)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Assumption
            || (d == 15 && m == Month::August)
            // National Holiday since 1967
            || (d == 26 && m == Month::October && y >= 1967)
            // National Holiday 1919-1934
            || (d == 12 && m == Month::November && (1919..=1934).contains(&y))
            // All Saints' Day
            || (d == 1 && m == Month::November)
            // Immaculate Conception
            || (d == 8 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen
            || (d == 26 && m == Month::December))
    }
}

struct ExchangeImpl;

impl CalendarImpl for ExchangeImpl {
    fn name(&self) -> String {
        "Vienna stock exchange".to_string()
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
            // Whit Monay
            || (dd == em + 49)
            // Labour Day
            || (d == 1 && m == Month::May)
            // National Holiday since 1967
            || (d == 26 && m == Month::October && y >= 1967)
            // National Holiday 1919-1934
            || (d == 12 && m == Month::November && (1919..=1934).contains(&y))
            // Christmas' Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // St. Stephen
            || (d == 26 && m == Month::December)
            // Exchange Holiday
            || (d == 31 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            Austria::new(Market::Settlement).name(),
            "Austrian settlement"
        );
        assert_eq!(
            Austria::new(Market::Exchange).name(),
            "Vienna stock exchange"
        );
    }

    #[test]
    fn settlement_unconditional_fixed_holidays() {
        let c = Austria::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(6, Month::January, 2019))); // Epiphany
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(15, Month::August, 2019))); // Assumption
        assert!(c.is_holiday(Date::new(1, Month::November, 2019))); // All Saints' Day
        assert!(c.is_holiday(Date::new(8, Month::December, 2019))); // Immaculate Conception
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // St. Stephen
    }

    #[test]
    fn exchange_unconditional_fixed_holidays() {
        let c = Austria::new(Market::Exchange);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labour Day
        assert!(c.is_holiday(Date::new(24, Month::December, 2019))); // Christmas' Eve
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // St. Stephen
        assert!(c.is_holiday(Date::new(31, Month::December, 2019))); // Exchange Holiday
    }

    #[test]
    fn weekend_rule() {
        let c = Austria::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
