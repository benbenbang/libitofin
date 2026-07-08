//! German calendars.
//!
//! Port of `ql/time/calendars/germany.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// German calendar markets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Frankfurt stock exchange.
    FrankfurtStockExchange,
    /// Xetra.
    Xetra,
    /// Eurex.
    Eurex,
    /// Euwax.
    Euwax,
}

/// The German calendar.
pub struct Germany;

impl Germany {
    /// Builds a German calendar for the given market.
    ///
    /// QuantLib defaults to [`Market::FrankfurtStockExchange`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::FrankfurtStockExchange => shared(FrankfurtStockExchangeImpl),
            Market::Xetra => shared(XetraImpl),
            Market::Eurex => shared(EurexImpl),
            Market::Euwax => shared(EuwaxImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct SettlementImpl;
struct FrankfurtStockExchangeImpl;
struct XetraImpl;
struct EurexImpl;
struct EuwaxImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "German settlement".to_string()
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
            // Ascension Thursday
            || (dd == em + 38)
            // Whit Monday
            || (dd == em + 49)
            // Corpus Christi
            || (dd == em + 59)
            // Labour Day
            || (d == 1 && m == Month::May)
            // National Day
            || (d == 3 && m == Month::October)
            // Christmas Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Boxing Day
            || (d == 26 && m == Month::December))
    }
}

impl CalendarImpl for FrankfurtStockExchangeImpl {
    fn name(&self) -> String {
        "Frankfurt stock exchange".to_string()
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
            // Christmas' Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Christmas Day
            || (d == 26 && m == Month::December))
    }
}

impl CalendarImpl for XetraImpl {
    fn name(&self) -> String {
        "Xetra".to_string()
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
            // Christmas' Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Christmas Day
            || (d == 26 && m == Month::December))
    }
}

impl CalendarImpl for EurexImpl {
    fn name(&self) -> String {
        "Eurex".to_string()
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
            // Christmas' Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Christmas Day
            || (d == 26 && m == Month::December)
            // New Year's Eve
            || (d == 31 && m == Month::December))
    }
}

impl CalendarImpl for EuwaxImpl {
    fn name(&self) -> String {
        "Euwax".to_string()
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
        !((w == Weekday::Saturday || w == Weekday::Sunday)
            // New Year's Day
            || (d == 1 && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // Labour Day
            || (d == 1 && m == Month::May)
            // Whit Monday
            || (dd == em + 49)
            // Christmas' Eve
            || (d == 24 && m == Month::December)
            // Christmas
            || (d == 25 && m == Month::December)
            // Christmas Day
            || (d == 26 && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Germany::new(Market::Settlement).name(), "German settlement");
        assert_eq!(
            Germany::new(Market::FrankfurtStockExchange).name(),
            "Frankfurt stock exchange"
        );
        assert_eq!(Germany::new(Market::Xetra).name(), "Xetra");
        assert_eq!(Germany::new(Market::Eurex).name(), "Eurex");
        assert_eq!(Germany::new(Market::Euwax).name(), "Euwax");
    }

    #[test]
    fn settlement_fixed_holidays() {
        let c = Germany::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(3, Month::October, 2019)));
        assert!(c.is_holiday(Date::new(24, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn eurex_new_years_eve() {
        let c = Germany::new(Market::Eurex);
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Germany::new(Market::Euwax);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
