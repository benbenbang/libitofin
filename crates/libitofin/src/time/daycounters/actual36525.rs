//! Actual/365.25 day count convention.
//!
//! Port of `ql/time/daycounters/actual36525.hpp`. Also known as "Act/365.25"
//! or "A/365.25". An optional flag counts the last day as well, giving the
//! "(inc)" variant.

use crate::shared::shared;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The Actual/365.25 day count convention.
pub struct Actual36525;

impl Actual36525 {
    /// Builds an Actual/365.25 counter that excludes the last day.
    pub fn new() -> DayCounter {
        Self::with_last_day(false)
    }

    /// Builds an Actual/365.25 counter, optionally including the last day (the
    /// "(inc)" variant), matching QuantLib's `includeLastDay` constructor flag.
    pub fn with_last_day(include_last_day: bool) -> DayCounter {
        DayCounter::from_impl(shared(Impl { include_last_day }))
    }
}

struct Impl {
    include_last_day: bool,
}

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        if self.include_last_day {
            "Actual/365.25 (inc)".to_string()
        } else {
            "Actual/365.25".to_string()
        }
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        (d2 - d1) + SerialNumber::from(self.include_last_day)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 365.25
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn name_reflects_include_last_day() {
        assert_eq!(Actual36525::new().name(), "Actual/365.25");
        assert_eq!(
            Actual36525::with_last_day(true).name(),
            "Actual/365.25 (inc)"
        );
    }

    #[test]
    fn include_last_day_adds_one() {
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(31, Month::January, 2020);
        assert_eq!(Actual36525::with_last_day(true).day_count(d1, d2), 31);
    }

    #[test]
    fn matches_quantlib_year_fractions() {
        let dates = [
            Date::new(1, Month::February, 2002),
            Date::new(4, Month::February, 2002),
            Date::new(16, Month::May, 2003),
            Date::new(17, Month::December, 2003),
            Date::new(17, Month::December, 2004),
            Date::new(19, Month::December, 2005),
            Date::new(2, Month::January, 2006),
            Date::new(13, Month::March, 2006),
            Date::new(15, Month::May, 2006),
            Date::new(17, Month::March, 2006),
            Date::new(15, Month::May, 2006),
            Date::new(26, Month::July, 2006),
            Date::new(28, Month::June, 2007),
            Date::new(16, Month::September, 2009),
            Date::new(26, Month::July, 2016),
        ];
        let expected = [
            0.0082135523613963,
            1.27583846680356,
            0.588637919233402,
            1.00205338809035,
            1.00479123887748,
            0.0383299110198494,
            0.191649555099247,
            0.172484599589322,
            -0.161533196440794,
            0.161533196440794,
            0.197125256673511,
            0.922655715263518,
            2.22039698836413,
            6.85831622176591,
        ];

        let dc = Actual36525::new();
        for i in 1..dates.len() {
            let calculated = dc.year_fraction(dates[i - 1], dates[i]);
            assert!(
                (calculated - expected[i - 1]).abs() <= 1.0e-12,
                "from {} to {}: calculated {calculated}, expected {}",
                dates[i - 1],
                dates[i],
                expected[i - 1]
            );
        }
    }
}
