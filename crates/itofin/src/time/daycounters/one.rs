//! 1/1 day count convention.
//!
//! Port of `ql/time/daycounters/one.hpp`. Every period counts as one year
//! (one day), carrying only the sign of the interval.

use crate::shared::shared;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The 1/1 day count convention.
pub struct OneDayCounter;

impl OneDayCounter {
    /// Builds a 1/1 counter.
    pub fn new() -> DayCounter {
        DayCounter::from_impl(shared(Impl))
    }
}

struct Impl;

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        "1/1".to_string()
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        if d2 >= d1 { 1 } else { -1 }
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    #[test]
    fn name_is_one_over_one() {
        assert_eq!(OneDayCounter::new().name(), "1/1");
    }

    #[test]
    fn day_count_carries_only_the_sign() {
        let dc = OneDayCounter::new();
        let d1 = Date::new(1, Month::January, 2004);
        let d2 = Date::new(1, Month::July, 2004);
        assert_eq!(dc.day_count(d1, d2), 1);
        assert_eq!(dc.day_count(d1, d1), 1);
        assert_eq!(dc.day_count(d2, d1), -1);
    }

    #[test]
    fn year_fraction_is_one_for_any_period() {
        let periods = [
            Period::new(3, TimeUnit::Months),
            Period::new(6, TimeUnit::Months),
            Period::new(1, TimeUnit::Years),
        ];
        let dc = OneDayCounter::new();
        let first = Date::new(1, Month::January, 2004);
        let last = Date::new(31, Month::December, 2004);
        let mut start = first;
        while start <= last {
            for p in periods {
                let end = start + p;
                assert!(
                    (dc.year_fraction(start, end) - 1.0).abs() <= 1.0e-12,
                    "from {start} to {end}"
                );
            }
            start += 1;
        }
    }
}
