//! Brazilian calendars.
//!
//! Port of `ql/time/calendars/brazil.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Brazilian markets.
///
/// QuantLib defaults this to [`Market::Settlement`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// BOVESPA (stock exchange) calendar.
    Exchange,
}

/// Brazilian calendar.
pub struct Brazil;

impl Brazil {
    /// Builds a Brazilian calendar for the given market.
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
        "Brazil".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        let dd = date.day_of_year();
        let em = western_easter_monday(y);

        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Tiradentes Day
            || (d == 21 && m == Month::April)
            // Labor Day
            || (d == 1 && m == Month::May)
            // Independence Day
            || (d == 7 && m == Month::September)
            // Nossa Sra. Aparecida Day
            || (d == 12 && m == Month::October)
            // All Souls Day
            || (d == 2 && m == Month::November)
            // Republic Day
            || (d == 15 && m == Month::November)
            // Black Awareness Day
            || (d == 20 && m == Month::November && y >= 2024)
            // Christmas
            || (d == 25 && m == Month::December)
            // Passion of Christ
            || (dd == em - 3)
            // Carnival
            || (dd == em - 49 || dd == em - 48)
            // Corpus Christi
            || (dd == em + 59))
    }
}

impl CalendarImpl for ExchangeImpl {
    fn name(&self) -> String {
        "BOVESPA".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        let w = date.weekday();
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        let dd = date.day_of_year();
        let em = western_easter_monday(y);

        !(is_weekend_sat_sun(w)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Sao Paulo City Day
            || (d == 25 && m == Month::January && y < 2022)
            // Tiradentes Day
            || (d == 21 && m == Month::April)
            // Labor Day
            || (d == 1 && m == Month::May)
            // Revolution Day
            || (d == 9 && m == Month::July && y < 2022)
            // Independence Day
            || (d == 7 && m == Month::September)
            // Nossa Sra. Aparecida Day
            || (d == 12 && m == Month::October)
            // All Souls Day
            || (d == 2 && m == Month::November)
            // Republic Day
            || (d == 15 && m == Month::November)
            // Black Consciousness Day
            || (d == 20 && m == Month::November && y >= 2007 && y != 2022 && y != 2023)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Passion of Christ
            || (dd == em - 3)
            // Carnival
            || (dd == em - 49 || dd == em - 48)
            // Corpus Christi
            || (dd == em + 59)
            // last business day of the year
            || (m == Month::December && (d == 31 || (d >= 29 && w == Weekday::Friday))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Brazil::new(Market::Settlement).name(), "Brazil");
        assert_eq!(Brazil::new(Market::Exchange).name(), "BOVESPA");
    }

    #[test]
    fn fixed_holidays() {
        let c = Brazil::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(21, Month::April, 2019))); // Tiradentes Day
        assert!(c.is_holiday(Date::new(1, Month::May, 2019))); // Labor Day
        assert!(c.is_holiday(Date::new(7, Month::September, 2019))); // Independence Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
    }

    #[test]
    fn weekend_rule() {
        let c = Brazil::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
