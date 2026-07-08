//! Actual/360 day count convention.
//!
//! Port of `ql/time/daycounters/actual360.hpp`. Also known as "Act/360" or
//! "A/360". An optional flag counts the last day as well, giving the "(inc)"
//! variant.

use crate::shared::shared;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The Actual/360 day count convention.
pub struct Actual360;

impl Actual360 {
    /// Builds an Actual/360 counter that excludes the last day.
    pub fn new() -> DayCounter {
        Self::with_last_day(false)
    }

    /// Builds an Actual/360 counter, optionally including the last day (the
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
            "Actual/360 (inc)".to_string()
        } else {
            "Actual/360".to_string()
        }
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        (d2 - d1) + SerialNumber::from(self.include_last_day)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 360.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn name_reflects_include_last_day() {
        assert_eq!(Actual360::new().name(), "Actual/360");
        assert_eq!(Actual360::with_last_day(true).name(), "Actual/360 (inc)");
    }

    #[test]
    fn day_count_is_plain_difference() {
        let dc = Actual360::new();
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(31, Month::January, 2020);
        assert_eq!(dc.day_count(d1, d2), 30);
    }

    #[test]
    fn include_last_day_adds_one() {
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(31, Month::January, 2020);
        assert_eq!(Actual360::with_last_day(true).day_count(d1, d2), 31);
    }

    #[test]
    fn year_fraction_divides_by_360() {
        let dc = Actual360::new();
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(1, Month::July, 2020);
        // 182 days between the two dates.
        assert!((dc.year_fraction(d1, d2) - 182.0 / 360.0).abs() < 1e-12);
    }

    #[test]
    fn counters_are_equal_by_name() {
        assert_eq!(Actual360::new(), Actual360::new());
        assert_ne!(Actual360::new(), Actual360::with_last_day(true));
    }
}
