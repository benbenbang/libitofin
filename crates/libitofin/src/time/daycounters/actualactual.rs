//! Actual/Actual day count conventions.
//!
//! Port of `ql/time/daycounters/actualactual.{hpp,cpp}`. Three families are
//! supported:
//!
//! - **ISDA** ("Actual/Actual (Historical)", "Act/Act", and per ISDA also
//!   "Actual/365"): each day is weighted by the length of its own calendar year.
//! - **ISMA / Bond** (US Treasury): each day is weighted by the length of the
//!   coupon period it falls in, taken from the supplied reference period.
//! - **AFB** ("Actual/Actual (Euro)"): whole years are peeled off the far end,
//!   with a leap-day-aware denominator for the stub.
//!
//! ## Divergences from QuantLib
//!
//! QuantLib's ISMA counter has two implementations: a schedule-driven one
//! (`ISMA_Impl`), used when a [`Schedule`] is supplied to
//! [`ActualActual::with_schedule`], and a reference-date one (`Old_ISMA_Impl`),
//! used otherwise and reached through
//! [`year_fraction_ref`](crate::time::daycounter::DayCounter::year_fraction_ref).
//! Both are ported here and both keep QuantLib's name, `Actual/Actual (ISMA)`.
//!
//! QuantLib's `ISMA_Impl::yearFraction` raises when the requested period falls
//! outside the schedule. [`DayCounterImpl::year_fraction`] is infallible, so the
//! port panics there instead of returning an error; see its `# Panics` section.

use crate::shared::shared;
use crate::time::date::{Date, Month};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Time};

/// The Actual/Actual convention to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Convention {
    /// ISMA / US-Treasury bond convention; identical to [`Bond`](Self::Bond).
    ISMA,
    /// Bond convention; identical to [`ISMA`](Self::ISMA).
    Bond,
    /// ISDA convention; identical to [`Historical`](Self::Historical).
    ISDA,
    /// Historical convention; identical to [`ISDA`](Self::ISDA).
    Historical,
    /// Actual/365 (ISDA alias); identical to [`ISDA`](Self::ISDA).
    Actual365,
    /// AFB convention; identical to [`Euro`](Self::Euro).
    AFB,
    /// Euro convention; identical to [`AFB`](Self::AFB).
    Euro,
}

/// The Actual/Actual day count convention.
pub struct ActualActual;

impl ActualActual {
    /// Builds an Actual/Actual counter for the given convention.
    ///
    /// The ISMA/Bond counter uses the reference-date algorithm; supply the
    /// reference period through
    /// [`year_fraction_ref`](crate::time::daycounter::DayCounter::year_fraction_ref).
    pub fn with_convention(c: Convention) -> DayCounter {
        match c {
            Convention::ISMA | Convention::Bond => DayCounter::from_impl(shared(IsmaImpl)),
            Convention::ISDA | Convention::Historical | Convention::Actual365 => {
                DayCounter::from_impl(shared(IsdaImpl))
            }
            Convention::AFB | Convention::Euro => DayCounter::from_impl(shared(AfbImpl)),
        }
    }

    /// Builds an Actual/Actual counter for the given convention, backed by a
    /// schedule.
    ///
    /// Only the ISMA/Bond conventions consult the schedule: they take the
    /// reference period for a date range from the schedule's coupon dates
    /// (extended with quasi-payment dates around irregular stubs) rather than
    /// from the caller. An empty schedule falls back to the reference-date
    /// algorithm, as does every other convention.
    pub fn with_schedule(c: Convention, schedule: Schedule) -> DayCounter {
        match c {
            Convention::ISMA | Convention::Bond if !schedule.is_empty() => {
                DayCounter::from_impl(shared(ScheduleIsmaImpl { schedule }))
            }
            _ => ActualActual::with_convention(c),
        }
    }
}

/// `Real(d2 - d1)`, QuantLib's `daysBetween`.
fn days_between(d1: Date, d2: Date) -> Time {
    Time::from(d2 - d1)
}

/// `schedule.calendar().advance(d, p, ...)` with the schedule's own convention
/// and end-of-month flag.
fn advance(schedule: &Schedule, d: Date, p: Period) -> Date {
    schedule.calendar().advance_by_period(
        d,
        p,
        schedule.business_day_convention(),
        schedule.end_of_month(),
    )
}

/// The schedule's dates, with the quasi-payment dates that bracket an irregular
/// first or last period substituted in (and, where the stub spills outside the
/// schedule, prepended or appended).
///
/// Port of `getListOfPeriodDatesIncludingQuasiPayments`. Note that the two
/// rewrites index the *original* schedule positions: when a prior notional
/// coupon has been prepended, `size - 1` no longer addresses the final date.
/// That is QuantLib's behaviour and is reproduced here.
fn period_dates_including_quasi_payments(schedule: &Schedule) -> Vec<Date> {
    let issue_date = schedule.date(0);
    let size = schedule.len();
    let mut new_dates = schedule.dates().to_vec();

    if !schedule.has_is_regular() || !schedule.is_regular_at(1) {
        let first_coupon = schedule.date(1);
        let notional_first_coupon = advance(schedule, first_coupon, -schedule.tenor());
        new_dates[0] = notional_first_coupon;

        if notional_first_coupon > issue_date {
            let prior_notional_coupon = advance(schedule, notional_first_coupon, -schedule.tenor());
            new_dates.insert(0, prior_notional_coupon);
        }
    }

    if !schedule.has_is_regular() || !schedule.is_regular_at(size - 1) {
        let notional_last_coupon = advance(schedule, schedule.date(size - 2), schedule.tenor());
        new_dates[size - 1] = notional_last_coupon;

        if notional_last_coupon < schedule.end_date() {
            let next_notional_coupon = advance(schedule, notional_last_coupon, schedule.tenor());
            new_dates.push(next_notional_coupon);
        }
    }

    new_dates
}

struct IsmaImpl;

impl DayCounterImpl for IsmaImpl {
    fn name(&self) -> String {
        "Actual/Actual (ISMA)".to_string()
    }

    /// # Panics
    ///
    /// Panics (mirroring QuantLib's `QL_REQUIRE`) if the reference period is
    /// degenerate - `ref_period_end` must be strictly after both
    /// `ref_period_start` and `d1`.
    fn year_fraction(
        &self,
        d1: Date,
        d2: Date,
        ref_period_start: Date,
        ref_period_end: Date,
    ) -> Time {
        if d1 == d2 {
            return 0.0;
        }
        if d1 > d2 {
            return -self.year_fraction(d2, d1, ref_period_start, ref_period_end);
        }

        // When the reference period is not specified, take it equal to (d1, d2).
        let mut ref_start = if ref_period_start != Date::null() {
            ref_period_start
        } else {
            d1
        };
        let mut ref_end = if ref_period_end != Date::null() {
            ref_period_end
        } else {
            d2
        };

        assert!(
            ref_end > ref_start && ref_end > d1,
            "invalid reference period for Actual/Actual (ISMA)"
        );

        // Estimate roughly the length in months of a period.
        let mut months = (12.0 * days_between(ref_start, ref_end) / 365.0).round() as Integer;
        if months == 0 {
            // A short stub with no usable reference period is treated as a
            // fraction of the year following d1. Building d1 + 1*Years overflows
            // the supported date range when d1 is in the last representable
            // year; there d2 is necessarily within that same year, so the result
            // reduces to the stub over the year's length (365 - neither the last
            // supported year nor the next one is a leap year).
            if d1.year() == Date::max_date().year() {
                return days_between(d1, d2) / 365.0;
            }
            // ...take the reference period as 1 year from d1.
            ref_start = d1;
            ref_end = d1 + 1 * TimeUnit::Years;
            months = 12;
        }

        let period = Time::from(months) / 12.0;

        if d2 <= ref_end {
            if d1 >= ref_start {
                // ref_start <= d1 <= d2 <= ref_end
                period * days_between(d1, d2) / days_between(ref_start, ref_end)
            } else {
                // Long first coupon: d1 < ref_start < ref_end and d2 <= ref_end.
                let previous_ref = ref_start - months * TimeUnit::Months;
                if d2 > ref_start {
                    self.year_fraction(d1, ref_start, previous_ref, ref_start)
                        + self.year_fraction(ref_start, d2, ref_start, ref_end)
                } else {
                    self.year_fraction(d1, d2, previous_ref, ref_start)
                }
            }
        } else {
            // ref_end is the last (notional) payment date: d1 < ref_end < d2.
            assert!(
                ref_start <= d1,
                "invalid dates: d1 < ref_period_start < ref_period_end < d2"
            );
            // The part from d1 to ref_end.
            let mut sum = self.year_fraction(d1, ref_end, ref_start, ref_end);
            // Count whole regular periods in [ref_end, d2], then the remainder.
            let mut i = 0;
            let (mut new_ref_start, mut new_ref_end);
            loop {
                new_ref_start = ref_end + (months * i) * TimeUnit::Months;
                new_ref_end = ref_end + (months * (i + 1)) * TimeUnit::Months;
                if d2 < new_ref_end {
                    break;
                }
                sum += period;
                i += 1;
            }
            sum += self.year_fraction(new_ref_start, d2, new_ref_start, new_ref_end);
            sum
        }
    }
}

/// The number of coupon periods per year implied by a reference period, from
/// its length in whole months. Port of `findCouponsPerYear`; only correct for
/// reference periods longer than 15 days.
fn coupons_per_year(ref_start: Date, ref_end: Date) -> Integer {
    let months = (12.0 * days_between(ref_start, ref_end) / 365.0).round() as Integer;
    (12.0 / Time::from(months)).round() as Integer
}

/// Port of `yearFractionWithReferenceDates`: `[d1, d2]` measured against the
/// reference period `[d3, d4]`, whose length fixes the coupon frequency.
///
/// # Panics
///
/// Panics (mirroring QuantLib's `QL_REQUIRE`) if `d1 > d2`.
fn year_fraction_with_reference_dates(d1: Date, d2: Date, d3: Date, d4: Date) -> Time {
    assert!(d1 <= d2, "this function is only correct if d1 <= d2");

    let mut reference_day_count = days_between(d3, d4);
    let coupons = if reference_day_count < 16.0 {
        reference_day_count = days_between(d1, d1 + 1 * TimeUnit::Years);
        1
    } else {
        coupons_per_year(d3, d4)
    };

    days_between(d1, d2) / (reference_day_count * Time::from(coupons))
}

struct ScheduleIsmaImpl {
    schedule: Schedule,
}

impl DayCounterImpl for ScheduleIsmaImpl {
    fn name(&self) -> String {
        "Actual/Actual (ISMA)".to_string()
    }

    /// The reference period is taken from the schedule, so the reference-date
    /// arguments are ignored.
    ///
    /// # Panics
    ///
    /// Panics (mirroring QuantLib's `QL_REQUIRE`) if `[d1, d2]` is not
    /// contained in the schedule's date range, extended by any quasi-payment
    /// dates. The alternative - extrapolating past the last coupon - would
    /// silently return a wrong accrual, so the port keeps QuantLib's hard stop
    /// rather than widening [`DayCounterImpl::year_fraction`] to a `Result`.
    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        if d1 == d2 {
            return 0.0;
        }
        if d2 < d1 {
            return -self.year_fraction(d2, d1, Date::null(), Date::null());
        }

        let coupon_dates = period_dates_including_quasi_payments(&self.schedule);
        let first_date = *coupon_dates
            .iter()
            .min()
            .expect("a non-empty schedule has coupon dates");
        let last_date = *coupon_dates
            .iter()
            .max()
            .expect("a non-empty schedule has coupon dates");

        assert!(
            d1 >= first_date && d2 <= last_date,
            "dates out of range of schedule: date 1: {d1}, date 2: {d2}, \
             first date: {first_date}, last date: {last_date}"
        );

        let mut sum = 0.0;
        for pair in coupon_dates.windows(2) {
            let (start_reference_period, end_reference_period) = (pair[0], pair[1]);
            if d1 < end_reference_period && d2 > start_reference_period {
                sum += year_fraction_with_reference_dates(
                    d1.max(start_reference_period),
                    d2.min(end_reference_period),
                    start_reference_period,
                    end_reference_period,
                );
            }
        }
        sum
    }
}

struct IsdaImpl;

impl DayCounterImpl for IsdaImpl {
    fn name(&self) -> String {
        "Actual/Actual (ISDA)".to_string()
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        if d1 == d2 {
            return 0.0;
        }
        if d1 > d2 {
            return -self.year_fraction(d2, d1, Date::null(), Date::null());
        }

        let y1 = d1.year();
        let y2 = d2.year();
        let dib1 = if Date::is_leap(y1) { 366.0 } else { 365.0 };
        let dib2 = if Date::is_leap(y2) { 366.0 } else { 365.0 };

        // Same-year periods reduce to a plain day count; taking the general
        // route would build Jan 1st of y1 + 1, which overflows the supported
        // date range when y1 is its last year.
        if y1 == y2 {
            return days_between(d1, d2) / dib1;
        }

        let mut sum = Time::from(y2 - y1 - 1);
        sum += days_between(d1, Date::new(1, Month::January, y1 + 1)) / dib1;
        sum += days_between(Date::new(1, Month::January, y2), d2) / dib2;
        sum
    }
}

struct AfbImpl;

impl DayCounterImpl for AfbImpl {
    fn name(&self) -> String {
        "Actual/Actual (AFB)".to_string()
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        if d1 == d2 {
            return 0.0;
        }
        if d1 > d2 {
            return -self.year_fraction(d2, d1, Date::null(), Date::null());
        }

        let mut new_d2 = d2;
        let mut temp = d2;
        let mut sum = 0.0;
        while temp > d1 {
            temp = new_d2 - 1 * TimeUnit::Years;
            if temp.day_of_month() == 28
                && temp.month() == Month::February
                && Date::is_leap(temp.year())
            {
                temp += 1;
            }
            if temp >= d1 {
                sum += 1.0;
                new_d2 = temp;
            }
        }

        let mut den = 365.0;
        if Date::is_leap(new_d2.year()) {
            temp = Date::new(29, Month::February, new_d2.year());
            if new_d2 > temp && d1 <= temp {
                den += 1.0;
            }
        } else if Date::is_leap(d1.year()) {
            temp = Date::new(29, Month::February, d1.year());
            if new_d2 > temp && d1 <= temp {
                den += 1.0;
            }
        }

        sum + days_between(d1, new_d2) / den
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::canada::{Canada, Market as CanadaMarket};
    use crate::time::calendars::china::{China, Market as ChinaMarket};
    use crate::time::calendars::unitedstates::{Market as UsMarket, UnitedStates};
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    fn d(day: Integer, m: Month, y: Integer) -> Date {
        Date::new(day, m, y)
    }

    const TOL: Time = 1.0e-10;

    /// The `testActualActualIsma` schedule: an odd last period.
    fn odd_last_period_schedule() -> Schedule {
        MakeSchedule::new()
            .from(d(30, Month::January, 1999))
            .to(d(30, Month::June, 2000))
            .with_frequency(Frequency::Semiannual)
            .with_first_date(d(30, Month::July, 1999))
            .with_next_to_last_date(d(30, Month::January, 2000))
            .end_of_month(false)
            .build()
    }

    /// The `testActualActualWithSchedule` schedule: a long first coupon.
    fn long_first_coupon_schedule() -> Schedule {
        MakeSchedule::new()
            .from(d(17, Month::January, 2017))
            .with_first_date(d(31, Month::August, 2017))
            .to(d(28, Month::February, 2026))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(Canada::new(CanadaMarket::Settlement))
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .end_of_month(true)
            .build()
    }

    /// The test suite's own `ISMAYearFractionWithReferenceDates`: an
    /// independent, simpler reading of the ISMA rule, used as a cross-check.
    fn isma_year_fraction_with_reference_dates(
        dc: &DayCounter,
        start: Date,
        end: Date,
        ref_start: Date,
        ref_end: Date,
    ) -> Time {
        let reference_day_count = Time::from(dc.day_count(ref_start, ref_end));
        let coupons_per_year = (365.0 / reference_day_count).round();
        Time::from(dc.day_count(start, end)) / (reference_day_count * coupons_per_year)
    }

    /// The test suite's own `actualActualDaycountComputation`: sums the
    /// cross-check above over the schedule's regular periods.
    fn actual_actual_daycount_computation(schedule: &Schedule, start: Date, end: Date) -> Time {
        let dc = ActualActual::with_schedule(Convention::ISMA, schedule.clone());
        let mut year_fraction = 0.0;
        for i in 1..schedule.len() - 1 {
            let reference_start = schedule.date(i);
            let reference_end = schedule.date(i + 1);
            if start < reference_end && end > reference_start {
                year_fraction += isma_year_fraction_with_reference_dates(
                    &dc,
                    start.max(reference_start),
                    end.min(reference_end),
                    reference_start,
                    reference_end,
                );
            }
        }
        year_fraction
    }

    fn next_day(calendar: &Calendar, d: Date) -> Date {
        calendar.advance(
            d,
            1,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        )
    }

    /// The `testActualActualWithSemiannualSchedule` / `WithAnnualSchedule`
    /// schedule, whose first period is an undefined (long) stub.
    fn undefined_first_period_schedule(frequency: Frequency, end_of_month: bool) -> Schedule {
        MakeSchedule::new()
            .from(d(10, Month::January, 2017))
            .with_first_date(d(31, Month::August, 2017))
            .to(d(28, Month::February, 2026))
            .with_frequency(frequency)
            .with_calendar(UnitedStates::new(UsMarket::GovernmentBond))
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .end_of_month(end_of_month)
            .build()
    }

    #[test]
    fn quasi_payments_extend_an_odd_last_period() {
        let schedule = odd_last_period_schedule();
        let dates = period_dates_including_quasi_payments(&schedule);
        // The 30 Jun 2000 maturity is replaced by the 30 Jul 2000 notional
        // coupon; it is later than the maturity, so nothing is appended.
        assert_eq!(
            dates,
            vec![
                d(30, Month::January, 1999),
                d(30, Month::July, 1999),
                d(30, Month::January, 2000),
                d(30, Month::July, 2000),
            ]
        );
    }

    #[test]
    fn quasi_payments_prepend_before_a_long_first_coupon() {
        // The two quasi coupon dates asserted by testActualActualWithSchedule.
        let schedule = long_first_coupon_schedule();
        let dates = period_dates_including_quasi_payments(&schedule);
        assert_eq!(dates[0], d(31, Month::August, 2016));
        assert_eq!(dates[1], d(28, Month::February, 2017));
        assert_eq!(dates[2], d(31, Month::August, 2017));
        assert_eq!(dates.len(), schedule.len() + 1);
    }

    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            ActualActual::with_convention(Convention::ISMA).name(),
            "Actual/Actual (ISMA)"
        );
        assert_eq!(
            ActualActual::with_convention(Convention::ISDA).name(),
            "Actual/Actual (ISDA)"
        );
        assert_eq!(
            ActualActual::with_convention(Convention::AFB).name(),
            "Actual/Actual (AFB)"
        );
        // Aliases share a rule.
        assert_eq!(
            ActualActual::with_convention(Convention::Bond).name(),
            ActualActual::with_convention(Convention::ISMA).name()
        );
        assert_eq!(
            ActualActual::with_convention(Convention::Historical).name(),
            ActualActual::with_convention(Convention::ISDA).name()
        );
    }

    #[test]
    fn isda_handles_the_last_supported_year() {
        // A same-year period in 2199 must not reach for Jan 1st 2200, which
        // is outside the supported date range.
        let dc = ActualActual::with_convention(Convention::ISDA);
        let t = dc.year_fraction(d(1, Month::January, 2199), d(2, Month::January, 2199));
        assert!((t - 1.0 / 365.0).abs() < TOL, "got {t}");
    }

    #[test]
    fn isma_handles_the_last_supported_year() {
        // A short stub in the last supported year rounds to 0 months and would
        // otherwise fall back to d1 + 1 year (Jan 1st 2200), outside the range.
        let dc = ActualActual::with_convention(Convention::ISMA);
        let t = dc.year_fraction(d(1, Month::January, 2199), d(2, Month::January, 2199));
        assert!((t - 1.0 / 365.0).abs() < TOL, "got {t}");
    }

    #[test]
    fn isda_known_good_values() {
        // From QuantLib's testActualActual.
        let dc = ActualActual::with_convention(Convention::ISDA);
        let cases: &[(Date, Date, Time)] = &[
            (
                d(1, Month::November, 2003),
                d(1, Month::May, 2004),
                0.497724380567,
            ),
            (
                d(1, Month::February, 1999),
                d(1, Month::July, 1999),
                0.410958904110,
            ),
            (
                d(1, Month::July, 1999),
                d(1, Month::July, 2000),
                1.001377348600,
            ),
            (
                d(15, Month::August, 2002),
                d(15, Month::July, 2003),
                0.915068493151,
            ),
            (
                d(30, Month::January, 2000),
                d(30, Month::June, 2000),
                0.415300546448,
            ),
        ];
        for &(d1, d2, expected) in cases {
            assert!(
                (dc.year_fraction(d1, d2) - expected).abs() < TOL,
                "{d1} -> {d2}"
            );
        }
    }

    #[test]
    fn afb_known_good_values() {
        let dc = ActualActual::with_convention(Convention::AFB);
        let cases: &[(Date, Date, Time)] = &[
            (
                d(1, Month::November, 2003),
                d(1, Month::May, 2004),
                0.497267759563,
            ),
            (
                d(1, Month::July, 1999),
                d(1, Month::July, 2000),
                1.000000000000,
            ),
            (
                d(15, Month::July, 2003),
                d(15, Month::January, 2004),
                0.504109589041,
            ),
        ];
        for &(d1, d2, expected) in cases {
            assert!(
                (dc.year_fraction(d1, d2) - expected).abs() < TOL,
                "{d1} -> {d2}"
            );
        }
    }

    #[test]
    fn isma_reference_based_known_good_values() {
        // From QuantLib's testActualActual ISMA cases (explicit reference dates).
        let dc = ActualActual::with_convention(Convention::ISMA);
        let cases: &[(Date, Date, Date, Date, Time)] = &[
            (
                d(1, Month::November, 2003),
                d(1, Month::May, 2004),
                d(1, Month::November, 2003),
                d(1, Month::May, 2004),
                0.500000000000,
            ),
            (
                d(15, Month::August, 2002),
                d(15, Month::July, 2003),
                d(15, Month::January, 2003),
                d(15, Month::July, 2003),
                0.915760869565,
            ),
            (
                d(30, Month::January, 2000),
                d(30, Month::June, 2000),
                d(30, Month::January, 2000),
                d(30, Month::July, 2000),
                0.417582417582,
            ),
        ];
        for &(d1, d2, rs, re, expected) in cases {
            assert!(
                (dc.year_fraction_ref(d1, d2, rs, re) - expected).abs() < TOL,
                "{d1} -> {d2}"
            );
        }
    }

    /// Port of `testActualActualIsma`.
    #[test]
    fn isma_with_schedule_and_odd_last_period() {
        let dc = ActualActual::with_schedule(Convention::ISMA, odd_last_period_schedule());
        let calculated = dc.year_fraction(d(30, Month::January, 2000), d(30, Month::June, 2000));
        let expected = 152.0 / (182.0 * 2.0);
        assert!((calculated - expected).abs() < TOL, "got {calculated}");
    }

    /// Port of `testActualActualWithSemiannualSchedule`. The half that compares
    /// the schedule-driven counter with the plain reference-date one is the
    /// load-bearing check: both must agree over a full reference period.
    #[test]
    fn isma_with_undefined_semiannual_reference_periods() {
        let calendar = UnitedStates::new(UsMarket::GovernmentBond);
        let from_date = d(10, Month::January, 2017);
        let first_coupon = d(31, Month::August, 2017);
        let quasi_coupon = d(28, Month::February, 2017);
        let quasi_coupon2 = d(31, Month::August, 2016);

        let schedule = undefined_first_period_schedule(Frequency::Semiannual, true);
        let dc = ActualActual::with_schedule(Convention::ISMA, schedule.clone());
        let dc_no_schedule = ActualActual::with_convention(Convention::ISMA);

        let reference_period_start = schedule.date(1);
        let reference_period_end = schedule.date(2);

        assert_eq!(
            dc.year_fraction(reference_period_start, reference_period_start),
            0.0
        );
        assert_eq!(
            dc_no_schedule.year_fraction(reference_period_start, reference_period_start),
            0.0
        );
        assert_eq!(
            dc_no_schedule.year_fraction_ref(
                reference_period_start,
                reference_period_start,
                reference_period_start,
                reference_period_start,
            ),
            0.0
        );
        assert_eq!(
            dc.year_fraction(reference_period_start, reference_period_end),
            0.5
        );
        assert_eq!(
            dc_no_schedule.year_fraction_ref(
                reference_period_start,
                reference_period_end,
                reference_period_start,
                reference_period_end,
            ),
            0.5
        );

        let mut test_date = schedule.date(1);
        while test_date < reference_period_end {
            let difference = dc.year_fraction_ref(
                test_date,
                reference_period_end,
                reference_period_start,
                reference_period_end,
            ) - dc.year_fraction(test_date, reference_period_end);
            assert!(difference.abs() < TOL, "at {test_date}");
            test_date = next_day(&calendar, test_date);
        }

        let calculated = dc.year_fraction(from_date, first_coupon);
        let expected = 0.5
            + Time::from(dc.day_count(from_date, quasi_coupon))
                / (2.0 * Time::from(dc.day_count(quasi_coupon2, quasi_coupon)));
        assert!((calculated - expected).abs() < TOL, "got {calculated}");

        let schedule = undefined_first_period_schedule(Frequency::Semiannual, false);
        let dc = ActualActual::with_schedule(Convention::ISMA, schedule.clone());
        let period_start_date = schedule.date(1);
        let mut period_end_date = schedule.date(2);

        while period_end_date < schedule.date(schedule.len() - 2) {
            let expected =
                actual_actual_daycount_computation(&schedule, period_start_date, period_end_date);
            let calculated = dc.year_fraction(period_start_date, period_end_date);
            assert!(
                (expected - calculated).abs() < 1.0e-8,
                "{period_start_date} to {period_end_date}"
            );
            period_end_date = next_day(&calendar, period_end_date);
        }
    }

    /// Port of `testActualActualWithAnnualSchedule`.
    #[test]
    fn isma_with_undefined_annual_reference_periods() {
        let calendar = UnitedStates::new(UsMarket::GovernmentBond);
        let schedule = undefined_first_period_schedule(Frequency::Annual, false);
        let dc = ActualActual::with_schedule(Convention::ISMA, schedule.clone());

        let reference_period_start = schedule.date(1);
        let reference_period_end = schedule.date(2);

        let mut test_date = schedule.date(1);
        while test_date < reference_period_end {
            let difference = isma_year_fraction_with_reference_dates(
                &dc,
                test_date,
                reference_period_end,
                reference_period_start,
                reference_period_end,
            ) - dc.year_fraction(test_date, reference_period_end);
            assert!(difference.abs() < TOL, "at {test_date}");
            test_date = next_day(&calendar, test_date);
        }
    }

    /// Port of `testActualActualWithSchedule`: a long first coupon split across
    /// two quasi-periods.
    #[test]
    fn isma_with_schedule_and_long_first_coupon() {
        let schedule = long_first_coupon_schedule();
        let issue_date = schedule.date(0);
        let first_coupon_date = schedule.date(1);
        assert_eq!(issue_date, d(17, Month::January, 2017));
        assert_eq!(first_coupon_date, d(31, Month::August, 2017));

        let quasi_coupon_date2 = advance(&schedule, first_coupon_date, -schedule.tenor());
        let quasi_coupon_date1 = advance(&schedule, quasi_coupon_date2, -schedule.tenor());
        assert_eq!(quasi_coupon_date2, d(28, Month::February, 2017));
        assert_eq!(quasi_coupon_date1, d(31, Month::August, 2016));

        let dc = ActualActual::with_schedule(Convention::ISMA, schedule.clone());
        let expected = 0.6160220994;

        let t_with_reference = dc.year_fraction_ref(
            issue_date,
            first_coupon_date,
            quasi_coupon_date2,
            first_coupon_date,
        );
        let t_no_reference = dc.year_fraction(issue_date, first_coupon_date);
        let t_total = isma_year_fraction_with_reference_dates(
            &dc,
            issue_date,
            quasi_coupon_date2,
            quasi_coupon_date1,
            quasi_coupon_date2,
        ) + 0.5;

        assert!((t_total - expected).abs() < TOL, "got {t_total}");
        assert!(
            (t_with_reference - expected).abs() < TOL,
            "got {t_with_reference}"
        );
        assert!((t_no_reference - t_with_reference).abs() < TOL);

        // Settlement date in the first quasi-period.
        let settlement_date = d(29, Month::January, 2017);
        let t_expected_first_qp = 0.03314917127071823;
        let t_with_reference = isma_year_fraction_with_reference_dates(
            &dc,
            issue_date,
            settlement_date,
            quasi_coupon_date1,
            quasi_coupon_date2,
        );
        let t_no_reference = dc.year_fraction(issue_date, settlement_date);
        assert!((t_with_reference - t_expected_first_qp).abs() < TOL);
        assert!((t_no_reference - t_with_reference).abs() < TOL);

        let t2 = dc.year_fraction(settlement_date, first_coupon_date);
        assert!((t_expected_first_qp + t2 - expected).abs() < TOL);

        // Settlement date in the second quasi-period.
        let settlement_date = d(29, Month::July, 2017);
        let t_no_reference = dc.year_fraction(issue_date, settlement_date);
        let t_with_reference = isma_year_fraction_with_reference_dates(
            &dc,
            issue_date,
            quasi_coupon_date2,
            quasi_coupon_date1,
            quasi_coupon_date2,
        ) + isma_year_fraction_with_reference_dates(
            &dc,
            quasi_coupon_date2,
            settlement_date,
            quasi_coupon_date2,
            first_coupon_date,
        );
        assert!((t_no_reference - t_with_reference).abs() < TOL);

        let t2 = dc.year_fraction(settlement_date, first_coupon_date);
        assert!((t_total - (t_no_reference + t2)).abs() < TOL);
    }

    /// Port of `testActualActualOutOfScheduleRange`. QuantLib raises; the
    /// infallible trait signature makes this a panic here.
    #[test]
    #[should_panic(expected = "dates out of range of schedule")]
    fn isma_with_schedule_rejects_dates_out_of_range() {
        let schedule = Schedule::new(
            d(21, Month::May, 2019),
            d(21, Month::May, 2029),
            1 * TimeUnit::Years,
            China::new(ChinaMarket::Ib),
            BusinessDayConvention::Unadjusted,
            BusinessDayConvention::Unadjusted,
            DateGeneration::Backward,
            false,
            Date::null(),
            Date::null(),
        );
        let dc = ActualActual::with_schedule(Convention::Bond, schedule);
        let today = d(10, Month::November, 2020);
        dc.year_fraction(today, today + 9 * TimeUnit::Years);
    }

    #[test]
    fn reversed_dates_negate() {
        let dc = ActualActual::with_convention(Convention::ISDA);
        let d1 = d(1, Month::November, 2003);
        let d2 = d(1, Month::May, 2004);
        assert!((dc.year_fraction(d2, d1) + dc.year_fraction(d1, d2)).abs() < TOL);
    }
}
