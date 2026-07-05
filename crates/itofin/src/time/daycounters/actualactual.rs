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
//! (`ISMA_Impl`) used when a [`Schedule`] is supplied, and a reference-date one
//! (`Old_ISMA_Impl`) used otherwise. Only the reference-date implementation is
//! ported here, since `Schedule` (ticket #61) is not yet available; it is
//! reached through [`year_fraction_ref`](crate::time::daycounter::DayCounter::year_fraction_ref).
//! The schedule-driven overload will be added when `Schedule` lands.
//!
//! [`Schedule`]: https://www.quantlib.org/reference/class_quant_lib_1_1_schedule.html

use crate::shared::shared;
use crate::time::date::{Date, Month};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
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
}

/// `Real(d2 - d1)`, QuantLib's `daysBetween`.
fn days_between(d1: Date, d2: Date) -> Time {
    Time::from(d2 - d1)
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

    fn d(day: Integer, m: Month, y: Integer) -> Date {
        Date::new(day, m, y)
    }

    const TOL: Time = 1.0e-10;

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

    #[test]
    fn reversed_dates_negate() {
        let dc = ActualActual::with_convention(Convention::ISDA);
        let d1 = d(1, Month::November, 2003);
        let d2 = d(1, Month::May, 2004);
        assert!((dc.year_fraction(d2, d1) + dc.year_fraction(d1, d2)).abs() < TOL);
    }
}
