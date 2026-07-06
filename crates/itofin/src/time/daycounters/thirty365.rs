//! 30/365 day count convention.
//!
//! Port of `ql/time/daycounters/thirty365.{hpp,cpp}`. Days are counted with
//! the ISO 20022 30/360-style adjustment (day 31 becomes 30) but the year
//! fraction divides by 365.

use crate::shared::shared;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The 30/365 day count convention.
pub struct Thirty365;

impl Thirty365 {
    /// Builds a 30/365 counter.
    pub fn new() -> DayCounter {
        DayCounter::from_impl(shared(Impl))
    }
}

struct Impl;

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        "30/365".to_string()
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        let (mut dd1, mut dd2) = (d1.day_of_month(), d2.day_of_month());
        let (mm1, mm2) = (d1.month().ordinal(), d2.month().ordinal());
        let (yy1, yy2) = (d1.year(), d2.year());

        if dd1 == 31 {
            dd1 = 30;
        }
        if dd2 == 31 {
            dd2 = 30;
        }

        360 * (yy2 - yy1) + 30 * (mm2 - mm1) + (dd2 - dd1)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 365.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn name_is_thirty_over_365() {
        assert_eq!(Thirty365::new().name(), "30/365");
    }

    #[test]
    fn matches_quantlib_day_counts_and_year_fractions() {
        let cases = [
            (
                Date::new(17, Month::June, 2011),
                Date::new(30, Month::December, 2012),
                553,
            ),
            (
                Date::new(31, Month::March, 2025),
                Date::new(30, Month::April, 2025),
                30,
            ),
            (
                Date::new(30, Month::September, 2024),
                Date::new(31, Month::March, 2025),
                180,
            ),
            (
                Date::new(30, Month::March, 2025),
                Date::new(31, Month::March, 2025),
                0,
            ),
        ];

        let dc = Thirty365::new();
        for (d1, d2, expected) in cases {
            assert_eq!(dc.day_count(d1, d2), expected, "from {d1} to {d2}");
            let expected_time = Time::from(expected) / 365.0;
            assert!(
                (dc.year_fraction(d1, d2) - expected_time).abs() <= 1.0e-12,
                "from {d1} to {d2}"
            );
        }
    }
}
