//! Canadian calendars.
//!
//! Port of `ql/time/calendars/canada.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, western_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Canadian markets.
///
/// QuantLib defaults this to [`Market::Settlement`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Generic settlement calendar.
    Settlement,
    /// Toronto stock exchange calendar.
    TSX,
}

/// Canadian calendar.
pub struct Canada;

impl Canada {
    /// Builds a Canadian calendar for the given market.
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Settlement => shared(SettlementImpl),
            Market::TSX => shared(TsxImpl),
        };
        Calendar::from_impl(imp)
    }
}

struct SettlementImpl;
struct TsxImpl;

impl CalendarImpl for SettlementImpl {
    fn name(&self) -> String {
        "Canada".to_string()
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
            // Family Day (third Monday in February, since 2008)
            || ((d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::February && y >= 2008)
            // Good Friday
            || (dd == em - 3)
            // The Monday on or preceding 24 May (Victoria Day)
            || (d > 17 && d <= 24 && w == Weekday::Monday && m == Month::May)
            // July 1st, possibly moved to Monday (Canada Day)
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday)) && m == Month::July)
            // first Monday of August (Provincial Holiday)
            || (d <= 7 && w == Weekday::Monday && m == Month::August)
            // first Monday of September (Labor Day)
            || (d <= 7 && w == Weekday::Monday && m == Month::September)
            // September 30th, possibly moved to Monday
            // (National Day for Truth and Reconciliation, since 2021)
            || (((d == 30 && m == Month::September)
                || (d <= 2 && m == Month::October && w == Weekday::Monday))
                && y >= 2021)
            // second Monday of October (Thanksgiving Day)
            || (d > 7 && d <= 14 && w == Weekday::Monday && m == Month::October)
            // November 11th (possibly moved to Monday)
            || ((d == 11 || ((d == 12 || d == 13) && w == Weekday::Monday)) && m == Month::November)
            // Christmas (possibly moved to Monday or Tuesday)
            || ((d == 25 || (d == 27 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December)
            // Boxing Day (possibly moved to Monday or Tuesday)
            || ((d == 26 || (d == 28 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December))
    }
}

impl CalendarImpl for TsxImpl {
    fn name(&self) -> String {
        "TSX".to_string()
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
            // Family Day (third Monday in February, since 2008)
            || ((d >= 15 && d <= 21) && w == Weekday::Monday && m == Month::February && y >= 2008)
            // Good Friday
            || (dd == em - 3)
            // The Monday on or preceding 24 May (Victoria Day)
            || (d > 17 && d <= 24 && w == Weekday::Monday && m == Month::May)
            // July 1st, possibly moved to Monday (Canada Day)
            || ((d == 1 || ((d == 2 || d == 3) && w == Weekday::Monday)) && m == Month::July)
            // first Monday of August (Provincial Holiday)
            || (d <= 7 && w == Weekday::Monday && m == Month::August)
            // first Monday of September (Labor Day)
            || (d <= 7 && w == Weekday::Monday && m == Month::September)
            // second Monday of October (Thanksgiving Day)
            || (d > 7 && d <= 14 && w == Weekday::Monday && m == Month::October)
            // Christmas (possibly moved to Monday or Tuesday)
            || ((d == 25 || (d == 27 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December)
            // Boxing Day (possibly moved to Monday or Tuesday)
            || ((d == 26 || (d == 28 && (w == Weekday::Monday || w == Weekday::Tuesday)))
                && m == Month::December))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks, not a full transcription of test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Canada::new(Market::Settlement).name(), "Canada");
        assert_eq!(Canada::new(Market::TSX).name(), "TSX");
    }

    #[test]
    fn weekend_rule() {
        let c = Canada::new(Market::Settlement);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
