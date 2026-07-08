//! Null calendar.
//!
//! Port of `ql/time/calendars/nullcalendar.hpp`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl};
use crate::time::date::Date;
use crate::time::weekday::Weekday;

/// A calendar with no holidays and no weekend, for reproducing theoretical
/// calculations: dates at whole-month distances keep the same day of month.
pub struct NullCalendar;

impl NullCalendar {
    /// Builds a null calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "Null".to_string()
    }

    fn is_weekend(&self, _w: Weekday) -> bool {
        false
    }

    fn is_business_day(&self, _date: Date) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn every_day_is_a_business_day() {
        let c = NullCalendar::new();
        assert!(c.is_business_day(Date::new(1, Month::January, 2018))); // Monday
        assert!(c.is_business_day(Date::new(6, Month::January, 2018))); // Saturday
        assert!(c.is_business_day(Date::new(25, Month::December, 2018)));
        assert!(!c.is_weekend(Weekday::Sunday));
    }

    #[test]
    fn name_matches_quantlib() {
        assert_eq!(NullCalendar::new().name(), "Null");
    }
}
