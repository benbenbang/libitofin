//! Business-day adjustment conventions.
//!
//! Port of `ql/time/businessdayconvention.hpp`. These conventions specify how a
//! date that falls on a holiday is rolled to a nearby business day; the
//! adjustment logic itself lives in [`Calendar::adjust`](crate::time::calendar::Calendar::adjust).

use std::fmt;

/// The rule used to roll a non-business day to a valid business day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum BusinessDayConvention {
    // ISDA
    /// The first business day after the given holiday.
    #[default]
    Following,
    /// Like [`Following`](Self::Following), unless it lands in a different
    /// month, in which case the first business day before the holiday.
    ModifiedFollowing,
    /// The first business day before the given holiday.
    Preceding,
    // non-ISDA
    /// Like [`Preceding`](Self::Preceding), unless it lands in a different
    /// month, in which case the first business day after the holiday.
    ModifiedPreceding,
    /// Do not adjust.
    Unadjusted,
    /// Like [`Following`](Self::Following), unless that day crosses the
    /// mid-month (15th) or the end of month, in which case the first business
    /// day before the holiday.
    HalfMonthModifiedFollowing,
    /// The nearest business day. Ties (equally far preceding and following)
    /// resolve to the following business day.
    Nearest,
}

impl fmt::Display for BusinessDayConvention {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            BusinessDayConvention::Following => "Following",
            BusinessDayConvention::ModifiedFollowing => "Modified Following",
            BusinessDayConvention::Preceding => "Preceding",
            BusinessDayConvention::ModifiedPreceding => "Modified Preceding",
            BusinessDayConvention::Unadjusted => "Unadjusted",
            BusinessDayConvention::HalfMonthModifiedFollowing => "Half-Month Modified Following",
            BusinessDayConvention::Nearest => "Nearest",
        };
        f.write_str(name)
    }
}
