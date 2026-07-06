//! Simple day counter for reproducing theoretical calculations.
//!
//! Port of `ql/time/daycounters/simpledaycounter.{hpp,cpp}`. Whole-month
//! distances come out as simple fractions (1 year = 1.0, 6 months = 0.5,
//! 3 months = 0.25 and so forth); other periods fall back to 30/360
//! (Bond Basis).
//!
//! As in QuantLib, this counter should be used together with a null calendar,
//! which keeps dates at whole-month distances on the same day of month; it is
//! not guaranteed to work with any other calendar.

use crate::shared::shared;
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::time::daycounters::thirty360::{Convention, Thirty360};
use crate::types::Time;

/// The simple day count convention.
pub struct SimpleDayCounter;

impl SimpleDayCounter {
    /// Builds a simple day counter.
    pub fn new() -> DayCounter {
        DayCounter::from_impl(shared(Impl {
            fallback: Thirty360::with_convention(Convention::BondBasis),
        }))
    }
}

struct Impl {
    fallback: DayCounter,
}

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        "Simple".to_string()
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        self.fallback.day_count(d1, d2)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        let dm1 = d1.day_of_month();
        let dm2 = d2.day_of_month();

        if dm1 == dm2
            || (dm1 > dm2 && Date::is_end_of_month(d2))
            || (dm1 < dm2 && Date::is_end_of_month(d1))
        {
            Time::from(d2.year() - d1.year())
                + Time::from(d2.month().ordinal() - d1.month().ordinal()) / 12.0
        } else {
            self.fallback.year_fraction(d1, d2)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    #[test]
    fn name_is_simple() {
        assert_eq!(SimpleDayCounter::new().name(), "Simple");
    }

    #[test]
    fn whole_month_distances_are_simple_fractions() {
        let cases = [
            (Period::new(3, TimeUnit::Months), 0.25),
            (Period::new(6, TimeUnit::Months), 0.5),
            (Period::new(1, TimeUnit::Years), 1.0),
        ];
        let dc = SimpleDayCounter::new();
        let first = Date::new(1, Month::January, 2002);
        let last = Date::new(31, Month::December, 2005);
        let mut start = first;
        while start <= last {
            for (p, expected) in cases {
                let end = start + p;
                let calculated = dc.year_fraction(start, end);
                assert!(
                    (calculated - expected).abs() <= 1.0e-12,
                    "from {start} to {end}: calculated {calculated}, expected {expected}"
                );
            }
            start += 1;
        }
    }

    #[test]
    fn broken_periods_fall_back_to_thirty360() {
        let dc = SimpleDayCounter::new();
        let fallback = Thirty360::with_convention(Convention::BondBasis);
        let d1 = Date::new(3, Month::February, 2002);
        let d2 = Date::new(17, Month::June, 2002);
        assert_eq!(dc.year_fraction(d1, d2), fallback.year_fraction(d1, d2));
        assert_eq!(dc.day_count(d1, d2), fallback.day_count(d1, d2));
    }
}
