//! Romanian calendars.
//!
//! Port of `ql/time/calendars/romania.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun, orthodox_easter_monday};
use crate::time::date::{Date, Month};
use crate::time::weekday::Weekday;

/// Romanian calendar markets.
///
/// QuantLib defaults this to [`Market::BVB`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Market {
    /// Public holidays.
    Public,
    /// Bucharest stock exchange.
    BVB,
}

/// The Romanian calendar.
pub struct Romania;

impl Romania {
    /// Builds a Romanian calendar for the given market. QuantLib defaults to
    /// [`Market::BVB`].
    pub fn new(market: Market) -> Calendar {
        let imp: crate::shared::Shared<dyn CalendarImpl> = match market {
            Market::Public => shared(PublicImpl),
            Market::BVB => shared(BvbImpl),
        };
        Calendar::from_impl(imp)
    }
}

fn public_is_business_day(date: Date) -> bool {
    let w = date.weekday();
    let d = date.day_of_month();
    let dd = date.day_of_year();
    let m = date.month();
    let y = date.year();
    let em = orthodox_easter_monday(y);
    !(is_weekend_sat_sun(w)
        // New Year's Day
        || (d == 1 && m == Month::January)
        // Day after New Year's Day
        || (d == 2 && m == Month::January)
        // Unification Day
        || (d == 24 && m == Month::January)
        // Orthodox Easter Monday
        || (dd == em)
        // Labour Day
        || (d == 1 && m == Month::May)
        // Pentecost
        || (dd == em + 49)
        // Children's Day (since 2017)
        || (d == 1 && m == Month::June && y >= 2017)
        // St Marys Day
        || (d == 15 && m == Month::August)
        // Feast of St Andrew
        || (d == 30 && m == Month::November)
        // National Day
        || (d == 1 && m == Month::December)
        // Christmas
        || (d == 25 && m == Month::December)
        // 2nd Day of Chritsmas
        || (d == 26 && m == Month::December))
}

struct PublicImpl;

impl CalendarImpl for PublicImpl {
    fn name(&self) -> String {
        "Romania".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        public_is_business_day(date)
    }
}

struct BvbImpl;

impl CalendarImpl for BvbImpl {
    fn name(&self) -> String {
        "Bucharest stock exchange".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        if !public_is_business_day(date) {
            return false;
        }
        let d = date.day_of_month();
        let m = date.month();
        let y = date.year();
        // one-off closing days
        if m == Month::December && y == 2014 && (d == 24 || d == 31) {
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Spot-checks against fixed Romanian holidays; not a full transcription of
    // test-suite/calendars.cpp.
    #[test]
    fn names_match_quantlib() {
        assert_eq!(Romania::new(Market::Public).name(), "Romania");
        assert_eq!(Romania::new(Market::BVB).name(), "Bucharest stock exchange");
    }

    #[test]
    fn fixed_holidays() {
        let c = Romania::new(Market::Public);
        assert!(c.is_holiday(Date::new(1, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(2, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(24, Month::January, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::May, 2019)));
        assert!(c.is_holiday(Date::new(15, Month::August, 2019)));
        assert!(c.is_holiday(Date::new(30, Month::November, 2019)));
        assert!(c.is_holiday(Date::new(1, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(25, Month::December, 2019)));
        assert!(c.is_holiday(Date::new(26, Month::December, 2019)));
    }

    #[test]
    fn bvb_one_off_closings() {
        let c = Romania::new(Market::BVB);
        assert!(c.is_holiday(Date::new(24, Month::December, 2014)));
        assert!(c.is_holiday(Date::new(31, Month::December, 2014)));
    }

    #[test]
    fn weekend_rule() {
        let c = Romania::new(Market::BVB);
        assert!(c.is_weekend(Weekday::Saturday));
        assert!(c.is_weekend(Weekday::Sunday));
    }
}
