//! Bespoke calendar.
//!
//! Port of `ql/time/calendars/bespokecalendar.{hpp,cpp}`. A bespoke calendar
//! has no predefined holidays; weekend days are declared through
//! [`BespokeCalendar::add_weekend`], and one-off holidays through the usual
//! [`Calendar::add_holiday`](crate::time::calendar::Calendar::add_holiday).
//!
//! The weekend mask lives behind a shared [`Cell`], so every [`Calendar`]
//! produced by [`BespokeCalendar::calendar`] observes weekends added later,
//! reproducing QuantLib's "linked instances" behaviour. (The per-calendar
//! added/removed holiday overrides are, as elsewhere in this port, not shared
//! across separate `calendar()` handles - see the [`calendar`](crate::time::calendar)
//! module docs.)

use std::cell::Cell;

use crate::shared::Shared;
use crate::time::calendar::{Calendar, CalendarImpl};
use crate::time::date::Date;
use crate::time::weekday::Weekday;

/// A user-defined calendar whose weekend days are configured explicitly.
///
/// # Warning
///
/// Different bespoke calendars created with the same name (or all created with
/// no name) compare as equal, matching QuantLib.
pub struct BespokeCalendar {
    imp: Shared<Impl>,
}

impl BespokeCalendar {
    /// Builds a bespoke calendar with the given name and no weekend days.
    pub fn new(name: impl Into<String>) -> BespokeCalendar {
        BespokeCalendar {
            imp: Shared::new(Impl {
                name: name.into(),
                weekend_mask: Cell::new(0),
            }),
        }
    }

    /// Marks `w` as part of the weekend. Visible through every [`Calendar`]
    /// already produced by [`calendar`](Self::calendar).
    pub fn add_weekend(&self, w: Weekday) {
        let mask = self.imp.weekend_mask.get();
        self.imp.weekend_mask.set(mask | (1 << w.ordinal()));
    }

    /// Produces a [`Calendar`] backed by this bespoke definition.
    pub fn calendar(&self) -> Calendar {
        Calendar::from_impl(self.imp.clone())
    }
}

struct Impl {
    name: String,
    weekend_mask: Cell<u32>,
}

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        (self.weekend_mask.get() & (1 << w.ordinal())) != 0
    }

    fn is_business_day(&self, date: Date) -> bool {
        !self.is_weekend(date.weekday())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn no_weekends_by_default() {
        let c = BespokeCalendar::new("test").calendar();
        assert!(c.is_business_day(Date::new(6, Month::January, 2018))); // Saturday
    }

    #[test]
    fn added_weekend_is_visible_through_existing_handle() {
        let b = BespokeCalendar::new("test");
        let c = b.calendar();
        b.add_weekend(Weekday::Sunday);
        assert!(c.is_weekend(Weekday::Sunday));
        assert!(c.is_holiday(Date::new(7, Month::January, 2018))); // Sunday
        assert!(c.is_business_day(Date::new(6, Month::January, 2018))); // Saturday still open
    }

    #[test]
    fn added_holiday_works() {
        let c = BespokeCalendar::new("test").calendar();
        let d = Date::new(3, Month::January, 2018);
        c.add_holiday(d);
        assert!(c.is_holiday(d));
    }
}
