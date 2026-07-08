//! Joint calendar.
//!
//! Port of `ql/time/calendars/jointcalendar.{hpp,cpp}`. Combines several
//! calendars into one whose business days are the union or the intersection of
//! the underlying business-day sets, depending on the [`JointCalendarRule`].

use crate::shared::shared;
use crate::time::calendar::{Calendar, CalendarImpl};
use crate::time::date::Date;
use crate::time::weekday::Weekday;

/// How the business days of several calendars are combined.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JointCalendarRule {
    /// A date is a holiday for the joint calendar if it is a holiday for *any*
    /// of the given calendars (intersection of business days).
    JoinHolidays,
    /// A date is a business day for the joint calendar if it is a business day
    /// for *any* of the given calendars (union of business days).
    JoinBusinessDays,
}

/// A calendar formed by joining several others under a [`JointCalendarRule`].
pub struct JointCalendar;

impl JointCalendar {
    /// Builds a joint calendar over `calendars` with the given rule.
    ///
    /// # Panics
    ///
    /// Panics if `calendars` is empty (the name would have no leading calendar,
    /// matching QuantLib's reliance on a non-empty list).
    pub fn new(calendars: Vec<Calendar>, rule: JointCalendarRule) -> Calendar {
        assert!(!calendars.is_empty(), "at least one calendar required");
        Calendar::from_impl(shared(Impl { rule, calendars }))
    }

    /// Convenience constructor joining exactly two calendars.
    pub fn of_two(c1: Calendar, c2: Calendar, rule: JointCalendarRule) -> Calendar {
        Self::new(vec![c1, c2], rule)
    }
}

struct Impl {
    rule: JointCalendarRule,
    calendars: Vec<Calendar>,
}

impl CalendarImpl for Impl {
    fn name(&self) -> String {
        let prefix = match self.rule {
            JointCalendarRule::JoinHolidays => "JoinHolidays(",
            JointCalendarRule::JoinBusinessDays => "JoinBusinessDays(",
        };
        let mut out = String::from(prefix);
        for (i, c) in self.calendars.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&c.name());
        }
        out.push(')');
        out
    }

    fn is_weekend(&self, w: Weekday) -> bool {
        match self.rule {
            JointCalendarRule::JoinHolidays => self.calendars.iter().any(|c| c.is_weekend(w)),
            JointCalendarRule::JoinBusinessDays => self.calendars.iter().all(|c| c.is_weekend(w)),
        }
    }

    fn is_business_day(&self, date: Date) -> bool {
        match self.rule {
            JointCalendarRule::JoinHolidays => {
                self.calendars.iter().all(|c| c.is_business_day(date))
            }
            JointCalendarRule::JoinBusinessDays => {
                self.calendars.iter().any(|c| c.is_business_day(date))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::calendars::target::Target;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::Month;

    #[test]
    fn join_holidays_is_holiday_if_any_is() {
        // TARGET has Christmas as a holiday; WeekendsOnly does not. Under
        // JoinHolidays the joint calendar treats Christmas as a holiday.
        let joint = JointCalendar::of_two(
            Target::new(),
            WeekendsOnly::new(),
            JointCalendarRule::JoinHolidays,
        );
        let christmas = Date::new(25, Month::December, 2018); // Tuesday
        assert!(joint.is_holiday(christmas));
    }

    #[test]
    fn join_business_days_is_business_if_any_is() {
        // Under JoinBusinessDays, Christmas (a business day for WeekendsOnly)
        // stays a business day for the joint calendar.
        let joint = JointCalendar::of_two(
            Target::new(),
            WeekendsOnly::new(),
            JointCalendarRule::JoinBusinessDays,
        );
        let christmas = Date::new(25, Month::December, 2018);
        assert!(joint.is_business_day(christmas));
    }

    #[test]
    fn name_lists_members() {
        let joint = JointCalendar::of_two(
            Target::new(),
            WeekendsOnly::new(),
            JointCalendarRule::JoinHolidays,
        );
        assert_eq!(joint.name(), "JoinHolidays(TARGET, weekends only)");
    }
}
