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
    ///
    /// # Panics
    ///
    /// Panics on a null `ref_date`, which, unlike QuantLib, is never
    /// inferred from the evaluation date.
    pub fn lower_bound(&self, ref_date: Date) -> usize {
        assert!(ref_date != Date::null(), "null reference date");
        self.dates.partition_point(|d| *d < ref_date)
    }

    /// The first date later than or equal to `ref_date`, if any.
    ///
    /// # Panics
    ///
    /// Panics on a null `ref_date`, which, unlike QuantLib, is never
    /// inferred from the evaluation date.
    pub fn next_date(&self, ref_date: Date) -> Option<Date> {
        self.dates.get(self.lower_bound(ref_date)).copied()
    }

    /// The last date earlier than `ref_date`, if any.
    ///
    /// # Panics
    ///
    /// Panics on a null `ref_date`, which, unlike QuantLib, is never
    /// inferred from the evaluation date.
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
    /// Panics if the schedule is empty or `truncation_date` is not before
    /// the last schedule date.
    pub fn after(&self, truncation_date: Date) -> Schedule {
        let mut result = self.clone();

        assert!(
            !result.dates.is_empty(),
            "cannot truncate an empty schedule"
        );
        assert!(
            truncation_date < result.dates[result.dates.len() - 1],
            "truncation date {} must be before the last schedule date {}",
            truncation_date,
            result.dates[result.dates.len() - 1]
        );
        if truncation_date > result.dates[0] {
            let cut = result.lower_bound(truncation_date);
            result.dates.drain(..cut);
            result.is_regular.drain(..cut.min(result.is_regular.len()));

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
    /// Panics if the schedule is empty or `truncation_date` is not later
    /// than the first schedule date.
    pub fn until(&self, truncation_date: Date) -> Schedule {
        let mut result = self.clone();

        assert!(
            !result.dates.is_empty(),
            "cannot truncate an empty schedule"
        );
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
    use crate::time::calendars::japan::Japan;
    use crate::time::calendars::target::Target;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::{Day, Month, Year};

    fn d(day: Day, m: Month, y: Year) -> Date {
        Date::new(day, m, y)
    }

    fn make_cds_schedule(from: Date, to: Date, rule: DateGeneration) -> Schedule {
        MakeSchedule::new()
            .from(from)
            .to(to)
            .with_calendar(WeekendsOnly::new())
            .with_tenor(Period::new(3, TimeUnit::Months))
            .with_convention(BusinessDayConvention::Following)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_rule(rule)
            .build()
    }

    fn cds_maturity(trade_date: Date, tenor: Period, rule: DateGeneration) -> Option<Date> {
        assert!(matches!(
            rule,
            DateGeneration::CDS2015 | DateGeneration::CDS | DateGeneration::OldCDS
        ));
        assert!(
            tenor.units() == TimeUnit::Years
                || (tenor.units() == TimeUnit::Months && tenor.length() % 3 == 0)
        );
        if rule == DateGeneration::OldCDS {
            assert!(tenor != Period::new(0, TimeUnit::Months));
        }
        let mut anchor_date = previous_twentieth(trade_date, rule);
        if rule == DateGeneration::CDS2015
            && (anchor_date == d(20, Month::December, anchor_date.year())
                || anchor_date == d(20, Month::June, anchor_date.year()))
        {
            if tenor.length() == 0 {
                return None;
            }
            anchor_date = anchor_date - Period::new(3, TimeUnit::Months);
        }
        let maturity = anchor_date + tenor + Period::new(3, TimeUnit::Months);
        assert!(maturity > trade_date, "error calculating CDS maturity");
        Some(maturity)
    }

    fn check_dates(s: &Schedule, expected: &[Date]) {
        assert_eq!(
            s.len(),
            expected.len(),
            "expected {} dates, found {}",
            expected.len(),
            s.len()
        );
        for (i, e) in expected.iter().enumerate() {
            assert_eq!(s[i], *e, "wrong date at index {i}");
        }
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
    fn daily_schedule() {
        let start_date = d(17, Month::January, 2012);

        let s = MakeSchedule::new()
            .from(start_date)
            .to(start_date + 7)
            .with_calendar(Target::new())
            .with_frequency(Frequency::Daily)
            .with_convention(BusinessDayConvention::Preceding)
            .build();

        check_dates(
            &s,
            &[
                d(17, Month::January, 2012),
                d(18, Month::January, 2012),
                d(19, Month::January, 2012),
                d(20, Month::January, 2012),
                d(23, Month::January, 2012),
                d(24, Month::January, 2012),
            ],
        );
    }

    #[test]
    fn eom_adjustment_with_different_conventions() {
        let start_date = d(29, Month::February, 2024);
        let end_date = start_date + Period::new(1, TimeUnit::Years);

        let s1 = MakeSchedule::new()
            .from(start_date)
            .to(end_date)
            .with_calendar(Target::new())
            .with_frequency(Frequency::Monthly)
            .with_convention(BusinessDayConvention::Unadjusted)
            .end_of_month(true)
            .build();

        check_dates(
            &s1,
            &[
                d(29, Month::February, 2024),
                d(31, Month::March, 2024),
                d(30, Month::April, 2024),
                d(31, Month::May, 2024),
                d(30, Month::June, 2024),
                d(31, Month::July, 2024),
                d(31, Month::August, 2024),
                d(30, Month::September, 2024),
                d(31, Month::October, 2024),
                d(30, Month::November, 2024),
                d(31, Month::December, 2024),
                d(31, Month::January, 2025),
                d(28, Month::February, 2025),
            ],
        );

        let s2 = MakeSchedule::new()
            .from(start_date)
            .to(end_date)
            .with_calendar(Target::new())
            .with_frequency(Frequency::Monthly)
            .with_convention(BusinessDayConvention::Following)
            .end_of_month(true)
            .build();

        check_dates(
            &s2,
            &[
                d(29, Month::February, 2024),
                d(2, Month::April, 2024),
                d(30, Month::April, 2024),
                d(31, Month::May, 2024),
                d(1, Month::July, 2024),
                d(31, Month::July, 2024),
                d(2, Month::September, 2024),
                d(30, Month::September, 2024),
                d(31, Month::October, 2024),
                d(2, Month::December, 2024),
                d(31, Month::December, 2024),
                d(31, Month::January, 2025),
                d(28, Month::February, 2025),
            ],
        );

        let s3 = MakeSchedule::new()
            .from(start_date)
            .to(end_date)
            .with_calendar(Target::new())
            .with_frequency(Frequency::Monthly)
            .with_convention(BusinessDayConvention::ModifiedPreceding)
            .end_of_month(true)
            .build();

        check_dates(
            &s3,
            &[
                d(29, Month::February, 2024),
                d(28, Month::March, 2024),
                d(30, Month::April, 2024),
                d(31, Month::May, 2024),
                d(28, Month::June, 2024),
                d(31, Month::July, 2024),
                d(30, Month::August, 2024),
                d(30, Month::September, 2024),
                d(31, Month::October, 2024),
                d(29, Month::November, 2024),
                d(31, Month::December, 2024),
                d(31, Month::January, 2025),
                d(28, Month::February, 2025),
            ],
        );
    }

    #[test]
    fn end_date_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(30, Month::September, 2009))
            .to(d(15, Month::June, 2012))
            .with_calendar(Japan::new())
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .forwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(30, Month::September, 2009),
                d(31, Month::March, 2010),
                d(30, Month::September, 2010),
                d(31, Month::March, 2011),
                d(30, Month::September, 2011),
                d(30, Month::March, 2012),
                d(15, Month::June, 2012),
            ],
        );
    }

    #[test]
    fn dates_past_end_date_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(28, Month::March, 2013))
            .to(d(30, Month::March, 2015))
            .with_calendar(Target::new())
            .with_tenor(Period::new(1, TimeUnit::Years))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(28, Month::March, 2013),
                d(31, Month::March, 2014),
                d(30, Month::March, 2015),
            ],
        );
        assert!(!s.is_regular_at(2), "last period should not be regular");
    }

    #[test]
    fn dates_same_as_end_date_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(28, Month::March, 2013))
            .to(d(31, Month::March, 2015))
            .with_calendar(Target::new())
            .with_tenor(Period::new(1, TimeUnit::Years))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(28, Month::March, 2013),
                d(31, Month::March, 2014),
                d(31, Month::March, 2015),
            ],
        );
        assert!(s.is_regular_at(2), "last period should be regular");
    }

    #[test]
    fn forward_dates_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(31, Month::August, 1996))
            .to(d(15, Month::September, 1997))
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(31, Month::August, 1996),
                d(28, Month::February, 1997),
                d(31, Month::August, 1997),
                d(15, Month::September, 1997),
            ],
        );
    }

    #[test]
    fn backward_dates_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(22, Month::August, 1996))
            .to(d(31, Month::August, 1997))
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(22, Month::August, 1996),
                d(31, Month::August, 1996),
                d(28, Month::February, 1997),
                d(31, Month::August, 1997),
            ],
        );
    }

    #[test]
    fn double_first_date_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(22, Month::August, 1996))
            .to(d(31, Month::August, 1997))
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::Following)
            .backwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(22, Month::August, 1996),
                d(30, Month::August, 1996),
                d(28, Month::February, 1997),
                d(2, Month::September, 1997),
            ],
        );
    }

    #[test]
    fn first_date_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(10, Month::August, 1996))
            .to(d(10, Month::August, 1998))
            .with_first_date(d(28, Month::February, 1997))
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .forwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(12, Month::August, 1996),
                d(28, Month::February, 1997),
                d(29, Month::August, 1997),
                d(27, Month::February, 1998),
                d(10, Month::August, 1998),
            ],
        );
    }

    #[test]
    fn next_to_last_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(10, Month::August, 1996))
            .to(d(10, Month::August, 1998))
            .with_next_to_last_date(d(28, Month::February, 1998))
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .backwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(12, Month::August, 1996),
                d(30, Month::August, 1996),
                d(28, Month::February, 1997),
                d(29, Month::August, 1997),
                d(27, Month::February, 1998),
                d(10, Month::August, 1998),
            ],
        );
    }

    #[test]
    fn effective_date_with_eom_adjustment() {
        let s = MakeSchedule::new()
            .from(d(16, Month::January, 2023))
            .to(d(16, Month::March, 2023))
            .with_first_date(d(31, Month::January, 2023))
            .with_calendar(NullCalendar::new())
            .with_tenor(Period::new(1, TimeUnit::Months))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(true)
            .build();

        check_dates(
            &s,
            &[
                d(16, Month::January, 2023),
                d(31, Month::January, 2023),
                d(28, Month::February, 2023),
                d(16, Month::March, 2023),
            ],
        );
    }

    type CdsGridRow = (
        Day,
        Integer,
        Year,
        Integer,
        Day,
        Integer,
        Year,
        Day,
        Integer,
        Year,
    );

    fn check_cds_grid(rows: &[CdsGridRow], rule: DateGeneration) {
        for &(td, tm, ty, tenor_months, sd, sm, sy, ed, em, ey) in rows {
            let from = d(td, Month::from_ordinal(tm), ty);
            let tenor = Period::new(tenor_months, TimeUnit::Months);
            let exp_start = d(sd, Month::from_ordinal(sm), sy);
            let exp_end = d(ed, Month::from_ordinal(em), ey);

            let maturity = cds_maturity(from, tenor, rule).expect("live CDS maturity");
            assert_eq!(maturity, exp_end, "maturity from {from}, tenor {tenor}");

            let s = make_cds_schedule(from, maturity, rule);
            assert_eq!(
                s.start_date(),
                exp_start,
                "start from {from}, tenor {tenor}"
            );
            assert_eq!(s.end_date(), exp_end, "end from {from}, tenor {tenor}");
        }
    }

    #[test]
    fn cds2015_convention_grid() {
        let rows: &[CdsGridRow] = &[
            (19, 3, 2016, 3, 21, 12, 2015, 20, 3, 2016),
            (20, 3, 2016, 3, 21, 12, 2015, 20, 9, 2016),
            (21, 3, 2016, 3, 21, 3, 2016, 20, 9, 2016),
            (19, 6, 2016, 3, 21, 3, 2016, 20, 9, 2016),
            (20, 6, 2016, 3, 20, 6, 2016, 20, 9, 2016),
            (21, 6, 2016, 3, 20, 6, 2016, 20, 9, 2016),
            (19, 9, 2016, 3, 20, 6, 2016, 20, 9, 2016),
            (20, 9, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (21, 9, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (19, 12, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (20, 12, 2016, 3, 20, 12, 2016, 20, 3, 2017),
            (21, 12, 2016, 3, 20, 12, 2016, 20, 3, 2017),
            (19, 3, 2016, 6, 21, 12, 2015, 20, 6, 2016),
            (20, 3, 2016, 6, 21, 12, 2015, 20, 12, 2016),
            (21, 3, 2016, 6, 21, 3, 2016, 20, 12, 2016),
            (19, 6, 2016, 6, 21, 3, 2016, 20, 12, 2016),
            (20, 6, 2016, 6, 20, 6, 2016, 20, 12, 2016),
            (21, 6, 2016, 6, 20, 6, 2016, 20, 12, 2016),
            (19, 9, 2016, 6, 20, 6, 2016, 20, 12, 2016),
            (20, 9, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (21, 9, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (19, 12, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (20, 12, 2016, 6, 20, 12, 2016, 20, 6, 2017),
            (21, 12, 2016, 6, 20, 12, 2016, 20, 6, 2017),
            (19, 3, 2016, 9, 21, 12, 2015, 20, 9, 2016),
            (20, 3, 2016, 9, 21, 12, 2015, 20, 3, 2017),
            (21, 3, 2016, 9, 21, 3, 2016, 20, 3, 2017),
            (19, 6, 2016, 9, 21, 3, 2016, 20, 3, 2017),
            (20, 6, 2016, 9, 20, 6, 2016, 20, 3, 2017),
            (21, 6, 2016, 9, 20, 6, 2016, 20, 3, 2017),
            (19, 9, 2016, 9, 20, 6, 2016, 20, 3, 2017),
            (20, 9, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (21, 9, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (19, 12, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (20, 12, 2016, 9, 20, 12, 2016, 20, 9, 2017),
            (21, 12, 2016, 9, 20, 12, 2016, 20, 9, 2017),
            (19, 3, 2016, 12, 21, 12, 2015, 20, 12, 2016),
            (20, 3, 2016, 12, 21, 12, 2015, 20, 6, 2017),
            (21, 3, 2016, 12, 21, 3, 2016, 20, 6, 2017),
            (19, 6, 2016, 12, 21, 3, 2016, 20, 6, 2017),
            (20, 6, 2016, 12, 20, 6, 2016, 20, 6, 2017),
            (21, 6, 2016, 12, 20, 6, 2016, 20, 6, 2017),
            (19, 9, 2016, 12, 20, 6, 2016, 20, 6, 2017),
            (20, 9, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (21, 9, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (19, 12, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (20, 12, 2016, 12, 20, 12, 2016, 20, 12, 2017),
            (21, 12, 2016, 12, 20, 12, 2016, 20, 12, 2017),
            (19, 3, 2016, 60, 21, 12, 2015, 20, 12, 2020),
            (20, 3, 2016, 60, 21, 12, 2015, 20, 6, 2021),
            (21, 3, 2016, 60, 21, 3, 2016, 20, 6, 2021),
            (19, 6, 2016, 60, 21, 3, 2016, 20, 6, 2021),
            (20, 6, 2016, 60, 20, 6, 2016, 20, 6, 2021),
            (21, 6, 2016, 60, 20, 6, 2016, 20, 6, 2021),
            (19, 9, 2016, 60, 20, 6, 2016, 20, 6, 2021),
            (20, 9, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (21, 9, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (19, 12, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (20, 12, 2016, 60, 20, 12, 2016, 20, 12, 2021),
            (21, 12, 2016, 60, 20, 12, 2016, 20, 12, 2021),
            (20, 3, 2016, 0, 21, 12, 2015, 20, 6, 2016),
            (21, 3, 2016, 0, 21, 3, 2016, 20, 6, 2016),
            (19, 6, 2016, 0, 21, 3, 2016, 20, 6, 2016),
            (20, 9, 2016, 0, 20, 9, 2016, 20, 12, 2016),
            (21, 9, 2016, 0, 20, 9, 2016, 20, 12, 2016),
            (19, 12, 2016, 0, 20, 9, 2016, 20, 12, 2016),
        ];
        check_cds_grid(rows, DateGeneration::CDS2015);
    }

    #[test]
    fn cds_convention_grid() {
        let rows: &[CdsGridRow] = &[
            (19, 3, 2016, 3, 21, 12, 2015, 20, 6, 2016),
            (20, 3, 2016, 3, 21, 12, 2015, 20, 9, 2016),
            (21, 3, 2016, 3, 21, 3, 2016, 20, 9, 2016),
            (19, 6, 2016, 3, 21, 3, 2016, 20, 9, 2016),
            (20, 6, 2016, 3, 20, 6, 2016, 20, 12, 2016),
            (21, 6, 2016, 3, 20, 6, 2016, 20, 12, 2016),
            (19, 9, 2016, 3, 20, 6, 2016, 20, 12, 2016),
            (20, 9, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (21, 9, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (19, 12, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (20, 12, 2016, 3, 20, 12, 2016, 20, 6, 2017),
            (21, 12, 2016, 3, 20, 12, 2016, 20, 6, 2017),
            (19, 3, 2016, 6, 21, 12, 2015, 20, 9, 2016),
            (20, 3, 2016, 6, 21, 12, 2015, 20, 12, 2016),
            (21, 3, 2016, 6, 21, 3, 2016, 20, 12, 2016),
            (19, 6, 2016, 6, 21, 3, 2016, 20, 12, 2016),
            (20, 6, 2016, 6, 20, 6, 2016, 20, 3, 2017),
            (21, 6, 2016, 6, 20, 6, 2016, 20, 3, 2017),
            (19, 9, 2016, 6, 20, 6, 2016, 20, 3, 2017),
            (20, 9, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (21, 9, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (19, 12, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (20, 12, 2016, 6, 20, 12, 2016, 20, 9, 2017),
            (21, 12, 2016, 6, 20, 12, 2016, 20, 9, 2017),
            (19, 3, 2016, 9, 21, 12, 2015, 20, 12, 2016),
            (20, 3, 2016, 9, 21, 12, 2015, 20, 3, 2017),
            (21, 3, 2016, 9, 21, 3, 2016, 20, 3, 2017),
            (19, 6, 2016, 9, 21, 3, 2016, 20, 3, 2017),
            (20, 6, 2016, 9, 20, 6, 2016, 20, 6, 2017),
            (21, 6, 2016, 9, 20, 6, 2016, 20, 6, 2017),
            (19, 9, 2016, 9, 20, 6, 2016, 20, 6, 2017),
            (20, 9, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (21, 9, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (19, 12, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (20, 12, 2016, 9, 20, 12, 2016, 20, 12, 2017),
            (21, 12, 2016, 9, 20, 12, 2016, 20, 12, 2017),
            (19, 3, 2016, 12, 21, 12, 2015, 20, 3, 2017),
            (20, 3, 2016, 12, 21, 12, 2015, 20, 6, 2017),
            (21, 3, 2016, 12, 21, 3, 2016, 20, 6, 2017),
            (19, 6, 2016, 12, 21, 3, 2016, 20, 6, 2017),
            (20, 6, 2016, 12, 20, 6, 2016, 20, 9, 2017),
            (21, 6, 2016, 12, 20, 6, 2016, 20, 9, 2017),
            (19, 9, 2016, 12, 20, 6, 2016, 20, 9, 2017),
            (20, 9, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (21, 9, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (19, 12, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (20, 12, 2016, 12, 20, 12, 2016, 20, 3, 2018),
            (21, 12, 2016, 12, 20, 12, 2016, 20, 3, 2018),
            (19, 3, 2016, 60, 21, 12, 2015, 20, 3, 2021),
            (20, 3, 2016, 60, 21, 12, 2015, 20, 6, 2021),
            (21, 3, 2016, 60, 21, 3, 2016, 20, 6, 2021),
            (19, 6, 2016, 60, 21, 3, 2016, 20, 6, 2021),
            (20, 6, 2016, 60, 20, 6, 2016, 20, 9, 2021),
            (21, 6, 2016, 60, 20, 6, 2016, 20, 9, 2021),
            (19, 9, 2016, 60, 20, 6, 2016, 20, 9, 2021),
            (20, 9, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (21, 9, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (19, 12, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (20, 12, 2016, 60, 20, 12, 2016, 20, 3, 2022),
            (21, 12, 2016, 60, 20, 12, 2016, 20, 3, 2022),
            (19, 3, 2016, 0, 21, 12, 2015, 20, 3, 2016),
            (20, 3, 2016, 0, 21, 12, 2015, 20, 6, 2016),
            (21, 3, 2016, 0, 21, 3, 2016, 20, 6, 2016),
            (19, 6, 2016, 0, 21, 3, 2016, 20, 6, 2016),
            (20, 6, 2016, 0, 20, 6, 2016, 20, 9, 2016),
            (21, 6, 2016, 0, 20, 6, 2016, 20, 9, 2016),
            (19, 9, 2016, 0, 20, 6, 2016, 20, 9, 2016),
            (20, 9, 2016, 0, 20, 9, 2016, 20, 12, 2016),
            (21, 9, 2016, 0, 20, 9, 2016, 20, 12, 2016),
            (19, 12, 2016, 0, 20, 9, 2016, 20, 12, 2016),
            (20, 12, 2016, 0, 20, 12, 2016, 20, 3, 2017),
            (21, 12, 2016, 0, 20, 12, 2016, 20, 3, 2017),
        ];
        check_cds_grid(rows, DateGeneration::CDS);
    }

    #[test]
    fn old_cds_convention_grid() {
        let rows: &[CdsGridRow] = &[
            (19, 3, 2016, 3, 19, 3, 2016, 20, 6, 2016),
            (20, 3, 2016, 3, 20, 3, 2016, 20, 9, 2016),
            (21, 3, 2016, 3, 21, 3, 2016, 20, 9, 2016),
            (19, 6, 2016, 3, 19, 6, 2016, 20, 9, 2016),
            (20, 6, 2016, 3, 20, 6, 2016, 20, 12, 2016),
            (21, 6, 2016, 3, 21, 6, 2016, 20, 12, 2016),
            (19, 9, 2016, 3, 19, 9, 2016, 20, 12, 2016),
            (20, 9, 2016, 3, 20, 9, 2016, 20, 3, 2017),
            (21, 9, 2016, 3, 21, 9, 2016, 20, 3, 2017),
            (19, 12, 2016, 3, 19, 12, 2016, 20, 3, 2017),
            (20, 12, 2016, 3, 20, 12, 2016, 20, 6, 2017),
            (21, 12, 2016, 3, 21, 12, 2016, 20, 6, 2017),
            (19, 3, 2016, 6, 19, 3, 2016, 20, 9, 2016),
            (20, 3, 2016, 6, 20, 3, 2016, 20, 12, 2016),
            (21, 3, 2016, 6, 21, 3, 2016, 20, 12, 2016),
            (19, 6, 2016, 6, 19, 6, 2016, 20, 12, 2016),
            (20, 6, 2016, 6, 20, 6, 2016, 20, 3, 2017),
            (21, 6, 2016, 6, 21, 6, 2016, 20, 3, 2017),
            (19, 9, 2016, 6, 19, 9, 2016, 20, 3, 2017),
            (20, 9, 2016, 6, 20, 9, 2016, 20, 6, 2017),
            (21, 9, 2016, 6, 21, 9, 2016, 20, 6, 2017),
            (19, 12, 2016, 6, 19, 12, 2016, 20, 6, 2017),
            (20, 12, 2016, 6, 20, 12, 2016, 20, 9, 2017),
            (21, 12, 2016, 6, 21, 12, 2016, 20, 9, 2017),
            (19, 3, 2016, 9, 19, 3, 2016, 20, 12, 2016),
            (20, 3, 2016, 9, 20, 3, 2016, 20, 3, 2017),
            (21, 3, 2016, 9, 21, 3, 2016, 20, 3, 2017),
            (19, 6, 2016, 9, 19, 6, 2016, 20, 3, 2017),
            (20, 6, 2016, 9, 20, 6, 2016, 20, 6, 2017),
            (21, 6, 2016, 9, 21, 6, 2016, 20, 6, 2017),
            (19, 9, 2016, 9, 19, 9, 2016, 20, 6, 2017),
            (20, 9, 2016, 9, 20, 9, 2016, 20, 9, 2017),
            (21, 9, 2016, 9, 21, 9, 2016, 20, 9, 2017),
            (19, 12, 2016, 9, 19, 12, 2016, 20, 9, 2017),
            (20, 12, 2016, 9, 20, 12, 2016, 20, 12, 2017),
            (21, 12, 2016, 9, 21, 12, 2016, 20, 12, 2017),
            (19, 3, 2016, 12, 19, 3, 2016, 20, 3, 2017),
            (20, 3, 2016, 12, 20, 3, 2016, 20, 6, 2017),
            (21, 3, 2016, 12, 21, 3, 2016, 20, 6, 2017),
            (19, 6, 2016, 12, 19, 6, 2016, 20, 6, 2017),
            (20, 6, 2016, 12, 20, 6, 2016, 20, 9, 2017),
            (21, 6, 2016, 12, 21, 6, 2016, 20, 9, 2017),
            (19, 9, 2016, 12, 19, 9, 2016, 20, 9, 2017),
            (20, 9, 2016, 12, 20, 9, 2016, 20, 12, 2017),
            (21, 9, 2016, 12, 21, 9, 2016, 20, 12, 2017),
            (19, 12, 2016, 12, 19, 12, 2016, 20, 12, 2017),
            (20, 12, 2016, 12, 20, 12, 2016, 20, 3, 2018),
            (21, 12, 2016, 12, 21, 12, 2016, 20, 3, 2018),
            (19, 3, 2016, 60, 19, 3, 2016, 20, 3, 2021),
            (20, 3, 2016, 60, 20, 3, 2016, 20, 6, 2021),
            (21, 3, 2016, 60, 21, 3, 2016, 20, 6, 2021),
            (19, 6, 2016, 60, 19, 6, 2016, 20, 6, 2021),
            (20, 6, 2016, 60, 20, 6, 2016, 20, 9, 2021),
            (21, 6, 2016, 60, 21, 6, 2016, 20, 9, 2021),
            (19, 9, 2016, 60, 19, 9, 2016, 20, 9, 2021),
            (20, 9, 2016, 60, 20, 9, 2016, 20, 12, 2021),
            (21, 9, 2016, 60, 21, 9, 2016, 20, 12, 2021),
            (19, 12, 2016, 60, 19, 12, 2016, 20, 12, 2021),
            (20, 12, 2016, 60, 20, 12, 2016, 20, 3, 2022),
            (21, 12, 2016, 60, 21, 12, 2016, 20, 3, 2022),
        ];
        check_cds_grid(rows, DateGeneration::OldCDS);
    }

    #[test]
    fn cds2015_convention() {
        let rule = DateGeneration::CDS2015;
        let tenor = Period::new(5, TimeUnit::Years);

        let trade_date = d(12, Month::December, 2016);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let exp_start = d(20, Month::September, 2016);
        let exp_maturity = d(20, Month::December, 2021);
        assert_eq!(maturity, exp_maturity);
        let s = make_cds_schedule(trade_date, maturity, rule);
        assert_eq!(s.start_date(), exp_start);
        assert_eq!(s.end_date(), exp_maturity);

        let maturity = trade_date + tenor;
        let s = make_cds_schedule(trade_date, maturity, rule);
        assert_eq!(s.start_date(), exp_start);
        assert_eq!(s.end_date(), exp_maturity);

        let trade_date = d(1, Month::March, 2017);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        assert_eq!(maturity, exp_maturity);
        let s = make_cds_schedule(trade_date, maturity, rule);
        let exp_start = d(20, Month::December, 2016);
        assert_eq!(s.start_date(), exp_start);
        assert_eq!(s.end_date(), exp_maturity);

        let maturity = trade_date + tenor;
        let s = make_cds_schedule(trade_date, maturity, rule);
        assert_eq!(s.start_date(), exp_start);
        assert_eq!(s.end_date(), d(20, Month::March, 2022));

        let trade_date = d(20, Month::March, 2017);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let exp_start = d(20, Month::March, 2017);
        let exp_maturity = d(20, Month::June, 2022);
        assert_eq!(maturity, exp_maturity);
        let s = make_cds_schedule(trade_date, maturity, rule);
        assert_eq!(s.start_date(), exp_start);
        assert_eq!(s.end_date(), exp_maturity);
    }

    #[test]
    fn cds2015_convention_sample_dates() {
        let rule = DateGeneration::CDS2015;
        let tenor = Period::new(1, TimeUnit::Years);

        let trade_date = d(18, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        let mut exp_dates = vec![
            d(22, Month::June, 2015),
            d(21, Month::September, 2015),
            d(21, Month::December, 2015),
            d(21, Month::March, 2016),
            d(20, Month::June, 2016),
        ];
        check_dates(&s, &exp_dates);

        let trade_date = d(19, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        check_dates(&s, &exp_dates);

        let trade_date = d(20, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates.push(d(20, Month::September, 2016));
        exp_dates.push(d(20, Month::December, 2016));
        check_dates(&s, &exp_dates);

        let trade_date = d(21, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates.remove(0);
        check_dates(&s, &exp_dates);

        let trade_date = d(20, Month::June, 2009);
        let maturity = d(20, Month::December, 2009);
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates = vec![
            d(20, Month::March, 2009),
            d(22, Month::June, 2009),
            d(21, Month::September, 2009),
            d(20, Month::December, 2009),
        ];
        check_dates(&s, &exp_dates);

        let trade_date = d(21, Month::June, 2009);
        let s = make_cds_schedule(trade_date, maturity, rule);
        check_dates(&s, &exp_dates);

        let trade_date = d(22, Month::June, 2009);
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates.remove(0);
        check_dates(&s, &exp_dates);
    }

    #[test]
    fn cds_convention_sample_dates() {
        let rule = DateGeneration::CDS;
        let tenor = Period::new(1, TimeUnit::Years);

        let trade_date = d(18, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        let mut exp_dates = vec![
            d(22, Month::June, 2015),
            d(21, Month::September, 2015),
            d(21, Month::December, 2015),
            d(21, Month::March, 2016),
            d(20, Month::June, 2016),
            d(20, Month::September, 2016),
        ];
        check_dates(&s, &exp_dates);

        let trade_date = d(19, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        check_dates(&s, &exp_dates);

        let trade_date = d(20, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates.push(d(20, Month::December, 2016));
        check_dates(&s, &exp_dates);

        let trade_date = d(21, Month::September, 2015);
        let maturity = cds_maturity(trade_date, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates.remove(0);
        check_dates(&s, &exp_dates);

        let trade_date = d(20, Month::June, 2009);
        let maturity = d(20, Month::December, 2009);
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates = vec![
            d(20, Month::March, 2009),
            d(22, Month::June, 2009),
            d(21, Month::September, 2009),
            d(20, Month::December, 2009),
        ];
        check_dates(&s, &exp_dates);

        let trade_date = d(21, Month::June, 2009);
        let s = make_cds_schedule(trade_date, maturity, rule);
        check_dates(&s, &exp_dates);

        let trade_date = d(22, Month::June, 2009);
        let s = make_cds_schedule(trade_date, maturity, rule);
        exp_dates.remove(0);
        check_dates(&s, &exp_dates);
    }

    #[test]
    fn old_cds_convention_sample_dates() {
        let rule = DateGeneration::OldCDS;
        let tenor = Period::new(1, TimeUnit::Years);

        let mut trade_date_plus_one = d(18, Month::September, 2015);
        let maturity = cds_maturity(trade_date_plus_one, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        let mut exp_dates = vec![
            d(18, Month::September, 2015),
            d(21, Month::December, 2015),
            d(21, Month::March, 2016),
            d(20, Month::June, 2016),
            d(20, Month::September, 2016),
        ];
        check_dates(&s, &exp_dates);

        trade_date_plus_one = d(19, Month::September, 2015);
        exp_dates[0] = trade_date_plus_one;
        let maturity = cds_maturity(trade_date_plus_one, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        check_dates(&s, &exp_dates);

        trade_date_plus_one = d(20, Month::September, 2015);
        exp_dates[0] = trade_date_plus_one;
        let maturity = cds_maturity(trade_date_plus_one, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        exp_dates.push(d(20, Month::December, 2016));
        check_dates(&s, &exp_dates);

        trade_date_plus_one = d(21, Month::September, 2015);
        exp_dates[0] = trade_date_plus_one;
        let maturity = cds_maturity(trade_date_plus_one, tenor, rule).unwrap();
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        check_dates(&s, &exp_dates);

        trade_date_plus_one = d(19, Month::November, 2015);
        exp_dates[0] = trade_date_plus_one;
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        check_dates(&s, &exp_dates);

        trade_date_plus_one = d(20, Month::November, 2015);
        exp_dates[0] = trade_date_plus_one;
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        check_dates(&s, &exp_dates);

        trade_date_plus_one = d(21, Month::November, 2015);
        exp_dates[0] = trade_date_plus_one;
        let s = make_cds_schedule(trade_date_plus_one, maturity, rule);
        exp_dates.remove(1);
        check_dates(&s, &exp_dates);
    }

    #[test]
    fn cds2015_zero_months_matured() {
        let rule = DateGeneration::CDS2015;
        let tenor = Period::new(0, TimeUnit::Months);

        let inputs = [
            d(20, Month::December, 2015),
            d(15, Month::February, 2016),
            d(19, Month::March, 2016),
            d(20, Month::June, 2016),
            d(15, Month::August, 2016),
            d(19, Month::September, 2016),
            d(20, Month::December, 2016),
        ];

        for input in inputs {
            assert_eq!(cds_maturity(input, tenor, rule), None, "at {input}");
        }
    }

    #[test]
    fn four_weeks_tenor() {
        let s = MakeSchedule::new()
            .from(d(13, Month::January, 2016))
            .to(d(4, Month::May, 2016))
            .with_calendar(Target::new())
            .with_tenor(Period::new(4, TimeUnit::Weeks))
            .with_convention(BusinessDayConvention::Following)
            .forwards()
            .build();
        assert!(s.len() > 1);
    }

    #[test]
    fn once_frequency() {
        let s = MakeSchedule::new()
            .from(d(13, Month::January, 2016))
            .to(d(13, Month::January, 2019))
            .with_frequency(Frequency::Once)
            .forwards()
            .build();

        assert_eq!(s.len(), 2);
        assert_eq!(s[0], d(13, Month::January, 2016));
        assert_eq!(s[1], d(13, Month::January, 2019));
    }

    #[test]
    fn schedule_always_has_a_start_date() {
        let calendar = UnitedStates::new(Market::GovernmentBond);
        let schedule = MakeSchedule::new()
            .from(d(10, Month::January, 2017))
            .with_first_date(d(31, Month::August, 2017))
            .to(d(28, Month::February, 2026))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(calendar.clone())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .end_of_month(false)
            .build();
        assert_eq!(
            schedule.date(0),
            d(10, Month::January, 2017),
            "the first element should always be the start date"
        );

        let schedule = MakeSchedule::new()
            .from(d(10, Month::January, 2017))
            .to(d(28, Month::February, 2026))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(calendar.clone())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .end_of_month(false)
            .build();
        assert_eq!(
            schedule.date(0),
            d(10, Month::January, 2017),
            "the first element should always be the start date"
        );

        let schedule = MakeSchedule::new()
            .from(d(31, Month::August, 2017))
            .to(d(28, Month::February, 2026))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(calendar)
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .end_of_month(false)
            .build();
        assert_eq!(
            schedule.date(0),
            d(31, Month::August, 2017),
            "the first element should always be the start date"
        );
    }

    #[test]
    fn short_eom_schedule() {
        let s = MakeSchedule::new()
            .from(d(21, Month::February, 2019))
            .to(d(28, Month::February, 2019))
            .with_calendar(Target::new())
            .with_tenor(Period::new(1, TimeUnit::Years))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .backwards()
            .end_of_month(true)
            .build();

        assert_eq!(s.len(), 2);
        assert_eq!(s[0], d(21, Month::February, 2019));
        assert_eq!(s[1], d(28, Month::February, 2019));
    }

    #[test]
    fn first_date_on_maturity() {
        let expected = [d(20, Month::September, 2016), d(20, Month::December, 2016)];

        let schedule = MakeSchedule::new()
            .from(d(20, Month::September, 2016))
            .to(d(20, Month::December, 2016))
            .with_first_date(d(20, Month::December, 2016))
            .with_frequency(Frequency::Quarterly)
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        check_dates(&schedule, &expected);

        let schedule = MakeSchedule::new()
            .from(d(20, Month::September, 2016))
            .to(d(20, Month::December, 2016))
            .with_first_date(d(20, Month::December, 2016))
            .with_frequency(Frequency::Quarterly)
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .build();
        check_dates(&schedule, &expected);
    }

    #[test]
    fn next_to_last_date_on_start() {
        let expected = [d(20, Month::September, 2016), d(20, Month::December, 2016)];

        let schedule = MakeSchedule::new()
            .from(d(20, Month::September, 2016))
            .to(d(20, Month::December, 2016))
            .with_next_to_last_date(d(20, Month::September, 2016))
            .with_frequency(Frequency::Quarterly)
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        check_dates(&schedule, &expected);
    }

    #[test]
    fn truncation() {
        let s = MakeSchedule::new()
            .from(d(30, Month::September, 2009))
            .to(d(15, Month::June, 2020))
            .with_calendar(Japan::new())
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .forwards()
            .end_of_month(true)
            .build();

        let t = s.until(d(1, Month::January, 2014));
        check_dates(
            &t,
            &[
                d(30, Month::September, 2009),
                d(31, Month::March, 2010),
                d(30, Month::September, 2010),
                d(31, Month::March, 2011),
                d(30, Month::September, 2011),
                d(30, Month::March, 2012),
                d(28, Month::September, 2012),
                d(29, Month::March, 2013),
                d(30, Month::September, 2013),
                d(1, Month::January, 2014),
            ],
        );
        assert!(!t.is_regular()[t.is_regular().len() - 1]);

        let t = s.until(d(30, Month::September, 2013));
        check_dates(
            &t,
            &[
                d(30, Month::September, 2009),
                d(31, Month::March, 2010),
                d(30, Month::September, 2010),
                d(31, Month::March, 2011),
                d(30, Month::September, 2011),
                d(30, Month::March, 2012),
                d(28, Month::September, 2012),
                d(29, Month::March, 2013),
                d(30, Month::September, 2013),
            ],
        );
        assert!(t.is_regular()[t.is_regular().len() - 1]);

        let t = s.after(d(1, Month::January, 2014));
        check_dates(
            &t,
            &[
                d(1, Month::January, 2014),
                d(31, Month::March, 2014),
                d(30, Month::September, 2014),
                d(31, Month::March, 2015),
                d(30, Month::September, 2015),
                d(31, Month::March, 2016),
                d(30, Month::September, 2016),
                d(31, Month::March, 2017),
                d(29, Month::September, 2017),
                d(30, Month::March, 2018),
                d(28, Month::September, 2018),
                d(29, Month::March, 2019),
                d(30, Month::September, 2019),
                d(31, Month::March, 2020),
                d(15, Month::June, 2020),
            ],
        );
        assert!(!t.is_regular()[0]);

        let t = s.after(d(28, Month::September, 2018));
        check_dates(
            &t,
            &[
                d(28, Month::September, 2018),
                d(29, Month::March, 2019),
                d(30, Month::September, 2019),
                d(31, Month::March, 2020),
                d(15, Month::June, 2020),
            ],
        );
        assert!(t.is_regular()[0]);
    }

    #[test]
    fn truncation_regular_metadata_matches_inserted_or_existing_cut_date() {
        let s = MakeSchedule::new()
            .from(d(30, Month::September, 2009))
            .to(d(30, Month::September, 2011))
            .with_calendar(NullCalendar::new())
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::Unadjusted)
            .forwards()
            .end_of_month(true)
            .build();

        let inserted_front = s.after(d(1, Month::January, 2010));
        assert_eq!(inserted_front.date(0), d(1, Month::January, 2010));
        assert!(!inserted_front.is_regular_at(1));

        let existing_front = s.after(d(31, Month::March, 2010));
        assert_eq!(existing_front.date(0), d(31, Month::March, 2010));
        assert!(existing_front.is_regular_at(1));

        let inserted_back = s.until(d(1, Month::January, 2011));
        assert_eq!(
            inserted_back.date(inserted_back.len() - 1),
            d(1, Month::January, 2011)
        );
        assert!(!inserted_back.is_regular_at(inserted_back.len() - 1));

        let existing_back = s.until(d(31, Month::March, 2011));
        assert_eq!(
            existing_back.date(existing_back.len() - 1),
            d(31, Month::March, 2011)
        );
        assert!(existing_back.is_regular_at(existing_back.len() - 1));
    }

    #[test]
    #[should_panic(expected = "null reference date")]
    fn null_reference_date_is_rejected() {
        let s = Schedule::from_dates(vec![d(1, Month::January, 2014)]);
        s.next_date(Date::null());
    }

    #[test]
    #[should_panic(expected = "cannot truncate an empty schedule")]
    fn after_rejects_empty_schedule() {
        Schedule::from_dates(vec![]).after(d(1, Month::January, 2014));
    }

    #[test]
    #[should_panic(expected = "cannot truncate an empty schedule")]
    fn until_rejects_empty_schedule() {
        Schedule::from_dates(vec![]).until(d(1, Month::January, 2014));
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
