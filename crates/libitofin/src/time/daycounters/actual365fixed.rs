//! Actual/365 (Fixed) day count convention.
//!
//! Port of `ql/time/daycounters/actual365fixed.{hpp,cpp}`. Also known as
//! "Act/365 (Fixed)", "A/365 (Fixed)", or "A/365F". Three variants are
//! supported: the [`Standard`](Convention::Standard) counter, the
//! [`Canadian`](Convention::Canadian) bond counter (which needs a reference
//! period), and the [`NoLeap`](Convention::NoLeap) counter that skips leap days.
//!
//! Note: per ISDA, plain "Actual/365" (without "Fixed") is an alias for
//! Actual/Actual (ISDA), not this counter.

use crate::shared::shared;
use crate::time::date::{Date, Month, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::{Integer, Time};

/// The Actual/365 (Fixed) variant to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Convention {
    /// The plain Actual/365 (Fixed) counter.
    Standard,
    /// The Canadian bond convention, which consults the reference period.
    Canadian,
    /// The "No Leap" counter, which never counts a 29 February.
    NoLeap,
}

/// The Actual/365 (Fixed) day count convention.
pub struct Actual365Fixed;

impl Actual365Fixed {
    /// Builds the standard Actual/365 (Fixed) counter.
    pub fn new() -> DayCounter {
        Self::with_convention(Convention::Standard)
    }

    /// Builds the Actual/365 (Fixed) counter for the given variant.
    pub fn with_convention(c: Convention) -> DayCounter {
        match c {
            Convention::Standard => DayCounter::from_impl(shared(StandardImpl)),
            Convention::Canadian => DayCounter::from_impl(shared(CanadianImpl)),
            Convention::NoLeap => DayCounter::from_impl(shared(NoLeapImpl)),
        }
    }
}

struct StandardImpl;

impl DayCounterImpl for StandardImpl {
    fn name(&self) -> String {
        "Actual/365 (Fixed)".to_string()
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(d2 - d1) / 365.0
    }
}

struct CanadianImpl;

impl DayCounterImpl for CanadianImpl {
    fn name(&self) -> String {
        "Actual/365 (Fixed) Canadian Bond".to_string()
    }

    /// # Panics
    ///
    /// Panics if the reference period is absent, inverted (`ref_end` not after
    /// `ref_start`), shorter than a month, or longer than a year - the Canadian
    /// bond convention cannot infer a coupon frequency without a valid one. The
    /// absent/short/long checks mirror QuantLib's `QL_REQUIRE`s; rejecting an
    /// inverted period is a deliberate hardening - QuantLib omits that check and
    /// would derive a negative frequency and return a nonsensical fraction.
    fn year_fraction(&self, d1: Date, d2: Date, ref_start: Date, ref_end: Date) -> Time {
        if d1 == d2 {
            return 0.0;
        }

        // The reference period is needed to recover the coupon frequency.
        assert!(ref_start != Date::null(), "invalid refPeriodStart");
        assert!(ref_end != Date::null(), "invalid refPeriodEnd");
        assert!(
            ref_end > ref_start,
            "invalid reference period for Act/365 Canadian; end must be after start"
        );

        let dcs = Time::from(d2 - d1);
        let dcc = Time::from(ref_end - ref_start);
        let months = (12.0 * dcc / 365.0).round() as Integer;
        assert!(
            months != 0,
            "invalid reference period for Act/365 Canadian; must be longer than a month"
        );
        let frequency = 12 / months;
        assert!(
            frequency != 0,
            "invalid reference period for Act/365 Canadian; must not be longer than a year"
        );

        if dcs < Time::from(365 / frequency) {
            return dcs / 365.0;
        }

        1.0 / Time::from(frequency) - (dcc - dcs) / 365.0
    }
}

struct NoLeapImpl;

/// Cumulative days before the start of each (non-leap) month, `January = 0`.
const MONTH_OFFSET: [SerialNumber; 12] = [
    0, 31, 59, 90, 120, 151, // Jan - Jun
    181, 212, 243, 273, 304, 334, // Jul - Dec
];

impl DayCounterImpl for NoLeapImpl {
    fn name(&self) -> String {
        "Actual/365 (No Leap)".to_string()
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        let serial = |d: Date| {
            let mut s = d.day_of_month()
                + MONTH_OFFSET[(d.month().ordinal() - 1) as usize]
                + d.year() * 365;
            if d.month() == Month::February && d.day_of_month() == 29 {
                s -= 1;
            }
            s
        };
        serial(d2) - serial(d1)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 365.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: SerialNumber, m: Month, y: SerialNumber) -> Date {
        Date::new(day, m, y)
    }

    #[test]
    fn standard_names_and_fraction() {
        let dc = Actual365Fixed::new();
        assert_eq!(dc.name(), "Actual/365 (Fixed)");
        let start = d(1, Month::January, 2020);
        let end = d(1, Month::July, 2020);
        // 182 days.
        assert!((dc.year_fraction(start, end) - 182.0 / 365.0).abs() < 1e-12);
    }

    #[test]
    fn canadian_name() {
        assert_eq!(
            Actual365Fixed::with_convention(Convention::Canadian).name(),
            "Actual/365 (Fixed) Canadian Bond"
        );
    }

    #[test]
    fn canadian_semiannual_short_period() {
        // A period shorter than a coupon reduces to Act/365 (Fixed).
        let dc = Actual365Fixed::with_convention(Convention::Canadian);
        let d1 = d(10, Month::September, 2018);
        let d2 = d(10, Month::October, 2018);
        let rs = d(10, Month::September, 2018);
        let re = d(10, Month::March, 2019); // ~6 months -> semiannual
        assert!((dc.year_fraction_ref(d1, d2, rs, re) - 30.0 / 365.0).abs() < 1e-12);
    }

    #[test]
    #[should_panic(expected = "invalid refPeriodStart")]
    fn canadian_without_reference_panics() {
        let dc = Actual365Fixed::with_convention(Convention::Canadian);
        dc.year_fraction(d(10, Month::September, 2018), d(10, Month::September, 2019));
    }

    #[test]
    #[should_panic(expected = "must be longer than a month")]
    fn canadian_short_reference_panics() {
        let dc = Actual365Fixed::with_convention(Convention::Canadian);
        dc.year_fraction_ref(
            d(10, Month::September, 2018),
            d(12, Month::September, 2018),
            d(10, Month::September, 2018),
            d(15, Month::September, 2018),
        );
    }

    #[test]
    #[should_panic(expected = "must not be longer than a year")]
    fn canadian_long_reference_panics() {
        let dc = Actual365Fixed::with_convention(Convention::Canadian);
        dc.year_fraction_ref(
            d(8, Month::January, 2025),
            d(8, Month::January, 2027),
            d(8, Month::January, 2025),
            d(8, Month::January, 2027),
        );
    }

    #[test]
    #[should_panic(expected = "end must be after start")]
    fn canadian_inverted_reference_panics() {
        // An inverted reference period would otherwise yield a negative
        // frequency and a nonsensical fraction; reject it up front.
        let dc = Actual365Fixed::with_convention(Convention::Canadian);
        dc.year_fraction_ref(
            d(10, Month::September, 2018),
            d(10, Month::October, 2018),
            d(10, Month::March, 2019),
            d(10, Month::September, 2018),
        );
    }

    #[test]
    fn no_leap_skips_29_february() {
        let dc = Actual365Fixed::with_convention(Convention::NoLeap);
        assert_eq!(dc.name(), "Actual/365 (No Leap)");
        // A full leap year counts as 365, not 366.
        let start = d(1, Month::January, 2020);
        let end = d(1, Month::January, 2021);
        assert_eq!(dc.day_count(start, end), 365);
        assert!((dc.year_fraction(start, end) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn no_leap_spanning_leap_day() {
        let dc = Actual365Fixed::with_convention(Convention::NoLeap);
        // 28 Feb -> 1 Mar 2020 spans the (skipped) 29 Feb, so it counts 1 day.
        let start = d(28, Month::February, 2020);
        let end = d(1, Month::March, 2020);
        assert_eq!(dc.day_count(start, end), 1);
    }
}
