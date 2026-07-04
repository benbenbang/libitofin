//! Australian calendar.
//!
//! Port of `ql/time/calendars/australia.{hpp,cpp}`.

// Range clauses are kept in the verbatim C++ `d >= a && d <= b` form.
#![allow(clippy::manual_range_contains)]

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Australian markets.
///
/// QuantLib defaults to [`Market::Settlement`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Australia ASX calendar.
    Asx,
}

/// The Australian calendar.
pub struct Australia;

impl Australia {
    /// Builds an Australian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Asx => shared(AsxImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct SettlementImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "Australia settlement".to_string()
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
            // New Year's Day (possibly moved to Monday)
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday)) && m == Month::January)
            // Australia Day, January 26th (possibly moved to Monday)
            || ((d == 26 || ((d == 27 || d == 28) && w == Weekday::Monday)) && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // ANZAC Day, April 25th
            || (d == 25 && m == Month::April)
            // Queen's Birthday, second Monday in June
            || ((d > 7 && d <= 14) && w == Weekday::Monday && m == Month::June)
            // Bank Holiday, first Monday in August
            || (d <= 7 && w == Weekday::Monday && m == Month::August)
            // Labour Day, first Monday in October
            || (d <= 7 && w == Weekday::Monday && m == Month::October)
            // Christmas, December 25th (possibly Monday or Tuesday)
            || ((d == 25 || (d == 27 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December)
            // Boxing Day, December 26th (possibly Monday or Tuesday)
            || ((d == 26 || (d == 28 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December)
            // National Day of Mourning for Her Majesty, September 22 (only 2022)
            || (d == 22 && m == Month::September && y == 2022))
    }
}

struct AsxImpl;

impl CalendarImpl for AsxImpl {
    fn name(&self) -> String {
        "Australia exchange".to_string()
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
            // New Year's Day (possibly moved to Monday)
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday)) && m == Month::January)
            // Australia Day, January 26th (possibly moved to Monday)
            || ((d == 26 || ((d == 27 || d == 28) && w == Weekday::Monday)) && m == Month::January)
            // Good Friday
            || (dd == em - 3)
            // Easter Monday
            || (dd == em)
            // ANZAC Day, April 25th
            || (d == 25 && m == Month::April)
            // Queen's Birthday, second Monday in June
            || ((d > 7 && d <= 14) && w == Weekday::Monday && m == Month::June)
            // Christmas, December 25th (possibly Monday or Tuesday)
            || ((d == 25 || (d == 27 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December)
            // Boxing Day, December 26th (possibly Monday or Tuesday)
            || ((d == 26 || (d == 28 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December)
            // National Day of Mourning for Her Majesty, September 22 (only 2022)
            || (d == 22 && m == Month::September && y == 2022))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            Australia::new(Market::Settlement).name(),
            "Australia settlement"
        );
        assert_eq!(Australia::new(Market::Asx).name(), "Australia exchange");
    }

    #[test]
    fn unconditional_holidays() {
        let c = Australia::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(25, Month::April, 2019))); // ANZAC Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
    }

    #[test]
    fn weekend_rule() {
        let c = Australia::new(Market::Asx);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(!c.is_weekend(Weekday::Monday));
    }
}
