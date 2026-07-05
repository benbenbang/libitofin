//! 30/360 day count conventions.
//!
//! Port of `ql/time/daycounters/thirty360.{hpp,cpp}`. The 30/360 family treats
//! every month as 30 days and every year as 360, differing only in how the day
//! numbers are adjusted at month ends. Six adjustment rules are supported,
//! reached through the nine-variant [`Convention`] enum (several variants are
//! aliases that share a rule).

use crate::shared::shared;
use crate::time::date::{Date, Day, Month, SerialNumber, Year};
use crate::time::daycounter::{DayCounter, DayCounterImpl};
use crate::types::Time;

/// The 30/360 convention to build.
///
/// The variants map onto six underlying rules: [`USA`](Self::USA);
/// [`BondBasis`](Self::BondBasis)/[`ISMA`](Self::ISMA);
/// [`European`](Self::European)/[`EurobondBasis`](Self::EurobondBasis);
/// [`Italian`](Self::Italian); [`German`](Self::German)/[`ISDA`](Self::ISDA);
/// and [`NASD`](Self::NASD).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::upper_case_acronyms)]
pub enum Convention {
    /// US convention, "30/360" or "360/360".
    USA,
    /// Bond Basis, "US (ISMA)"; identical to [`ISMA`](Self::ISMA).
    BondBasis,
    /// European convention, "30E/360"; identical to
    /// [`EurobondBasis`](Self::EurobondBasis).
    European,
    /// Eurobond Basis, "30E/360".
    EurobondBasis,
    /// Italian convention.
    Italian,
    /// German convention, "30E/360 ISDA"; identical to [`ISDA`](Self::ISDA).
    German,
    /// Bond Basis; identical to [`BondBasis`](Self::BondBasis).
    ISMA,
    /// ISDA convention, "30E/360 ISDA"; consults the termination date.
    ISDA,
    /// NASD convention.
    NASD,
}

/// The 30/360 day count convention.
pub struct Thirty360;

impl Thirty360 {
    /// Builds a 30/360 counter for the given convention. The termination date
    /// defaults to null; only [`ISDA`](Convention::ISDA)/[`German`](Convention::German)
    /// consult it (via [`with_termination`](Self::with_termination)).
    pub fn with_convention(c: Convention) -> DayCounter {
        Self::with_termination(c, Date::null())
    }

    /// Builds a 30/360 counter, supplying the termination date used by the ISDA
    /// (German) rule to leave the final period's end-of-February untouched.
    pub fn with_termination(c: Convention, termination: Date) -> DayCounter {
        let rule = match c {
            Convention::USA => Rule::Us,
            Convention::BondBasis | Convention::ISMA => Rule::BondBasis,
            Convention::European | Convention::EurobondBasis => Rule::Eurobond,
            Convention::Italian => Rule::Italian,
            Convention::German | Convention::ISDA => Rule::Isda,
            Convention::NASD => Rule::Nasd,
        };
        DayCounter::from_impl(shared(Impl { rule, termination }))
    }
}

#[derive(Clone, Copy)]
enum Rule {
    Us,
    BondBasis,
    Eurobond,
    Italian,
    Isda,
    Nasd,
}

struct Impl {
    rule: Rule,
    termination: Date,
}

fn is_last_of_february(d: Day, m: Month, y: Year) -> bool {
    m == Month::February && d == 28 + SerialNumber::from(Date::is_leap(y))
}

/// The 30/360 formula once the day numbers have been adjusted; months are
/// 1-based ordinals so the NASD rule can carry into a 13th month.
fn thirty_360(yy1: Year, yy2: Year, mm1: Day, mm2: Day, dd1: Day, dd2: Day) -> SerialNumber {
    360 * (yy2 - yy1) + 30 * (mm2 - mm1) + (dd2 - dd1)
}

impl DayCounterImpl for Impl {
    fn name(&self) -> String {
        match self.rule {
            Rule::Us => "30/360 (US)",
            Rule::BondBasis => "30/360 (Bond Basis)",
            Rule::Eurobond => "30E/360 (Eurobond Basis)",
            Rule::Italian => "30/360 (Italian)",
            Rule::Isda => "30E/360 (ISDA)",
            Rule::Nasd => "30/360 (NASD)",
        }
        .to_string()
    }

    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        let (mut dd1, mut dd2) = (d1.day_of_month(), d2.day_of_month());
        let (mm1, mm2) = (d1.month(), d2.month());
        let (yy1, yy2) = (d1.year(), d2.year());
        let (om1, mut om2) = (mm1.ordinal(), mm2.ordinal());

        match self.rule {
            Rule::Us => {
                // NOTE: the order of checks is important.
                if is_last_of_february(dd1, mm1, yy1) {
                    if is_last_of_february(dd2, mm2, yy2) {
                        dd2 = 30;
                    }
                    dd1 = 30;
                }
                if dd2 == 31 && dd1 >= 30 {
                    dd2 = 30;
                }
                if dd1 == 31 {
                    dd1 = 30;
                }
            }
            Rule::BondBasis => {
                if dd1 == 31 {
                    dd1 = 30;
                }
                if dd2 == 31 && dd1 == 30 {
                    dd2 = 30;
                }
            }
            Rule::Eurobond => {
                if dd1 == 31 {
                    dd1 = 30;
                }
                if dd2 == 31 {
                    dd2 = 30;
                }
            }
            Rule::Italian => {
                if dd1 == 31 {
                    dd1 = 30;
                }
                if dd2 == 31 {
                    dd2 = 30;
                }
                if mm1 == Month::February && dd1 > 27 {
                    dd1 = 30;
                }
                if mm2 == Month::February && dd2 > 27 {
                    dd2 = 30;
                }
            }
            Rule::Isda => {
                if dd1 == 31 {
                    dd1 = 30;
                }
                if dd2 == 31 {
                    dd2 = 30;
                }
                if is_last_of_february(dd1, mm1, yy1) {
                    dd1 = 30;
                }
                if d2 != self.termination && is_last_of_february(dd2, mm2, yy2) {
                    dd2 = 30;
                }
            }
            Rule::Nasd => {
                if dd1 == 31 {
                    dd1 = 30;
                }
                if dd2 == 31 && dd1 >= 30 {
                    dd2 = 30;
                }
                if dd2 == 31 && dd1 < 30 {
                    dd2 = 1;
                    om2 += 1;
                }
            }
        }
        thirty_360(yy1, yy2, om1, om2, dd1, dd2)
    }

    fn year_fraction(&self, d1: Date, d2: Date, _ref_start: Date, _ref_end: Date) -> Time {
        Time::from(self.day_count(d1, d2)) / 360.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(day: SerialNumber, m: Month, y: SerialNumber) -> Date {
        Date::new(day, m, y)
    }

    #[test]
    fn names_match_quantlib() {
        assert_eq!(
            Thirty360::with_convention(Convention::USA).name(),
            "30/360 (US)"
        );
        assert_eq!(
            Thirty360::with_convention(Convention::BondBasis).name(),
            "30/360 (Bond Basis)"
        );
        assert_eq!(
            Thirty360::with_convention(Convention::EurobondBasis).name(),
            "30E/360 (Eurobond Basis)"
        );
        assert_eq!(
            Thirty360::with_convention(Convention::Italian).name(),
            "30/360 (Italian)"
        );
        assert_eq!(
            Thirty360::with_convention(Convention::ISDA).name(),
            "30E/360 (ISDA)"
        );
        assert_eq!(
            Thirty360::with_convention(Convention::NASD).name(),
            "30/360 (NASD)"
        );
        // Aliases resolve to the same rule/name.
        assert_eq!(
            Thirty360::with_convention(Convention::ISMA).name(),
            Thirty360::with_convention(Convention::BondBasis).name()
        );
        assert_eq!(
            Thirty360::with_convention(Convention::German).name(),
            Thirty360::with_convention(Convention::ISDA).name()
        );
        assert_eq!(
            Thirty360::with_convention(Convention::European).name(),
            Thirty360::with_convention(Convention::EurobondBasis).name()
        );
    }

    #[test]
    fn usa_known_good_values() {
        // From QuantLib's testThirty360_USA.
        let dc = Thirty360::with_convention(Convention::USA);
        let cases: &[(Date, Date, SerialNumber)] = &[
            (
                d(20, Month::August, 2006),
                d(20, Month::February, 2007),
                180,
            ),
            (
                d(31, Month::August, 2006),
                d(28, Month::February, 2007),
                178,
            ),
            (
                d(31, Month::August, 2007),
                d(29, Month::February, 2008),
                179,
            ),
            (
                d(28, Month::February, 2008),
                d(31, Month::August, 2008),
                183,
            ),
            (
                d(28, Month::February, 2008),
                d(30, Month::August, 2008),
                182,
            ),
            (
                d(29, Month::February, 2008),
                d(28, Month::February, 2009),
                360,
            ),
            (d(28, Month::February, 2008), d(31, Month::March, 2008), 33),
            (d(28, Month::February, 2006), d(3, Month::March, 2006), 3),
            (
                d(30, Month::September, 2006),
                d(31, Month::October, 2006),
                30,
            ),
        ];
        for &(start, end, expected) in cases {
            assert_eq!(dc.day_count(start, end), expected, "{start} -> {end}");
        }
        // year_fraction divides the day count by 360.
        let (s, e) = (d(20, Month::August, 2006), d(20, Month::February, 2007));
        assert!((dc.year_fraction(s, e) - 180.0 / 360.0).abs() < 1e-12);
    }

    #[test]
    fn eurobond_end_of_february_is_not_special() {
        let dc = Thirty360::with_convention(Convention::EurobondBasis);
        assert_eq!(
            dc.day_count(d(28, Month::February, 2006), d(31, Month::August, 2006)),
            182
        );
        assert_eq!(
            dc.day_count(d(31, Month::August, 2007), d(29, Month::February, 2008)),
            179
        );
    }

    #[test]
    fn isda_treats_last_of_february_as_thirty_except_termination() {
        // From QuantLib's testThirty360_ISDA (data2), termination 20 Aug 2009.
        let dc = Thirty360::with_termination(Convention::ISDA, d(20, Month::August, 2009));
        assert_eq!(
            dc.day_count(d(28, Month::February, 2006), d(31, Month::August, 2006)),
            180
        );
        assert_eq!(
            dc.day_count(d(31, Month::August, 2007), d(29, Month::February, 2008)),
            180
        );
    }

    #[test]
    fn nasd_carries_into_next_month() {
        // Ending on the 31st with a start before the 30th rolls to the 1st of
        // the following month.
        let dc = Thirty360::with_convention(Convention::NASD);
        assert_eq!(
            dc.day_count(d(15, Month::January, 2006), d(31, Month::January, 2006)),
            16
        );
    }
}
