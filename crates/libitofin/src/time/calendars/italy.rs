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
            // Saint Francis of Assisi
            || (d == 4 && m == Month::October && y >= 2026)
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
    use crate::time::date::{Day, Year};

    fn dates(list: &[(Day, Month, Year)]) -> Vec<Date> {
        list.iter().map(|&(d, m, y)| Date::new(d, m, y)).collect()
    }

    /// `testItaly`, `test-suite/calendars.cpp:745` (v1.44).
    #[test]
    fn quantlib_test_italy() {
        use Month::*;
        let c = Italy::new(Market::Settlement);
        assert_eq!(
            c.holiday_list(
                Date::new(1, January, 2002),
                Date::new(31, December, 2004),
                false
            ),
            dates(&[
                (1, January, 2002),
                (1, April, 2002),
                (25, April, 2002),
                (1, May, 2002),
                (15, August, 2002),
                (1, November, 2002),
                (25, December, 2002),
                (26, December, 2002),
                (1, January, 2003),
                (6, January, 2003),
                (21, April, 2003),
                (25, April, 2003),
                (1, May, 2003),
                (2, June, 2003),
                (15, August, 2003),
                (8, December, 2003),
                (25, December, 2003),
                (26, December, 2003),
                (1, January, 2004),
                (6, January, 2004),
                (12, April, 2004),
                (2, June, 2004),
                (1, November, 2004),
                (8, December, 2004),
            ])
        );
        assert_eq!(
            c.holiday_list(
                Date::new(1, January, 1999),
                Date::new(31, December, 1999),
                false
            ),
            dates(&[
                (1, January, 1999),
                (6, January, 1999),
                (5, April, 1999),
                (1, November, 1999),
                (8, December, 1999),
                (31, December, 1999),
            ])
        );
        assert_eq!(
            c.holiday_list(
                Date::new(1, January, 2028),
                Date::new(31, December, 2028),
                false
            ),
            dates(&[
                (6, January, 2028),
                (17, April, 2028),
                (25, April, 2028),
                (1, May, 2028),
                (2, June, 2028),
                (15, August, 2028),
                (4, October, 2028),
                (1, November, 2028),
                (8, December, 2028),
                (25, December, 2028),
                (26, December, 2028),
            ])
        );
        let exchange = Italy::new(Market::Exchange);
        let republic_day = Date::new(2, June, 2025);
        assert!(c.is_holiday(republic_day));
        assert!(exchange.is_business_day(republic_day));
    }

    #[test]
    fn saint_francis_is_a_holiday_only_from_2026() {
        let c = Italy::new(Market::Settlement);
        assert!(c.is_business_day(Date::new(4, Month::October, 2023)));
        assert!(c.is_holiday(Date::new(4, Month::October, 2027)));
    }

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
