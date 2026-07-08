//! Weekends-only calendar.
//!
//! Port of `ql/time/calendars/weekendsonly.{hpp,cpp}`.

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl, is_weekend_sat_sun};
use crate::time::date::Date;
use crate::time::weekday::Weekday;

/// A calendar whose only holidays are Saturdays and Sundays.
pub struct WeekendsOnly;

impl WeekendsOnly {
    /// Builds a weekends-only calendar.
    pub fn new() -> Calendar {
        Calendar::from_impl(shared(Impl))
    }
}

struct Impl;

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        "weekends only".to_string()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        is_weekend_sat_sun(w)
    }

    fn is_business_day(&self, date: Date) -> bool {
        !is_weekend_sat_sun(date.weekday())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn only_weekends_are_holidays() {
        let c = WeekendsOnly::new();
        assert!(c.is_business_day(Date::new(1, Month::January, 2018))); // Monday
        assert!(c.is_business_day(Date::new(25, Month::December, 2018))); // Christmas is a business day here
        assert!(c.is_holiday(Date::new(6, Month::January, 2018))); // Saturday
        assert!(c.is_holiday(Date::new(7, Month::January, 2018))); // Sunday
    }

    #[test]
    fn name_matches_quantlib() {
        assert_eq!(WeekendsOnly::new().name(), "weekends only");
    }
}
