//! Frequency of events.
//!
//! Port of `ql/time/frequency.hpp` and the `operator<<` from `frequency.cpp`.
//! The [`Period`](crate::time::period::Period) type converts to and from a
//! `Frequency` (see [`Period::frequency`](crate::time::period::Period::frequency)
//! and `TryFrom<Frequency> for Period`).

use std::fmt;

/// Frequency of events, e.g. coupon payments.
///
/// The integer discriminants match QuantLib's `Frequency` enum: each is the
/// number of events per year (`Annual = 1`, `Semiannual = 2`, `Monthly = 12`,
/// ...), which the `Period` <-> `Frequency` conversions rely on. The two
/// sentinels ([`NoFrequency`](Self::NoFrequency), [`OtherFrequency`](Self::OtherFrequency))
/// keep QuantLib's out-of-band values.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i16)]
pub enum Frequency {
    /// Null frequency.
    NoFrequency = -1,
    /// Only once, e.g. a zero-coupon.
    Once = 0,
    /// Once a year.
    Annual = 1,
    /// Twice a year.
    Semiannual = 2,
    /// Every fourth month (three times a year).
    EveryFourthMonth = 3,
    /// Every third month (four times a year).
    Quarterly = 4,
    /// Every second month (six times a year).
    Bimonthly = 6,
    /// Once a month.
    Monthly = 12,
    /// Every fourth week (thirteen times a year).
    EveryFourthWeek = 13,
    /// Every second week (twenty-six times a year).
    Biweekly = 26,
    /// Once a week.
    Weekly = 52,
    /// Once a day.
    Daily = 365,
    /// Some other, unknown frequency.
    OtherFrequency = 999,
}

impl fmt::Display for Frequency {
    /// Renders the QuantLib label, e.g. `Semiannual` or `Every-Fourth-Month`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Frequency::NoFrequency => "No-Frequency",
            Frequency::Once => "Once",
            Frequency::Annual => "Annual",
            Frequency::Semiannual => "Semiannual",
            Frequency::EveryFourthMonth => "Every-Fourth-Month",
            Frequency::Quarterly => "Quarterly",
            Frequency::Bimonthly => "Bimonthly",
            Frequency::Monthly => "Monthly",
            Frequency::EveryFourthWeek => "Every-fourth-week",
            Frequency::Biweekly => "Biweekly",
            Frequency::Weekly => "Weekly",
            Frequency::Daily => "Daily",
            Frequency::OtherFrequency => "Unknown frequency",
        };
        f.write_str(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discriminants_match_events_per_year() {
        assert_eq!(Frequency::NoFrequency as i16, -1);
        assert_eq!(Frequency::Once as i16, 0);
        assert_eq!(Frequency::Annual as i16, 1);
        assert_eq!(Frequency::Quarterly as i16, 4);
        assert_eq!(Frequency::Monthly as i16, 12);
        assert_eq!(Frequency::Weekly as i16, 52);
        assert_eq!(Frequency::Daily as i16, 365);
        assert_eq!(Frequency::OtherFrequency as i16, 999);
    }

    #[test]
    fn display_matches_quantlib_labels() {
        assert_eq!(Frequency::NoFrequency.to_string(), "No-Frequency");
        assert_eq!(
            Frequency::EveryFourthMonth.to_string(),
            "Every-Fourth-Month"
        );
        assert_eq!(Frequency::EveryFourthWeek.to_string(), "Every-fourth-week");
        assert_eq!(Frequency::OtherFrequency.to_string(), "Unknown frequency");
    }
}
