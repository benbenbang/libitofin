//! Actual/364 day count convention.
//!
//! Port of `ql/time/daycounters/actual364.hpp`.

use crate::shared::shared;
use crate::time::date::Date;
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The Actual/364 day count convention.
pub struct Actual364;

impl Actual364 {
    /// Builds an Actual/364 counter.
    pub fn new() -> DayCounter {
        DayCounter::from_impl(shared(Impl))
    }
}

struct Impl;

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        "Actual/364".to_string()
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(d2 - d1) / 364.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    #[test]
    fn name_is_actual_364() {
        assert_eq!(Actual364::new().name(), "Actual/364");
    }

    #[test]
    fn day_count_is_plain_difference() {
        let dc = Actual364::new();
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(31, Month::January, 2020);
        assert_eq!(dc.day_count(d1, d2), 30);
    }

    #[test]
    fn year_fraction_divides_by_364() {
        let dc = Actual364::new();
        let d1 = Date::new(1, Month::January, 2020);
        let d2 = Date::new(1, Month::July, 2020);
        assert!((dc.year_fraction(d1, d2) - 182.0 / 364.0).abs() < 1e-12);
    }
}
