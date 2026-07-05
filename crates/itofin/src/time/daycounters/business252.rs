//! Business/252 day count convention.
//!
//! Port of `ql/time/daycounters/business252.{hpp,cpp}`. Counts business days on
//! a given calendar and divides by 252, the conventional number of business
//! days in a Brazilian year. Defaults to the Brazil (settlement) calendar.
//!
//! ## Divergences from QuantLib
//!
//! QuantLib memoizes per-month and per-year business-day totals in
//! process-global `std::map`s keyed by calendar name, then stitches a day count
//! together from those cached figures. That global mutable state conflicts with
//! this port's "explicit state, no hidden singletons" decision (D5), the same
//! reason [`Calendar`] holiday overrides are
//! per-value here. This port drops the cache and counts directly with
//! [`Calendar::business_days_between`] (first day included, last excluded). The
//! two are numerically identical:
//! QuantLib's month/year decomposition is a pure caching optimization whose
//! contiguous `[include-first, exclude-last]` segments telescope back to a
//! single `businessDaysBetween(d1, d2)`.

use crate::shared::shared;
use crate::time::calendar::Calendar;
use crate::time::calendars::brazil::{Brazil, Market};
use crate::time::date::{Date, SerialNumber};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The Business/252 day count convention.
pub struct Business252;

impl Business252 {
    /// Builds a Business/252 counter on the Brazil (settlement) calendar, the
    /// QuantLib default.
    pub fn new() -> DayCounter {
        Self::with_calendar(Brazil::new(Market::Settlement))
    }

    /// Builds a Business/252 counter on the given calendar.
    pub fn with_calendar(calendar: Calendar) -> DayCounter {
        DayCounter::from_impl(shared(Impl { calendar }))
    }
}

struct Impl {
    calendar: Calendar,
}

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        format!("Business/252({})", self.calendar.name())
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        self.calendar.business_days_between(d1, d2, true, false)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 252.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::date::Month;

    fn d(day: SerialNumber, m: Month, y: SerialNumber) -> Date {
        Date::new(day, m, y)
    }

    #[test]
    fn name_includes_calendar() {
        assert_eq!(Business252::new().name(), "Business/252(Brazil)");
    }

    #[test]
    fn brazil_known_good_values() {
        // From QuantLib's testBusiness252 (consecutive pairs of test dates).
        let dc = Business252::new();
        let dates = [
            d(1, Month::February, 2002),
            d(4, Month::February, 2002),
            d(16, Month::May, 2003),
            d(17, Month::December, 2003),
            d(17, Month::December, 2004),
            d(19, Month::December, 2005),
            d(2, Month::January, 2006),
            d(13, Month::March, 2006),
            d(15, Month::May, 2006),
            d(17, Month::March, 2006),
            d(15, Month::May, 2006),
            d(26, Month::July, 2006),
            d(28, Month::June, 2007),
            d(16, Month::September, 2009),
            d(26, Month::July, 2016),
        ];
        let expected = [
            0.0039682539683,
            1.2738095238095,
            0.6031746031746,
            0.9960317460317,
            1.0000000000000,
            0.0396825396825,
            0.1904761904762,
            0.1666666666667,
            -0.1507936507937,
            0.1507936507937,
            0.2023809523810,
            0.912698412698,
            2.214285714286,
            6.84126984127,
        ];
        for (i, &e) in expected.iter().enumerate() {
            let calculated = dc.year_fraction(dates[i], dates[i + 1]);
            assert!(
                (calculated - e).abs() < 1e-12,
                "{} -> {}: got {calculated}, want {e}",
                dates[i],
                dates[i + 1]
            );
        }
    }

    #[test]
    fn reversed_dates_negate() {
        let dc = Business252::new();
        let a = d(2, Month::January, 2006);
        let b = d(13, Month::March, 2006);
        assert!((dc.year_fraction(a, b) + dc.year_fraction(b, a)).abs() < 1e-12);
    }
}
