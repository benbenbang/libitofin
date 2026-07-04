//! Polish calendars.
//!
//! Port of `ql/time/calendars/poland.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Polish calendar markets.
///
/// QuantLib defaults this to [`Market::Settlement`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Settlement calendar.
    Settlement,
    /// Warsaw stock exchange calendar.
    Wse,
}

/// The Polish calendar.
pub struct Poland;

impl Poland {
    /// Builds a Polish calendar for the given market. QuantLib defaults to
    /// [`Market::Settlement`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Wse => shared(WseImpl),
        };
        Calendar::from_impl(imp)
    }
}

fn settlement_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let dd = date.day_of_year();
    let m = date.month();
    let y = date.year();
    let em = western_easter_monday(y);
    !(is_weekend_sat_sun(w)
        // Easter Monday
        || (dd == em)
        // Corpus Christi
        || (dd == em + 59)
        // New Year's Day
        || (d == 1 && m == Month::January)
        // Epiphany
        || (d == 6 && m == Month::January && y >= 2011)
        // May Day
        || (d == 1 && m == Month::May)
        // Constitution Day
        || (d == 3 && m == Month::May)
        // Assumption of the Blessed Virgin Mary
        || (d == 15 && m == Month::August)
        // All Saints Day
        || (d == 1 && m == Month::November)
        // Independence Day
        || (d == 11 && m == Month::November)
        // Christmas
        || (d == 25 && m == Month::December)
        // 2nd Day of Christmas
        || (d == 26 && m == Month::December))
}

struct SettlementImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "Poland Settlement".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        settlement_is_business_day(date)
    }
}

struct WseImpl;

impl CalendarImpl for WseImpl {
    fn name(&self) -> String {
        "Warsaw stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        // Additional holidays for Warsaw Stock Exchange
        // see https://www.gpw.pl/session-details
        let d = date.day_of_month();
        let m = date.month();

        if m == Month::December && (d == 24 || d == 31) {
            return false;
        }

        settlement_is_business_day(date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against fixed Polish holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Poland::new(Market::Settlement).name(), "Poland Settlement");
        assert_eq!(Poland::new(Market::Wse).name(), "Warsaw stock exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Poland::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(3, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(11, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn wse_extra_holidays() {
        let c = Poland::new(Market::Wse);
        assert!(c.is_holiday(Date::new(24, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(31, Month::December, 2019)));
    }

    #[test]
    fn weekend_rule() {
        let c = Poland::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
