//! French calendars.
//!
//! Port of `ql/time/calendars/france.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// French calendar markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Paris stock-exchange calendar.
    Exchange,
}

/// The French calendar.
pub struct France;

impl France {
    /// Builds a French calendar for the given market.
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
        "French settlement".to_string()
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
            // Jour de l'An
            || (d == 1 && m == Month::January)
            // Lundi de Paques
            || (dd == em)
            // Fete du Travail
            || (d == 1 && m == Month::May)
            // Victoire 1945
            || (d == 8 && m == Month::May)
            // Ascension
            || (d == 10 && m == Month::May)
            // Pentecote
            || (d == 21 && m == Month::May)
            // Fete nationale
            || (d == 14 && m == Month::July)
            // Assomption
            || (d == 15 && m == Month::August)
            // Toussaint
            || (d == 1 && m == Month::November)
            // Armistice 1918
            || (d == 11 && m == Month::November)
            // Noel
            || (d == 25 && m == Month::December))
    }
}

impl CalendarImpl for ExchangeImpl {
    fn name(&self) -> String {
        "Paris stock exchange".to_string()
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
            // Jour de l'An
            || (d == 1 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Labor Day
            || (d == 1 && m == Month::May)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas Day
            || (d == 25 && m == Month::December)
            // Boxing Day
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
        assert_eq!(France::new(Market::Settlement).name(), "French settlement");
        assert_eq!(France::new(Market::Exchange).name(), "Paris stock exchange");
    }

    #[test]
    fn settlement_fixed_holidays() {
        let c = France::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(8, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(14, Month::July, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(11, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
    }

    #[test]
    fn exchange_fixed_holidays() {
        let c = France::new(Market::Exchange);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(24, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = France::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
