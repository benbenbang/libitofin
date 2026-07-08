//! United Kingdom calendars.
//!
//! Port of `ql/time/calendars/unitedkingdom.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// United Kingdom markets.
///
/// QuantLib defaults this to [`Market::Settlement`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// London stock-exchange calendar.
    Exchange,
    /// London metals-exchange calendar.
    Metals,
}

/// United Kingdom calendar.
pub struct UnitedKingdom;

impl UnitedKingdom {
    /// Builds a UK calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::Exchange => shared(ExchangeImpl),
            Market::Metals => shared(MetalsImpl),
        };
        Calendar::from_impl(imp)
    }
}

fn is_bank_holiday(d: i32, w: Weekday, m: Month, y: i32) -> bool {
    // first Monday of May (Early May Bank Holiday)
    // moved to May 8th in 1995 and 2020 for V.E. day
    (d <= 7 && w == Weekday::Monday && m == Month::May && y != 1995 && y != 2020)
        || (d == 8 && m == Month::May && (y == 1995 || y == 2020))
        // last Monday of May (Spring Bank Holiday)
        // moved to in 2002, 2012 and 2022 for the Golden, Diamond and Platinum
        // Jubilee with an additional holiday
        || (d >= 25 && w == Weekday::Monday && m == Month::May && y != 2002 && y != 2012 && y != 2022)
        || ((d == 3 || d == 4) && m == Month::June && y == 2002)
        || ((d == 4 || d == 5) && m == Month::June && y == 2012)
        || ((d == 2 || d == 3) && m == Month::June && y == 2022)
        // last Monday of August (Summer Bank Holiday)
        || (d >= 25 && w == Weekday::Monday && m == Month::August)
        // April 29th, 2011 only (Royal Wedding Bank Holiday)
        || (d == 29 && m == Month::April && y == 2011)
        // September 19th, 2022 only (The Queen's Funeral Bank Holiday)
        || (d == 19 && m == Month::September && y == 2022)
        // May 8th, 2023 (King Charles III Coronation Bank Holiday)
        || (d == 8 && m == Month::May && y == 2023)
}

// The Settlement, Exchange and Metals markets share an identical
// `isBusinessDay` body in QuantLib; only their `name()` differs.
fn uk_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let dd = date.day_of_year();
    let m = date.month();
    let y = date.year();
    let em = western_easter_monday(y);

    !(is_weekend_sat_sun(w)
        // New Year's Day (possibly moved to Monday)
        || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday)) && m == Month::January)
        // Good Friday
        || (dd == em - 3)
        // Easter Monday
        || (dd == em)
        || is_bank_holiday(d, w, m, y)
        // Christmas (possibly moved to Monday or Tuesday)
        || ((d == 25 || (d == 27 && (w == Weekday::Monday || w == Weekday::Tuesday)))
            && m == Month::December)
        // Boxing Day (possibly moved to Monday or Tuesday)
        || ((d == 26 || (d == 28 && (w == Weekday::Monday || w == Weekday::Tuesday)))
            && m == Month::December)
        // December 31st, 1999 only
        || (d == 31 && m == Month::December && y == 1999))
}

struct SettlementImpl;
struct ExchangeImpl;
struct MetalsImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "UK settlement".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        uk_is_business_day(date)
    }
}

impl CalendarImpl for ExchangeImpl {
    fn name(&self) -> String {
        "London stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        uk_is_business_day(date)
    }
}

impl CalendarImpl for MetalsImpl {
    fn name(&self) -> String {
        "London metals exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        uk_is_business_day(date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            UnitedKingdom::new(Market::Settlement).name(),
            "UK settlement"
        );
        assert_eq!(
            UnitedKingdom::new(Market::Exchange).name(),
            "London stock exchange"
        );
        assert_eq!(
            UnitedKingdom::new(Market::Metals).name(),
            "London metals exchange"
        );
    }

    #[test]
    fn fixed_holidays() {
        let c = UnitedKingdom::new(Market::Settlement);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019))); // New Year's Day
        assert!(c.is_holiday(Date::new(25, Month::December, 2019))); // Christmas
        assert!(c.is_holiday(Date::new(26, Month::December, 2019))); // Boxing Day
    }

    #[test]
    fn weekend_rule() {
        let c = UnitedKingdom::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
