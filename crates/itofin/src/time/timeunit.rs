//! Units used to describe time periods.
//!
//! Port of `ql/time/timeunit.hpp`. Calendars only ever advance by `Days`,
//! `Weeks`, `Months` or `Years`, but the full set is kept for faithfulness with
//! QuantLib (the sub-day units support the high-resolution date variant).

use std::fmt;

/// The unit in which a [`Period`](crate::time::period::Period) is measured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TimeUnit {
    /// Calendar days.
    Days,
    /// Weeks (7 days).
    Weeks,
    /// Calendar months.
    Months,
    /// Calendar years.
    Years,
    /// Hours.
    Hours,
    /// Minutes.
    Minutes,
    /// Seconds.
    Seconds,
    /// Milliseconds.
    Milliseconds,
    /// Microseconds.
    Microseconds,
}

impl fmt::Display for TimeUnit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            TimeUnit::Days => "Days",
            TimeUnit::Weeks => "Weeks",
            TimeUnit::Months => "Months",
            TimeUnit::Years => "Years",
            TimeUnit::Hours => "Hours",
            TimeUnit::Minutes => "Minutes",
            TimeUnit::Seconds => "Seconds",
            TimeUnit::Milliseconds => "Milliseconds",
            TimeUnit::Microseconds => "Microseconds",
        };
        f.write_str(name)
    }
}
