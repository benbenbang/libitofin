//! Payment schedules.
//!
//! Port of `ql/time/schedule.hpp` and `ql/time/schedule.cpp`: the
//! [`Schedule`] date sequence with its meta information (tenor, calendar,
//! business-day conventions, generation rule), plus the free helpers
//! [`previous_twentieth`] and [`allows_end_of_month`] shared with the CDS
//! machinery.
//!
//! A `Date` equal to [`Date::null()`] plays the role of QuantLib's
//! default-constructed `Date` for the optional first and next-to-last stub
//! dates.

use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;

/// A payment schedule: a non-decreasing sequence of dates plus the meta
/// information used to build it, when available.
#[derive(Clone)]
pub struct Schedule {
    tenor: Option<Period>,
    calendar: Calendar,
    convention: BusinessDayConvention,
    termination_date_convention: Option<BusinessDayConvention>,
    rule: Option<DateGeneration>,
    end_of_month: Option<bool>,
    dates: Vec<Date>,
    is_regular: Vec<bool>,
}

impl Schedule {
    /// Builds a schedule from a plain list of dates, with no meta
    /// information: a null calendar and the `Unadjusted` convention.
    pub fn from_dates(dates: Vec<Date>) -> Schedule {
        Schedule::with_metadata(
            dates,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            None,
            None,
            None,
            None,
            Vec::new(),
        )
    }

    /// Builds a schedule from any list of dates, plus meta information that
    /// client classes can use. Neither the dates nor the meta information
    /// are checked for plausibility, matching QuantLib.
    ///
    /// # Panics
    ///
    /// Panics if `is_regular` is non-empty and its length differs from
    /// `dates.len() - 1`.
    #[allow(clippy::too_many_arguments)]
    pub fn with_metadata(
        dates: Vec<Date>,
        calendar: Calendar,
        convention: BusinessDayConvention,
        termination_date_convention: Option<BusinessDayConvention>,
        tenor: Option<Period>,
        rule: Option<DateGeneration>,
        end_of_month: Option<bool>,
        is_regular: Vec<bool>,
    ) -> Schedule {
        let end_of_month = match tenor {
            Some(t) if !allows_end_of_month(t) => Some(false),
            _ => end_of_month,
        };
        assert!(
            is_regular.is_empty() || is_regular.len() == dates.len().saturating_sub(1),
            "isRegular size ({}) must be zero or equal to the number of dates minus 1 ({})",
            is_regular.len(),
            dates.len().saturating_sub(1)
        );
        Schedule {
            tenor,
            calendar,
            convention,
            termination_date_convention,
            rule,
            end_of_month,
            dates,
            is_regular,
        }
    }

    /// The number of dates.
    pub fn len(&self) -> usize {
        self.dates.len()
    }

    /// Whether the schedule holds no dates.
    pub fn is_empty(&self) -> bool {
        self.dates.is_empty()
    }

    /// The `i`-th date.
    ///
    /// # Panics
    ///
    /// Panics if `i` is out of range.
    pub fn date(&self, i: usize) -> Date {
        self.dates[i]
    }

    /// All the dates.
    pub fn dates(&self) -> &[Date] {
        &self.dates
    }

    /// The first date.
    ///
    /// # Panics
    ///
    /// Panics if the schedule is empty.
    pub fn start_date(&self) -> Date {
        assert!(!self.dates.is_empty(), "empty Schedule: no start date");
        self.dates[0]
    }

    /// The last date.
    ///
    /// # Panics
    ///
    /// Panics if the schedule is empty.
    pub fn end_date(&self) -> Date {
        assert!(!self.dates.is_empty(), "empty Schedule: no end date");
        self.dates[self.dates.len() - 1]
    }

    /// The calendar used to build the schedule.
    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// Whether the tenor is part of the meta information.
    pub fn has_tenor(&self) -> bool {
        self.tenor.is_some()
    }

    /// The tenor used to build the schedule.
    ///
    /// # Panics
    ///
    /// Panics if the tenor is not part of the meta information.
    pub fn tenor(&self) -> Period {
        self.tenor.expect("full interface (tenor) not available")
    }

    /// The business-day convention used to build the schedule.
    pub fn business_day_convention(&self) -> BusinessDayConvention {
        self.convention
    }

    /// Whether the termination-date convention is part of the meta
    /// information.
    pub fn has_termination_date_business_day_convention(&self) -> bool {
        self.termination_date_convention.is_some()
    }

    /// The business-day convention used for the termination date.
    ///
    /// # Panics
    ///
    /// Panics if the convention is not part of the meta information.
    pub fn termination_date_business_day_convention(&self) -> BusinessDayConvention {
        self.termination_date_convention
            .expect("full interface (termination date bdc) not available")
    }

    /// Whether the date-generation rule is part of the meta information.
    pub fn has_rule(&self) -> bool {
        self.rule.is_some()
    }

    /// The date-generation rule used to build the schedule.
    ///
    /// # Panics
    ///
    /// Panics if the rule is not part of the meta information.
    pub fn rule(&self) -> DateGeneration {
        self.rule.expect("full interface (rule) not available")
    }

    /// Whether the end-of-month flag is part of the meta information.
    pub fn has_end_of_month(&self) -> bool {
        self.end_of_month.is_some()
    }

    /// The end-of-month flag used to build the schedule.
    ///
    /// # Panics
    ///
    /// Panics if the flag is not part of the meta information.
    pub fn end_of_month(&self) -> bool {
        self.end_of_month
            .expect("full interface (end of month) not available")
    }

    /// Whether the period regularity flags are part of the meta information.
    pub fn has_is_regular(&self) -> bool {
        !self.is_regular.is_empty()
    }

    /// The regularity flags of all periods.
    ///
    /// # Panics
    ///
    /// Panics if the flags are not part of the meta information.
    pub fn is_regular(&self) -> &[bool] {
        assert!(
            !self.is_regular.is_empty(),
            "full interface (isRegular) not available"
        );
        &self.is_regular
    }

    /// Whether the `i`-th period is regular, with `i` starting at 1 as in
    /// QuantLib.
    ///
    /// # Panics
    ///
    /// Panics if the flags are not part of the meta information or `i` is
    /// out of the range `[1, number of periods]`.
    pub fn is_regular_at(&self, i: usize) -> bool {
        assert!(
            !self.is_regular.is_empty(),
            "full interface (isRegular) not available"
        );
        assert!(
            i >= 1 && i <= self.is_regular.len(),
            "index ({}) must be in [1, {}]",
            i,
            self.is_regular.len()
        );
        self.is_regular[i - 1]
    }
}

impl std::ops::Index<usize> for Schedule {
    type Output = Date;

    fn index(&self, i: usize) -> &Date {
        &self.dates[i]
    }
}

/// The date on or before `d` that is the 20th of the month, snapped back to
/// the previous main IMM month (March, June, September, December) when the
/// generation rule calls for it.
pub fn previous_twentieth(d: Date, rule: DateGeneration) -> Date {
    let mut result = Date::new(20, d.month(), d.year());
    if result > d {
        result = result - Period::new(1, TimeUnit::Months);
    }
    if matches!(
        rule,
        DateGeneration::TwentiethIMM
            | DateGeneration::OldCDS
            | DateGeneration::CDS
            | DateGeneration::CDS2015
    ) {
        let m = result.month().ordinal();
        if m % 3 != 0 {
            result = result - Period::new(m % 3, TimeUnit::Months);
        }
    }
    result
}

/// Whether a tenor supports end-of-month adjustment: a period of at least
/// one month expressed in `Months` or `Years` units.
pub fn allows_end_of_month(tenor: Period) -> bool {
    (tenor.units() == TimeUnit::Months || tenor.units() == TimeUnit::Years)
        && tenor >= Period::new(1, TimeUnit::Months)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Day, Month, Year};

    fn d(day: Day, m: Month, y: Year) -> Date {
        Date::new(day, m, y)
    }

    #[test]
    fn date_constructor_without_meta() {
        let dates = vec![
            d(16, Month::May, 2015),
            d(18, Month::May, 2015),
            d(18, Month::May, 2016),
            d(31, Month::December, 2017),
        ];

        let schedule = Schedule::from_dates(dates.clone());
        assert_eq!(schedule.len(), dates.len());
        for (i, expected) in dates.iter().enumerate() {
            assert_eq!(schedule[i], *expected, "at position {i}");
        }
        assert_eq!(*schedule.calendar(), NullCalendar::new());
        assert_eq!(
            schedule.business_day_convention(),
            BusinessDayConvention::Unadjusted
        );
        assert!(!schedule.has_tenor());
        assert!(!schedule.has_rule());
        assert!(!schedule.has_is_regular());
    }

    #[test]
    fn date_constructor_with_meta() {
        let dates = vec![
            d(16, Month::May, 2015),
            d(18, Month::May, 2015),
            d(18, Month::May, 2016),
            d(31, Month::December, 2017),
        ];
        let regular = vec![false, true, false];

        let schedule = Schedule::with_metadata(
            dates.clone(),
            Target::new(),
            BusinessDayConvention::Following,
            Some(BusinessDayConvention::ModifiedPreceding),
            Some(Period::new(1, TimeUnit::Years)),
            Some(DateGeneration::Backward),
            Some(true),
            regular.clone(),
        );
        for i in 1..dates.len() {
            assert_eq!(schedule.is_regular_at(i), regular[i - 1], "period {i}");
        }
        assert_eq!(*schedule.calendar(), Target::new());
        assert_eq!(
            schedule.business_day_convention(),
            BusinessDayConvention::Following
        );
        assert_eq!(
            schedule.termination_date_business_day_convention(),
            BusinessDayConvention::ModifiedPreceding
        );
        assert_eq!(schedule.tenor(), Period::new(1, TimeUnit::Years));
        assert_eq!(schedule.rule(), DateGeneration::Backward);
        assert!(schedule.end_of_month());
    }

    #[test]
    fn previous_twentieth_snaps_to_imm_months() {
        assert_eq!(
            previous_twentieth(d(19, Month::March, 2016), DateGeneration::CDS),
            d(20, Month::December, 2015)
        );
        assert_eq!(
            previous_twentieth(d(19, Month::March, 2016), DateGeneration::Twentieth),
            d(20, Month::February, 2016)
        );
        assert_eq!(
            previous_twentieth(d(21, Month::March, 2016), DateGeneration::CDS2015),
            d(20, Month::March, 2016)
        );
    }

    #[test]
    fn allows_end_of_month_cases() {
        assert!(allows_end_of_month(Period::new(1, TimeUnit::Months)));
        assert!(allows_end_of_month(Period::new(2, TimeUnit::Years)));
        assert!(!allows_end_of_month(Period::new(4, TimeUnit::Weeks)));
        assert!(!allows_end_of_month(Period::new(30, TimeUnit::Days)));
        assert!(!allows_end_of_month(Period::new(0, TimeUnit::Months)));
    }
}
