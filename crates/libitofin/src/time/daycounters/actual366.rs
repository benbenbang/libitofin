//! Actual/366 day count convention.
//!
//! Port of `ql/time/daycounters/actual366.hpp`. Also known as "Act/366". An
//! optional flag counts the last day as well, giving the "(inc)" variant.

use crate::shared::shared;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The Actual/366 day count convention.
pub struct Actual366;

impl Actual366 {
    /// Builds an Actual/366 counter that excludes the last day.
    pub fn new() -> DayCounter {
        Self::with_last_day(false)
    }

    /// Builds an Actual/366 counter, optionally including the last day (the
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
            "Actual/366 (inc)".to_string()
        } else {
            "Actual/366".to_string()
        }
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        (d2 - d1) + SerialNumber::from(self.include_last_day)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 366.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn name_reflects_include_last_day() {
        assert_eq!(Actual366::new().name(), "Actual/366");
        assert_eq!(Actual366::with_last_day(true).name(), "Actual/366 (inc)");
    }

    #[test]
    fn include_last_day_adds_one() {
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(31, Month::January, 2020);
        assert_eq!(Actual366::with_last_day(true).day_count(d1, d2), 31);
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
            0.00819672131147541,
            1.27322404371585,
            0.587431693989071,
            1.0000000000000,
            1.00273224043716,
            0.0382513661202186,
            0.191256830601093,
            0.172131147540984,
            -0.16120218579235,
            0.16120218579235,
            0.19672131147541,
            0.920765027322404,
            2.21584699453552,
            6.84426229508197,
        ];

        let dc = Actual366::new();
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
