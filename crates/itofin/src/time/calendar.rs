//! Calendar base: business-day queries and date rolling.
//!
//! Port of `ql/time/calendar.{hpp,cpp}`. QuantLib uses the Bridge pattern: a
//! `Calendar` value holds a `shared_ptr<Impl>`, and each concrete market
//! calendar subclasses `Impl` to answer [`CalendarImpl::is_business_day`]. Here
//! the same split is expressed with a [`CalendarImpl`] trait object behind an
//! [`Rc`](std::rc::Rc): [`Calendar`] holds the shared implementation plus the
//! per-instance added/removed holiday overrides.
//!
//! ## Divergences from QuantLib
//!
//! Both of the following are deliberate decisions made for this port (not
//! accidental), chosen by the maintainer.
//!
//! 1. **Holiday-set sharing.** In QuantLib every construction of, say,
//!    `TARGET()` shares one process-global `Impl` instance, so a holiday added
//!    through one handle is visible through an independently constructed one.
//!    That global mutable state conflicts with this port's "explicit state, no
//!    hidden singletons" design decision. Here the added/removed holiday sets
//!    are shared only among *clones* of a given [`Calendar`] value (they sit
//!    behind a shared [`RefCell`](std::cell::RefCell)), not across separately
//!    constructed calendars. The natural (built-in) holiday rules are identical;
//!    only the reach of `add_holiday`/`remove_holiday` differs.
//!
//! 2. **Date-aware weekend filtering in [`Calendar::holiday_list`].** QuantLib's
//!    `holidayList` filters weekends with the weekday-only `isWeekend`, which is
//!    wrong for markets whose weekend rule changed over time (Saudi Arabia,
//!    Israel/TASE): a holiday on a day that was a weekend *then* can be wrongly
//!    kept, or one on a then-business day wrongly dropped. This port filters
//!    with the date-aware [`CalendarImpl::is_weekend_on`] instead. Calendars
//!    with a fixed weekend are unaffected (the default `is_weekend_on` equals
//!    the weekday rule).

use std::collections::BTreeSet;
use std::fmt;

use crate::shared::{Shared, SharedMut, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::{Date, Day, SerialNumber, Year};
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::time::weekday::Weekday;
use crate::types::Integer;

/// The market-specific behaviour behind a [`Calendar`].
///
/// Implementors answer only the *natural* holiday schedule; the added/removed
/// overrides are layered on by [`Calendar`] itself. This mirrors QuantLib's
/// `Calendar::Impl`, minus the holiday sets (which live in [`Calendar`]).
pub trait CalendarImpl {
    /// The calendar's name, used for display and equality.
    fn name(&self) -> String;
    /// Whether `date` is a business day under the natural schedule.
    fn is_business_day(&self, date: Date) -> bool;
    /// Whether `weekday` is part of the weekend for this market.
    fn is_weekend(&self, weekday: Weekday) -> bool;
    /// Whether `date` falls on a weekend, allowing for markets whose weekend has
    /// changed over time (e.g. Saudi Arabia, Israel/TASE).
    ///
    /// Defaults to the weekday-only [`is_weekend`](Self::is_weekend); markets
    /// with a date-dependent weekend override this. Used by
    /// [`Calendar::holiday_list`] so weekend filtering is correct even across a
    /// weekend-rule change. This is a deliberate improvement over QuantLib,
    /// whose `holidayList` filters on the fixed weekday rule only.
    fn is_weekend_on(&self, date: Date) -> bool {
        self.is_weekend(date.weekday())
    }
}

/// Whether `w` is a Saturday or Sunday (the Western/Orthodox weekend).
pub fn is_weekend_sat_sun(w: Weekday) -> bool {
    w == Weekday::Saturday || w == Weekday::Sunday
}

/// A calendar for a market: business-day tests and date adjustment/advancement.
///
/// Cloning is cheap and shares both the implementation and the added/removed
/// holiday overrides (see the [module docs](self) for how this differs from
/// QuantLib).
#[derive(Clone)]
pub struct Calendar {
    imp: Shared<dyn CalendarImpl>,
    added_holidays: SharedMut<BTreeSet<Date>>,
    removed_holidays: SharedMut<BTreeSet<Date>>,
}

impl Calendar {
    /// Wraps a concrete [`CalendarImpl`] into a usable calendar. Concrete
    /// market calendars call this from their own constructors.
    pub fn from_impl(imp: Shared<dyn CalendarImpl>) -> Calendar {
        Calendar {
            imp,
            added_holidays: shared_mut(BTreeSet::new()),
            removed_holidays: shared_mut(BTreeSet::new()),
        }
    }

    /// The name of the calendar.
    pub fn name(&self) -> String {
        self.imp.name()
    }

    /// Whether `d` is a business day for this market, honouring any
    /// added/removed holiday overrides.
    pub fn is_business_day(&self, d: Date) -> bool {
        if self.added_holidays.borrow().contains(&d) {
            return false;
        }
        if self.removed_holidays.borrow().contains(&d) {
            return true;
        }
        self.imp.is_business_day(d)
    }

    /// Whether `d` is a holiday for this market (the negation of
    /// [`is_business_day`](Self::is_business_day)).
    pub fn is_holiday(&self, d: Date) -> bool {
        !self.is_business_day(d)
    }

    /// Whether `w` is part of the weekend for this market.
    pub fn is_weekend(&self, w: Weekday) -> bool {
        self.imp.is_weekend(w)
    }

    /// Whether `d` falls on a weekend, honouring markets whose weekend rule has
    /// changed over time (Saudi Arabia, Israel/TASE). For markets with a fixed
    /// weekend this equals [`is_weekend`](Self::is_weekend) of `d.weekday()`.
    pub fn is_weekend_on(&self, d: Date) -> bool {
        self.imp.is_weekend_on(d)
    }

    /// The set of holidays added on top of the natural schedule.
    pub fn added_holidays(&self) -> BTreeSet<Date> {
        self.added_holidays.borrow().clone()
    }

    /// The set of natural holidays removed from the schedule.
    pub fn removed_holidays(&self) -> BTreeSet<Date> {
        self.removed_holidays.borrow().clone()
    }

    /// Clears every added and removed holiday override.
    pub fn reset_added_and_removed_holidays(&self) {
        self.added_holidays.borrow_mut().clear();
        self.removed_holidays.borrow_mut().clear();
    }

    /// Adds `d` to the set of holidays for this calendar.
    pub fn add_holiday(&self, d: Date) {
        // if d was a genuine holiday previously removed, revert the change
        self.removed_holidays.borrow_mut().remove(&d);
        // if it's already a holiday, leave the calendar alone; otherwise add it
        if self.imp.is_business_day(d) {
            self.added_holidays.borrow_mut().insert(d);
        }
    }

    /// Removes `d` from the set of holidays for this calendar.
    pub fn remove_holiday(&self, d: Date) {
        // if d was an artificially-added holiday, revert the change
        self.added_holidays.borrow_mut().remove(&d);
        // if it's already a business day, leave the calendar alone; else record
        if !self.imp.is_business_day(d) {
            self.removed_holidays.borrow_mut().insert(d);
        }
    }

    /// Whether, in this market, `d` is on or before the first business day of
    /// its month.
    pub fn is_start_of_month(&self, d: Date) -> bool {
        d <= self.start_of_month(d)
    }

    /// The first business day of the month `d` belongs to.
    pub fn start_of_month(&self, d: Date) -> Date {
        self.adjust(Date::start_of_month(d), BusinessDayConvention::Following)
    }

    /// Whether, in this market, `d` is on or after the last business day of its
    /// month.
    pub fn is_end_of_month(&self, d: Date) -> bool {
        d >= self.end_of_month(d)
    }

    /// The last business day of the month `d` belongs to.
    pub fn end_of_month(&self, d: Date) -> Date {
        self.adjust(Date::end_of_month(d), BusinessDayConvention::Preceding)
    }

    /// The holidays between `from` and `to` (inclusive).
    ///
    /// Weekends are included only when `include_weekends` is set.
    ///
    /// # Divergence from QuantLib (chosen for this port)
    ///
    /// The weekend filter uses the *date-aware*
    /// [`is_weekend_on`](Self::is_weekend_on) rather than the fixed weekday rule.
    /// QuantLib's `holidayList` filters on the weekday-only `isWeekend`, so for
    /// markets whose weekend changed over time (Saudi Arabia's Thu/Fri->Fri/Sat
    /// in 2013, Israel/TASE's Fri/Sat->Sat/Sun in 2026) it can wrongly keep a
    /// holiday that fell on a then-weekend day, or drop one that fell on a
    /// then-business day. This port deliberately fixes that; every calendar with
    /// a fixed weekend is unaffected (the default `is_weekend_on` is identical to
    /// the weekday rule).
    pub fn holiday_list(&self, from: Date, to: Date, include_weekends: bool) -> Vec<Date> {
        let mut result = Vec::new();
        let mut d = from;
        while d <= to {
            if self.is_holiday(d) && (include_weekends || !self.is_weekend_on(d)) {
                result.push(d);
            }
            // Stop before incrementing past the inclusive endpoint; otherwise
            // `to == Date::max_date()` would push the serial out of range.
            if d == to {
                break;
            }
            d += 1;
        }
        result
    }

    /// The business days between `from` and `to` (inclusive).
    pub fn business_day_list(&self, from: Date, to: Date) -> Vec<Date> {
        let mut result = Vec::new();
        let mut d = from;
        while d <= to {
            if self.is_business_day(d) {
                result.push(d);
            }
            // Stop before incrementing past the inclusive endpoint; otherwise
            // `to == Date::max_date()` would push the serial out of range.
            if d == to {
                break;
            }
            d += 1;
        }
        result
    }

    /// Rolls `d` to the nearest business day per the given convention.
    ///
    /// # Panics
    ///
    /// Panics if `d` is the null date, or on an unknown convention.
    pub fn adjust(&self, d: Date, c: BusinessDayConvention) -> Date {
        use BusinessDayConvention::*;
        assert!(d != Date::null(), "null date");

        if c == Unadjusted {
            return d;
        }

        let mut d1 = d;
        if c == Following || c == ModifiedFollowing || c == HalfMonthModifiedFollowing {
            while self.is_holiday(d1) {
                d1 += 1;
            }
            if c == ModifiedFollowing || c == HalfMonthModifiedFollowing {
                if d1.month() != d.month() {
                    return self.adjust(d, Preceding);
                }
                if c == HalfMonthModifiedFollowing
                    && d.day_of_month() <= 15
                    && d1.day_of_month() > 15
                {
                    return self.adjust(d, Preceding);
                }
            }
            d1
        } else if c == Preceding || c == ModifiedPreceding {
            while self.is_holiday(d1) {
                d1 -= 1;
            }
            if c == ModifiedPreceding && d1.month() != d.month() {
                return self.adjust(d, Following);
            }
            d1
        } else if c == Nearest {
            let mut d2 = d;
            while self.is_holiday(d1) && self.is_holiday(d2) {
                d1 += 1;
                d2 -= 1;
            }
            if self.is_holiday(d1) { d2 } else { d1 }
        } else {
            panic!("unknown business-day convention");
        }
    }

    /// Advances `d` by `n` units, adjusting the result per the convention.
    ///
    /// For `Days`, each intermediate day is skipped over holidays; for `Weeks`
    /// the shift is applied then adjusted; for `Months`/`Years` the shift is
    /// applied then adjusted, with `end_of_month` snapping preserved.
    ///
    /// # Panics
    ///
    /// Panics if `d` is the null date.
    pub fn advance(
        &self,
        d: Date,
        mut n: Integer,
        unit: TimeUnit,
        c: BusinessDayConvention,
        end_of_month: bool,
    ) -> Date {
        assert!(d != Date::null(), "null date");

        if n == 0 {
            return self.adjust(d, c);
        }

        match unit {
            TimeUnit::Days => {
                let mut d1 = d;
                if n > 0 {
                    while n > 0 {
                        d1 += 1;
                        while self.is_holiday(d1) {
                            d1 += 1;
                        }
                        n -= 1;
                    }
                } else {
                    while n < 0 {
                        d1 -= 1;
                        while self.is_holiday(d1) {
                            d1 -= 1;
                        }
                        n += 1;
                    }
                }
                d1
            }
            TimeUnit::Weeks => {
                let d1 = d + Period::new(n, unit);
                self.adjust(d1, c)
            }
            _ => {
                // Months or Years.
                let d1 = d + Period::new(n, unit);
                if end_of_month {
                    if c == BusinessDayConvention::Unadjusted {
                        if Date::is_end_of_month(d) {
                            return Date::end_of_month(d1);
                        }
                    } else if self.is_end_of_month(d) {
                        return self.end_of_month(d1);
                    }
                }
                self.adjust(d1, c)
            }
        }
    }

    /// Advances `d` by the given [`Period`], adjusting per the convention.
    pub fn advance_by_period(
        &self,
        d: Date,
        p: Period,
        c: BusinessDayConvention,
        end_of_month: bool,
    ) -> Date {
        self.advance(d, p.length(), p.units(), c, end_of_month)
    }

    /// The number of business days between `from` and `to`.
    ///
    /// `include_first`/`include_last` control whether the endpoints count. When
    /// `from > to` the result is negated, matching QuantLib.
    pub fn business_days_between(
        &self,
        from: Date,
        to: Date,
        include_first: bool,
        include_last: bool,
    ) -> SerialNumber {
        if from < to {
            days_between_impl(self, from, to, include_first, include_last)
        } else if from > to {
            -days_between_impl(self, to, from, include_last, include_first)
        } else {
            SerialNumber::from(include_first && include_last && self.is_business_day(from))
        }
    }
}

impl fmt::Debug for Calendar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Calendar")
            .field("name", &self.name())
            .finish()
    }
}

impl fmt::Display for Calendar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl PartialEq for Calendar {
    /// Two calendars are equal iff they share the same name, matching
    /// QuantLib's `operator==`.
    fn eq(&self, other: &Calendar) -> bool {
        self.name() == other.name()
    }
}

impl Eq for Calendar {}

// Requires: from < to.
fn days_between_impl(
    cal: &Calendar,
    from: Date,
    to: Date,
    include_first: bool,
    include_last: bool,
) -> SerialNumber {
    let mut res = SerialNumber::from(include_last && cal.is_business_day(to));
    let mut d = if include_first { from } else { from + 1 };
    while d < to {
        res += SerialNumber::from(cal.is_business_day(d));
        d += 1;
    }
    res
}

/// The Western (Gregorian) Easter Monday, as a one-based day of the year.
///
/// Table copied verbatim from `Calendar::WesternImpl::easterMonday` in
/// `ql/time/calendar.cpp`, indexed by `year - 1901`.
pub fn western_easter_monday(y: Year) -> Day {
    const EASTER_MONDAY: [u8; 299] = [
        98, 90, 103, 95, 114, 106, 91, 111, 102, // 1901-1909
        87, 107, 99, 83, 103, 95, 115, 99, 91, 111, // 1910-1919
        96, 87, 107, 92, 112, 103, 95, 108, 100, 91, // 1920-1929
        111, 96, 88, 107, 92, 112, 104, 88, 108, 100, // 1930-1939
        85, 104, 96, 116, 101, 92, 112, 97, 89, 108, // 1940-1949
        100, 85, 105, 96, 109, 101, 93, 112, 97, 89, // 1950-1959
        109, 93, 113, 105, 90, 109, 101, 86, 106, 97, // 1960-1969
        89, 102, 94, 113, 105, 90, 110, 101, 86, 106, // 1970-1979
        98, 110, 102, 94, 114, 98, 90, 110, 95, 86, // 1980-1989
        106, 91, 111, 102, 94, 107, 99, 90, 103, 95, // 1990-1999
        115, 106, 91, 111, 103, 87, 107, 99, 84, 103, // 2000-2009
        95, 115, 100, 91, 111, 96, 88, 107, 92, 112, // 2010-2019
        104, 95, 108, 100, 92, 111, 96, 88, 108, 92, // 2020-2029
        112, 104, 89, 108, 100, 85, 105, 96, 116, 101, // 2030-2039
        93, 112, 97, 89, 109, 100, 85, 105, 97, 109, // 2040-2049
        101, 93, 113, 97, 89, 109, 94, 113, 105, 90, // 2050-2059
        110, 101, 86, 106, 98, 89, 102, 94, 114, 105, // 2060-2069
        90, 110, 102, 86, 106, 98, 111, 102, 94, 114, // 2070-2079
        99, 90, 110, 95, 87, 106, 91, 111, 103, 94, // 2080-2089
        107, 99, 91, 103, 95, 115, 107, 91, 111, 103, // 2090-2099
        88, 108, 100, 85, 105, 96, 109, 101, 93, 112, // 2100-2109
        97, 89, 109, 93, 113, 105, 90, 109, 101, 86, // 2110-2119
        106, 97, 89, 102, 94, 113, 105, 90, 110, 101, // 2120-2129
        86, 106, 98, 110, 102, 94, 114, 98, 90, 110, // 2130-2139
        95, 86, 106, 91, 111, 102, 94, 107, 99, 90, // 2140-2149
        103, 95, 115, 106, 91, 111, 103, 87, 107, 99, // 2150-2159
        84, 103, 95, 115, 100, 91, 111, 96, 88, 107, // 2160-2169
        92, 112, 104, 95, 108, 100, 92, 111, 96, 88, // 2170-2179
        108, 92, 112, 104, 89, 108, 100, 85, 105, 96, // 2180-2189
        116, 101, 93, 112, 97, 89, 109, 100, 85, 105, // 2190-2199
    ];
    Day::from(EASTER_MONDAY[(y - 1901) as usize])
}
/// The Orthodox Easter Monday, as a one-based day of the year.
///
/// Table copied verbatim from `Calendar::OrthodoxImpl::easterMonday` in
/// `ql/time/calendar.cpp`, indexed by `year - 1901`.
pub fn orthodox_easter_monday(y: Year) -> Day {
    const EASTER_MONDAY: [u8; 299] = [
        105, 118, 110, 102, 121, 106, 126, 118, 102, // 1901-1909
        122, 114, 99, 118, 110, 95, 115, 106, 126, 111, // 1910-1919
        103, 122, 107, 99, 119, 110, 123, 115, 107, 126, // 1920-1929
        111, 103, 123, 107, 99, 119, 104, 123, 115, 100, // 1930-1939
        120, 111, 96, 116, 108, 127, 112, 104, 124, 115, // 1940-1949
        100, 120, 112, 96, 116, 108, 128, 112, 104, 124, // 1950-1959
        109, 100, 120, 105, 125, 116, 101, 121, 113, 104, // 1960-1969
        117, 109, 101, 120, 105, 125, 117, 101, 121, 113, // 1970-1979
        98, 117, 109, 129, 114, 105, 125, 110, 102, 121, // 1980-1989
        106, 98, 118, 109, 122, 114, 106, 118, 110, 102, // 1990-1999
        122, 106, 126, 118, 103, 122, 114, 99, 119, 110, // 2000-2009
        95, 115, 107, 126, 111, 103, 123, 107, 99, 119, // 2010-2019
        111, 123, 115, 107, 127, 111, 103, 123, 108, 99, // 2020-2029
        119, 104, 124, 115, 100, 120, 112, 96, 116, 108, // 2030-2039
        128, 112, 104, 124, 116, 100, 120, 112, 97, 116, // 2040-2049
        108, 128, 113, 104, 124, 109, 101, 120, 105, 125, // 2050-2059
        117, 101, 121, 113, 105, 117, 109, 101, 121, 105, // 2060-2069
        125, 110, 102, 121, 113, 98, 118, 109, 129, 114, // 2070-2079
        106, 125, 110, 102, 122, 106, 98, 118, 110, 122, // 2080-2089
        114, 99, 119, 110, 102, 115, 107, 126, 118, 103, // 2090-2099
        123, 115, 100, 120, 112, 96, 116, 108, 128, 112, // 2100-2109
        104, 124, 109, 100, 120, 105, 125, 116, 108, 121, // 2110-2119
        113, 104, 124, 109, 101, 120, 105, 125, 117, 101, // 2120-2129
        121, 113, 98, 117, 109, 129, 114, 105, 125, 110, // 2130-2139
        102, 121, 113, 98, 118, 109, 129, 114, 106, 125, // 2140-2149
        110, 102, 122, 106, 126, 118, 103, 122, 114, 99, // 2150-2159
        119, 110, 102, 115, 107, 126, 111, 103, 123, 114, // 2160-2169
        99, 119, 111, 130, 115, 107, 127, 111, 103, 123, // 2170-2179
        108, 99, 119, 104, 124, 115, 100, 120, 112, 103, // 2180-2189
        116, 108, 128, 119, 104, 124, 116, 100, 120, 112, // 2190-2199
    ];
    Day::from(EASTER_MONDAY[(y - 1901) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;
    use crate::time::date::Month;

    /// A weekends-only test calendar (Saturday/Sunday), used to exercise the
    /// shared base logic without depending on a concrete market calendar.
    struct WeekendsCal;
    impl CalendarImpl for WeekendsCal {
        fn name(&self) -> String {
            "Weekends test".to_string()
        }
        fn is_business_day(&self, date: Date) -> bool {
            !is_weekend_sat_sun(date.weekday())
        }
        fn is_weekend(&self, weekday: Weekday) -> bool {
            is_weekend_sat_sun(weekday)
        }
    }

    fn cal() -> Calendar {
        Calendar::from_impl(shared(WeekendsCal))
    }

    /// A calendar whose weekend changed on 29 June 2013 (Thu/Fri before,
    /// Fri/Sat after), used to check that `holiday_list` filters weekends by the
    /// date-aware rule, not the fixed weekday rule.
    struct SwitchingWeekendCal;
    impl CalendarImpl for SwitchingWeekendCal {
        fn name(&self) -> String {
            "switching test".to_string()
        }
        fn is_weekend(&self, w: Weekday) -> bool {
            // Fixed rule (matches the "current" weekend), as QuantLib's
            // weekday-only isWeekend would report.
            w == Weekday::Friday || w == Weekday::Saturday
        }
        fn is_weekend_on(&self, date: Date) -> bool {
            let w = date.weekday();
            if date < Date::new(29, Month::June, 2013) {
                w == Weekday::Thursday || w == Weekday::Friday
            } else {
                w == Weekday::Friday || w == Weekday::Saturday
            }
        }
        fn is_business_day(&self, date: Date) -> bool {
            !self.is_weekend_on(date)
        }
    }

    #[test]
    fn holiday_list_uses_date_aware_weekend() {
        let c = Calendar::from_impl(shared(SwitchingWeekendCal));
        // Thursday 27 June 2013 - a weekend day *before* the switch, but the
        // fixed weekday rule (Fri/Sat) would not call Thursday a weekend.
        let thu = Date::new(27, Month::June, 2013);
        assert_eq!(thu.weekday(), Weekday::Thursday);
        assert!(c.is_weekend_on(thu));
        assert!(!c.is_weekend(thu.weekday())); // fixed rule disagrees

        // With include_weekends = false, the date-aware filter excludes it;
        // the old fixed-weekday filter would have wrongly kept it.
        assert!(c.holiday_list(thu, thu, false).is_empty());
        // With include_weekends = true it is listed.
        assert_eq!(c.holiday_list(thu, thu, true), vec![thu]);
    }

    #[test]
    fn weekend_days_are_holidays() {
        let c = cal();
        // Saturday January 1st 2000.
        let sat = Date::new(1, Month::January, 2000);
        assert_eq!(sat.weekday(), Weekday::Saturday);
        assert!(c.is_holiday(sat));
        assert!(c.is_business_day(Date::new(3, Month::January, 2000))); // Monday
    }

    #[test]
    fn adjust_following_and_preceding() {
        let c = cal();
        let sat = Date::new(1, Month::January, 2000); // Saturday
        assert_eq!(
            c.adjust(sat, BusinessDayConvention::Following),
            Date::new(3, Month::January, 2000) // Monday
        );
        assert_eq!(
            c.adjust(sat, BusinessDayConvention::Preceding),
            Date::new(31, Month::December, 1999) // Friday
        );
    }

    #[test]
    fn adjust_modified_following_rolls_back_across_month() {
        let c = cal();
        // Saturday July 31st 2021: Following would cross into August, so
        // ModifiedFollowing rolls back to Friday July 30th.
        let sat = Date::new(31, Month::July, 2021);
        assert_eq!(sat.weekday(), Weekday::Saturday);
        assert_eq!(
            c.adjust(sat, BusinessDayConvention::ModifiedFollowing),
            Date::new(30, Month::July, 2021)
        );
    }

    #[test]
    fn advance_days_skips_weekends() {
        let c = cal();
        // Friday + 1 business day -> Monday.
        let fri = Date::new(7, Month::January, 2000);
        assert_eq!(fri.weekday(), Weekday::Friday);
        let next = c.advance(
            fri,
            1,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        assert_eq!(next, Date::new(10, Month::January, 2000));
    }

    #[test]
    fn business_days_between_counts_weekdays() {
        let c = cal();
        let mon = Date::new(3, Month::January, 2000);
        let next_mon = Date::new(10, Month::January, 2000);
        // (mon, next_mon] excluding first, including last: Tue-Fri + Mon = 5.
        assert_eq!(c.business_days_between(mon, next_mon, false, true), 5);
    }

    #[test]
    fn add_and_remove_holiday_are_local_and_reversible() {
        let c = cal();
        let wed = Date::new(5, Month::January, 2000); // business day
        assert!(c.is_business_day(wed));
        c.add_holiday(wed);
        assert!(c.is_holiday(wed));
        assert!(c.added_holidays().contains(&wed));

        // A clone shares the override...
        let clone = c.clone();
        assert!(clone.is_holiday(wed));
        // ...but an independently constructed calendar does not (documented
        // divergence from QuantLib's process-global sharing).
        assert!(cal().is_business_day(wed));

        c.remove_holiday(wed);
        assert!(c.is_business_day(wed));
    }

    #[test]
    fn holiday_and_business_day_lists_handle_max_date_endpoint() {
        let c = cal();
        // A `to` of `Date::max_date()` must not panic when the loop reaches the
        // inclusive endpoint (previously `d += 1` pushed the serial past range).
        let from = Date::max_date() - 3;
        let holidays = c.holiday_list(from, Date::max_date(), true);
        let business = c.business_day_list(from, Date::max_date());
        // Every day in the 4-day inclusive window is either a holiday or a
        // business day, so the two lists partition it.
        assert_eq!(holidays.len() + business.len(), 4);

        // from > to yields an empty list; from == to yields the single day.
        assert!(c.holiday_list(from, from - 1, true).is_empty());
        assert!(c.business_day_list(from, from - 1).is_empty());
        assert_eq!(
            c.business_day_list(from, from).len() + c.holiday_list(from, from, true).len(),
            1
        );
    }

    #[test]
    fn easter_monday_known_values() {
        // Western Easter Monday 2000 was April 24th -> day of year 115.
        assert_eq!(western_easter_monday(2000), 115);
        // Orthodox Easter Monday 2000 was May 1st -> day of year 122.
        assert_eq!(orthodox_easter_monday(2000), 122);
    }
}
