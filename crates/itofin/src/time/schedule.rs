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
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::time::weekday::Weekday;
use crate::types::Integer;

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
    first_date: Date,
    next_to_last_date: Date,
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
            first_date: Date::null(),
            next_to_last_date: Date::null(),
            dates,
            is_regular,
        }
    }

    /// Builds a schedule from a date-generation rule. Pass [`Date::null()`]
    /// to omit the optional `first_date`/`next_to_last_date` stub dates.
    ///
    /// # Panics
    ///
    /// Panics on a null or inconsistent date range, a negative tenor, a
    /// stub date or end-of-month flag incompatible with the rule, or a
    /// degenerate single-date result; also, unlike QuantLib, on a null
    /// effective date (never inferred from the evaluation date).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        effective_date: Date,
        termination_date: Date,
        mut tenor: Period,
        calendar: Calendar,
        convention: BusinessDayConvention,
        termination_date_convention: BusinessDayConvention,
        rule: DateGeneration,
        end_of_month: bool,
        first_date: Date,
        next_to_last_date: Date,
    ) -> Schedule {
        let end_of_month = allows_end_of_month(tenor) && end_of_month;
        let first_date = if first_date == effective_date {
            Date::null()
        } else {
            first_date
        };
        let next_to_last_date = if next_to_last_date == termination_date {
            Date::null()
        } else {
            next_to_last_date
        };

        assert!(termination_date != Date::null(), "null termination date");
        assert!(effective_date != Date::null(), "null effective date");
        assert!(
            effective_date < termination_date,
            "effective date ({effective_date}) later than or equal to termination date ({termination_date})"
        );

        let rule = if tenor.length() == 0 {
            DateGeneration::Zero
        } else {
            assert!(
                tenor.length() > 0,
                "non positive tenor ({tenor}) not allowed"
            );
            rule
        };

        if first_date != Date::null() {
            match rule {
                DateGeneration::Backward | DateGeneration::Forward => assert!(
                    first_date > effective_date && first_date <= termination_date,
                    "first date ({first_date}) out of effective-termination date range ({effective_date}, {termination_date}]"
                ),
                DateGeneration::ThirdWednesday => assert!(
                    is_imm_date(first_date),
                    "first date ({first_date}) is not an IMM date"
                ),
                DateGeneration::Zero
                | DateGeneration::Twentieth
                | DateGeneration::TwentiethIMM
                | DateGeneration::OldCDS
                | DateGeneration::CDS
                | DateGeneration::CDS2015 => {
                    panic!("first date incompatible with {rule} date generation rule")
                }
                DateGeneration::ThirdWednesdayInclusive => panic!("unknown rule ({rule})"),
            }
        }
        if next_to_last_date != Date::null() {
            match rule {
                DateGeneration::Backward | DateGeneration::Forward => assert!(
                    next_to_last_date >= effective_date && next_to_last_date < termination_date,
                    "next to last date ({next_to_last_date}) out of effective-termination date range [{effective_date}, {termination_date})"
                ),
                DateGeneration::ThirdWednesday => assert!(
                    is_imm_date(next_to_last_date),
                    "next-to-last date ({next_to_last_date}) is not an IMM date"
                ),
                DateGeneration::Zero
                | DateGeneration::Twentieth
                | DateGeneration::TwentiethIMM
                | DateGeneration::OldCDS
                | DateGeneration::CDS
                | DateGeneration::CDS2015 => {
                    panic!("next to last date incompatible with {rule} date generation rule")
                }
                DateGeneration::ThirdWednesdayInclusive => panic!("unknown rule ({rule})"),
            }
        }

        let null_calendar = NullCalendar::new();
        let mut periods: Integer = 1;
        let mut seed = Date::null();
        let mut dates: Vec<Date> = Vec::new();
        let mut is_regular: Vec<bool> = Vec::new();

        match rule {
            DateGeneration::Zero => {
                tenor = Period::new(0, TimeUnit::Years);
                dates.push(effective_date);
                dates.push(termination_date);
                is_regular.push(true);
            }

            DateGeneration::Backward => {
                dates.push(termination_date);

                seed = termination_date;
                if next_to_last_date != Date::null() {
                    dates.push(next_to_last_date);
                    let temp = null_calendar.advance_by_period(
                        seed,
                        -(periods * tenor),
                        convention,
                        end_of_month,
                    );
                    is_regular.push(temp == next_to_last_date);
                    seed = next_to_last_date;
                }

                let mut exit_date = effective_date;
                if first_date != Date::null() {
                    exit_date = first_date;
                }

                loop {
                    let temp = null_calendar.advance_by_period(
                        seed,
                        -(periods * tenor),
                        convention,
                        end_of_month,
                    );
                    if temp < exit_date {
                        if first_date != Date::null()
                            && calendar.adjust(dates[dates.len() - 1], convention)
                                != calendar.adjust(first_date, convention)
                        {
                            let previous = dates[dates.len() - 1];
                            dates.push(first_date);
                            is_regular.push(
                                null_calendar.advance_by_period(
                                    previous,
                                    -tenor,
                                    convention,
                                    end_of_month,
                                ) == first_date,
                            );
                        }
                        break;
                    } else {
                        if calendar.adjust(dates[dates.len() - 1], convention)
                            != calendar.adjust(temp, convention)
                        {
                            dates.push(temp);
                            is_regular.push(true);
                        }
                        periods += 1;
                    }
                }

                if calendar.adjust(dates[dates.len() - 1], convention)
                    != calendar.adjust(effective_date, convention)
                {
                    let previous = dates[dates.len() - 1];
                    dates.push(effective_date);
                    is_regular.push(
                        null_calendar.advance_by_period(previous, -tenor, convention, end_of_month)
                            == effective_date,
                    );
                }

                dates.reverse();
                is_regular.reverse();
            }

            _ => {
                if matches!(
                    rule,
                    DateGeneration::Twentieth
                        | DateGeneration::TwentiethIMM
                        | DateGeneration::ThirdWednesday
                        | DateGeneration::ThirdWednesdayInclusive
                        | DateGeneration::OldCDS
                        | DateGeneration::CDS
                        | DateGeneration::CDS2015
                ) {
                    assert!(
                        !end_of_month,
                        "endOfMonth convention incompatible with {rule} date generation rule"
                    );
                }

                if rule == DateGeneration::CDS || rule == DateGeneration::CDS2015 {
                    let prev_twentieth = previous_twentieth(effective_date, rule);
                    if calendar.adjust(prev_twentieth, convention) > effective_date {
                        dates.push(prev_twentieth - Period::new(3, TimeUnit::Months));
                        is_regular.push(true);
                    }
                    dates.push(prev_twentieth);
                } else {
                    dates.push(effective_date);
                }

                seed = dates[dates.len() - 1];

                if first_date != Date::null() {
                    dates.push(first_date);
                    let temp = null_calendar.advance_by_period(
                        seed,
                        periods * tenor,
                        convention,
                        end_of_month,
                    );
                    is_regular.push(temp == first_date);
                    seed = first_date;
                } else if matches!(
                    rule,
                    DateGeneration::Twentieth
                        | DateGeneration::TwentiethIMM
                        | DateGeneration::OldCDS
                        | DateGeneration::CDS
                        | DateGeneration::CDS2015
                ) {
                    let mut next = next_twentieth(effective_date, rule);
                    let stub_days = 30;
                    if rule == DateGeneration::OldCDS && next - effective_date < stub_days {
                        next = next_twentieth(next + 1, rule);
                    }
                    if next != effective_date {
                        dates.push(next);
                        is_regular
                            .push(rule == DateGeneration::CDS || rule == DateGeneration::CDS2015);
                        seed = next;
                    }
                }

                let exit_date = if next_to_last_date != Date::null() {
                    next_to_last_date
                } else {
                    termination_date
                };
                loop {
                    let temp = null_calendar.advance_by_period(
                        seed,
                        periods * tenor,
                        convention,
                        end_of_month,
                    );
                    if temp > exit_date {
                        if next_to_last_date != Date::null()
                            && calendar.adjust(dates[dates.len() - 1], convention)
                                != calendar.adjust(next_to_last_date, convention)
                        {
                            let previous = dates[dates.len() - 1];
                            dates.push(next_to_last_date);
                            is_regular.push(
                                null_calendar.advance_by_period(
                                    previous,
                                    tenor,
                                    convention,
                                    end_of_month,
                                ) == next_to_last_date,
                            );
                        }
                        break;
                    } else {
                        if calendar.adjust(dates[dates.len() - 1], convention)
                            != calendar.adjust(temp, convention)
                        {
                            dates.push(temp);
                            is_regular.push(true);
                        }
                        periods += 1;
                    }
                }

                if calendar.adjust(dates[dates.len() - 1], termination_date_convention)
                    != calendar.adjust(termination_date, termination_date_convention)
                {
                    if matches!(
                        rule,
                        DateGeneration::Twentieth
                            | DateGeneration::TwentiethIMM
                            | DateGeneration::OldCDS
                            | DateGeneration::CDS
                            | DateGeneration::CDS2015
                    ) {
                        dates.push(next_twentieth(termination_date, rule));
                        is_regular.push(true);
                    } else {
                        dates.push(termination_date);
                        is_regular.push(false);
                    }
                }
            }
        }

        if rule == DateGeneration::ThirdWednesday {
            for i in 1..dates.len() - 1 {
                dates[i] =
                    Date::nth_weekday(3, Weekday::Wednesday, dates[i].month(), dates[i].year());
            }
        } else if rule == DateGeneration::ThirdWednesdayInclusive {
            for date in &mut dates {
                *date = Date::nth_weekday(3, Weekday::Wednesday, date.month(), date.year());
            }
        }

        if convention != BusinessDayConvention::Unadjusted && rule != DateGeneration::OldCDS {
            dates[0] = calendar.adjust(dates[0], convention);
        }

        if termination_date_convention != BusinessDayConvention::Unadjusted
            && rule != DateGeneration::CDS
            && rule != DateGeneration::CDS2015
        {
            let last = dates.len() - 1;
            dates[last] = calendar.adjust(dates[last], termination_date_convention);
        }

        if end_of_month && seed != Date::null() && calendar.is_end_of_month(seed) {
            for i in 1..dates.len() - 1 {
                dates[i] = calendar.adjust(Date::end_of_month(dates[i]), convention);
            }
        } else {
            for i in 1..dates.len() - 1 {
                dates[i] = calendar.adjust(dates[i], convention);
            }
        }

        if dates.len() >= 2 && dates[dates.len() - 2] >= dates[dates.len() - 1] {
            if is_regular.len() >= 2 {
                let n = is_regular.len();
                is_regular[n - 2] = dates[dates.len() - 2] == dates[dates.len() - 1];
            }
            let n = dates.len();
            dates[n - 2] = dates[n - 1];
            dates.pop();
            is_regular.pop();
        }
        if dates.len() >= 2 && dates[1] <= dates[0] {
            if is_regular.len() >= 2 {
                is_regular[1] = dates[1] == dates[0];
            }
            dates[1] = dates[0];
            dates.remove(0);
            is_regular.remove(0);
        }

        assert!(
            dates.len() > 1,
            "degenerate single date ({}) schedule: effective date: {effective_date}, termination date: {termination_date}, generation rule: {rule}, end of month: {end_of_month}",
            dates[0]
        );

        Schedule {
            tenor: Some(tenor),
            calendar,
            convention,
            termination_date_convention: Some(termination_date_convention),
            rule: Some(rule),
            end_of_month: Some(end_of_month),
            first_date,
            next_to_last_date,
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

    /// The index of the first date not earlier than `ref_date`.
    pub fn lower_bound(&self, ref_date: Date) -> usize {
        self.dates.partition_point(|d| *d < ref_date)
    }

    /// The first date later than or equal to `ref_date`, if any.
    pub fn next_date(&self, ref_date: Date) -> Option<Date> {
        self.dates.get(self.lower_bound(ref_date)).copied()
    }

    /// The last date earlier than `ref_date`, if any.
    pub fn previous_date(&self, ref_date: Date) -> Option<Date> {
        match self.lower_bound(ref_date) {
            0 => None,
            i => Some(self.dates[i - 1]),
        }
    }

    /// The schedule truncated to the dates on or after `truncation_date`.
    ///
    /// # Panics
    ///
    /// Panics if `truncation_date` is not before the last schedule date.
    pub fn after(&self, truncation_date: Date) -> Schedule {
        let mut result = self.clone();

        assert!(
            truncation_date < result.dates[result.dates.len() - 1],
            "truncation date {} must be before the last schedule date {}",
            truncation_date,
            result.dates[result.dates.len() - 1]
        );
        if truncation_date > result.dates[0] {
            while result.dates[0] < truncation_date {
                result.dates.remove(0);
                if !result.is_regular.is_empty() {
                    result.is_regular.remove(0);
                }
            }

            if truncation_date != result.dates[0] {
                result.dates.insert(0, truncation_date);
                result.is_regular.insert(0, false);
                result.termination_date_convention = Some(BusinessDayConvention::Unadjusted);
            } else {
                result.termination_date_convention = Some(self.convention);
            }

            if result.next_to_last_date <= truncation_date {
                result.next_to_last_date = Date::null();
            }
            if result.first_date <= truncation_date {
                result.first_date = Date::null();
            }
        }

        result
    }

    /// The schedule truncated to the dates on or before `truncation_date`.
    ///
    /// # Panics
    ///
    /// Panics if `truncation_date` is not later than the first schedule
    /// date.
    pub fn until(&self, truncation_date: Date) -> Schedule {
        let mut result = self.clone();

        assert!(
            truncation_date > result.dates[0],
            "truncation date {} must be later than schedule first date {}",
            truncation_date,
            result.dates[0]
        );
        if truncation_date < result.dates[result.dates.len() - 1] {
            while result.dates[result.dates.len() - 1] > truncation_date {
                result.dates.pop();
                if !result.is_regular.is_empty() {
                    result.is_regular.pop();
                }
            }

            if truncation_date != result.dates[result.dates.len() - 1] {
                result.dates.push(truncation_date);
                result.is_regular.push(false);
                result.termination_date_convention = Some(BusinessDayConvention::Unadjusted);
            } else {
                result.termination_date_convention = Some(self.convention);
            }

            if result.next_to_last_date >= truncation_date {
                result.next_to_last_date = Date::null();
            }
            if result.first_date >= truncation_date {
                result.first_date = Date::null();
            }
        }

        result
    }
}

impl std::ops::Index<usize> for Schedule {
    type Output = Date;

    fn index(&self, i: usize) -> &Date {
        &self.dates[i]
    }
}

/// Fluent builder over [`Schedule::new`], a port of QuantLib's
/// `MakeSchedule` helper class.
///
/// Defaults match QuantLib: `Backward` generation, no end-of-month
/// adjustment, a null calendar when none is given, `Following` when a
/// calendar is given but no convention, `Unadjusted` otherwise, and the
/// convention itself for the termination date when not overridden.
#[derive(Clone)]
pub struct MakeSchedule {
    calendar: Option<Calendar>,
    effective_date: Date,
    termination_date: Date,
    tenor: Option<Period>,
    convention: Option<BusinessDayConvention>,
    termination_date_convention: Option<BusinessDayConvention>,
    rule: DateGeneration,
    end_of_month: bool,
    first_date: Date,
    next_to_last_date: Date,
}

impl MakeSchedule {
    /// Starts a schedule specification with the QuantLib defaults.
    pub fn new() -> MakeSchedule {
        MakeSchedule {
            calendar: None,
            effective_date: Date::null(),
            termination_date: Date::null(),
            tenor: None,
            convention: None,
            termination_date_convention: None,
            rule: DateGeneration::Backward,
            end_of_month: false,
            first_date: Date::null(),
            next_to_last_date: Date::null(),
        }
    }

    /// Sets the effective date.
    pub fn from(mut self, effective_date: Date) -> MakeSchedule {
        self.effective_date = effective_date;
        self
    }

    /// Sets the termination date.
    pub fn to(mut self, termination_date: Date) -> MakeSchedule {
        self.termination_date = termination_date;
        self
    }

    /// Sets the tenor.
    pub fn with_tenor(mut self, tenor: Period) -> MakeSchedule {
        self.tenor = Some(tenor);
        self
    }

    /// Sets the tenor from a frequency.
    ///
    /// # Panics
    ///
    /// Panics for [`Frequency::OtherFrequency`], which names no period.
    pub fn with_frequency(mut self, frequency: Frequency) -> MakeSchedule {
        self.tenor = Some(
            Period::try_from(frequency).expect("no period equivalent for the given frequency"),
        );
        self
    }

    /// Sets the calendar.
    pub fn with_calendar(mut self, calendar: Calendar) -> MakeSchedule {
        self.calendar = Some(calendar);
        self
    }

    /// Sets the business-day convention.
    pub fn with_convention(mut self, convention: BusinessDayConvention) -> MakeSchedule {
        self.convention = Some(convention);
        self
    }

    /// Sets the business-day convention for the termination date.
    pub fn with_termination_date_convention(
        mut self,
        convention: BusinessDayConvention,
    ) -> MakeSchedule {
        self.termination_date_convention = Some(convention);
        self
    }

    /// Sets the date-generation rule.
    pub fn with_rule(mut self, rule: DateGeneration) -> MakeSchedule {
        self.rule = rule;
        self
    }

    /// Selects forward generation.
    pub fn forwards(self) -> MakeSchedule {
        self.with_rule(DateGeneration::Forward)
    }

    /// Selects backward generation.
    pub fn backwards(self) -> MakeSchedule {
        self.with_rule(DateGeneration::Backward)
    }

    /// Sets the end-of-month adjustment flag.
    pub fn end_of_month(mut self, flag: bool) -> MakeSchedule {
        self.end_of_month = flag;
        self
    }

    /// Sets the first (stub) date.
    pub fn with_first_date(mut self, d: Date) -> MakeSchedule {
        self.first_date = d;
        self
    }

    /// Sets the next-to-last (stub) date.
    pub fn with_next_to_last_date(mut self, d: Date) -> MakeSchedule {
        self.next_to_last_date = d;
        self
    }

    /// Builds the schedule, the equivalent of QuantLib's conversion
    /// operator.
    ///
    /// # Panics
    ///
    /// Panics if the effective date, termination date or tenor is missing,
    /// or if [`Schedule::new`] rejects the specification.
    pub fn build(self) -> Schedule {
        assert!(
            self.effective_date != Date::null(),
            "effective date not provided"
        );
        assert!(
            self.termination_date != Date::null(),
            "termination date not provided"
        );
        let tenor = self.tenor.expect("tenor/frequency not provided");

        let convention = match self.convention {
            Some(convention) => convention,
            None if self.calendar.is_some() => BusinessDayConvention::Following,
            None => BusinessDayConvention::Unadjusted,
        };
        let termination_date_convention = self.termination_date_convention.unwrap_or(convention);
        let calendar = self.calendar.unwrap_or_else(NullCalendar::new);

        Schedule::new(
            self.effective_date,
            self.termination_date,
            tenor,
            calendar,
            convention,
            termination_date_convention,
            self.rule,
            self.end_of_month,
            self.first_date,
            self.next_to_last_date,
        )
    }
}

impl Default for MakeSchedule {
    fn default() -> MakeSchedule {
        MakeSchedule::new()
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

fn next_twentieth(d: Date, rule: DateGeneration) -> Date {
    let mut result = Date::new(20, d.month(), d.year());
    if result < d {
        result = result + Period::new(1, TimeUnit::Months);
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
            result = result + Period::new(3 - m % 3, TimeUnit::Months);
        }
    }
    result
}

fn is_imm_date(d: Date) -> bool {
    d.weekday() == Weekday::Wednesday && (15..=21).contains(&d.day_of_month())
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
    fn navigation_and_bounds() {
        let schedule = Schedule::from_dates(vec![
            d(16, Month::May, 2015),
            d(18, Month::May, 2015),
            d(18, Month::May, 2016),
        ]);
        assert_eq!(
            schedule.next_date(d(17, Month::May, 2015)),
            Some(d(18, Month::May, 2015))
        );
        assert_eq!(
            schedule.next_date(d(18, Month::May, 2015)),
            Some(d(18, Month::May, 2015))
        );
        assert_eq!(schedule.next_date(d(19, Month::May, 2016)), None);
        assert_eq!(
            schedule.previous_date(d(17, Month::May, 2015)),
            Some(d(16, Month::May, 2015))
        );
        assert_eq!(schedule.previous_date(d(16, Month::May, 2015)), None);
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
    fn backward_regular_first_period_with_first_date() {
        let semiannual = Period::new(6, TimeUnit::Months);

        let backward = Schedule::new(
            d(30, Month::September, 2017),
            d(30, Month::September, 2024),
            semiannual,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            BusinessDayConvention::Unadjusted,
            DateGeneration::Backward,
            true,
            d(31, Month::March, 2018),
            Date::null(),
        );
        assert!(
            backward.is_regular_at(1),
            "first period should be regular (effectiveDate + 6M == firstDate)"
        );

        let forward = Schedule::new(
            d(30, Month::September, 2017),
            d(30, Month::September, 2024),
            semiannual,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            BusinessDayConvention::Unadjusted,
            DateGeneration::Forward,
            true,
            d(31, Month::March, 2018),
            Date::null(),
        );
        assert!(
            forward.is_regular_at(1),
            "forward first period should also be regular"
        );

        let irregular = Schedule::new(
            d(3, Month::September, 2017),
            d(30, Month::September, 2024),
            semiannual,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            BusinessDayConvention::Unadjusted,
            DateGeneration::Backward,
            true,
            d(31, Month::March, 2018),
            Date::null(),
        );
        assert!(
            !irregular.is_regular_at(1),
            "first period should be irregular (effectiveDate + 6M != firstDate)"
        );

        let forward_ntl_irregular = Schedule::new(
            d(30, Month::September, 2017),
            d(30, Month::September, 2024),
            semiannual,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            BusinessDayConvention::Unadjusted,
            DateGeneration::Forward,
            true,
            Date::null(),
            d(15, Month::March, 2024),
        );
        let n = forward_ntl_irregular.len();
        assert!(
            !forward_ntl_irregular.is_regular_at(n - 2),
            "period ending at off-grid nextToLastDate should be irregular"
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
