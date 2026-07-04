//! Length-plus-unit time spans.
//!
//! Port of `ql/time/period.{hpp,cpp}`: a [`Period`] is an integer length in a
//! [`TimeUnit`], with the limited algebra QuantLib defines - arithmetic
//! operators, a *partial* ordering, and normalization. The
//! [`Frequency`](crate::time::frequency::Frequency) conversions and the
//! `years`/`months`/`weeks`/`days` helpers live alongside this type.
//!
//! # Divergences from QuantLib
//!
//! QuantLib's `operator<`/`operator==` *throw* when a comparison is undecidable
//! (e.g. `1 Month` vs `30 Days`, whose day ranges overlap). This port instead
//! models the relation as a genuine partial order: [`PartialOrd::partial_cmp`]
//! returns `None` for those pairs and `==` is then simply `false`, so ordinary
//! comparison never panics. Every *decidable* comparison matches QuantLib
//! exactly. Only the four calendar units (`Days`/`Weeks`/`Months`/`Years`)
//! participate in the algebra, mirroring QuantLib; the sub-day units panic if
//! fed to it.

use std::cmp::Ordering;
use std::fmt;
use std::ops::{Add, AddAssign, Div, DivAssign, Mul, MulAssign, Neg, Sub, SubAssign};

use crate::time::timeunit::TimeUnit;
use crate::types::Integer;

/// A time span expressed as an integer `length` of a given [`TimeUnit`].
///
/// The length may be negative to express a span into the past. Equality and
/// ordering are *semantic* (`12 Months == 1 Year`), not structural; see the
/// [module docs](self) for the partial-order behaviour.
#[derive(Clone, Copy, Debug)]
pub struct Period {
    length: Integer,
    units: TimeUnit,
}

impl Period {
    /// Builds a period of `n` units.
    pub fn new(n: Integer, units: TimeUnit) -> Period {
        Period { length: n, units }
    }

    /// The (signed) number of units in the period.
    pub fn length(&self) -> Integer {
        self.length
    }

    /// The unit the period is measured in.
    pub fn units(&self) -> TimeUnit {
        self.units
    }

    /// Rewrites the period into a canonical unit where possible: a zero length
    /// becomes `Days`, a whole number of months collapses to years, and a whole
    /// number of days collapses to weeks. Weeks and years are already canonical.
    ///
    /// # Panics
    ///
    /// Panics if the period uses a sub-day unit, which has no place in the
    /// calendar algebra (matching QuantLib's `QL_FAIL`).
    pub fn normalize(&mut self) {
        if self.length == 0 {
            self.units = TimeUnit::Days;
            return;
        }
        match self.units {
            TimeUnit::Months if self.length % 12 == 0 => {
                self.length /= 12;
                self.units = TimeUnit::Years;
            }
            TimeUnit::Days if self.length % 7 == 0 => {
                self.length /= 7;
                self.units = TimeUnit::Weeks;
            }
            TimeUnit::Days | TimeUnit::Weeks | TimeUnit::Months | TimeUnit::Years => {}
            other => panic!("cannot normalize a period in {other}"),
        }
    }

    /// Returns a [`normalize`](Self::normalize)d copy, leaving `self` untouched.
    pub fn normalized(&self) -> Period {
        let mut p = *self;
        p.normalize();
        p
    }
}

impl Default for Period {
    /// The empty span, `0 Days` (QuantLib's default-constructed `Period`).
    fn default() -> Period {
        Period::new(0, TimeUnit::Days)
    }
}

/// The `[min, max]` number of days a period can span, used to compare periods
/// whose units are not exactly convertible. Months are 28-31 days, years
/// 365-366.
fn days_min_max(p: &Period) -> (Integer, Integer) {
    match p.units {
        TimeUnit::Days => (p.length, p.length),
        TimeUnit::Weeks => (7 * p.length, 7 * p.length),
        TimeUnit::Months => (28 * p.length, 31 * p.length),
        TimeUnit::Years => (365 * p.length, 366 * p.length),
        other => panic!("cannot compare a period in {other}"),
    }
}

/// Whether `p1 < p2`, or `None` when the ordering is undecidable (overlapping
/// day ranges). Faithful port of QuantLib's `operator<`, with its throw turned
/// into `None`.
fn less(p1: &Period, p2: &Period) -> Option<bool> {
    // special cases: a zero length compares against the sign of the other
    if p1.length == 0 {
        return Some(p2.length > 0);
    }
    if p2.length == 0 {
        return Some(p1.length < 0);
    }

    // exact comparisons
    if p1.units == p2.units {
        return Some(p1.length < p2.length);
    }
    match (p1.units, p2.units) {
        (TimeUnit::Months, TimeUnit::Years) => return Some(p1.length < 12 * p2.length),
        (TimeUnit::Years, TimeUnit::Months) => return Some(12 * p1.length < p2.length),
        (TimeUnit::Days, TimeUnit::Weeks) => return Some(p1.length < 7 * p2.length),
        (TimeUnit::Weeks, TimeUnit::Days) => return Some(7 * p1.length < p2.length),
        _ => {}
    }

    // inexact comparison via day ranges
    let (p1lo, p1hi) = days_min_max(p1);
    let (p2lo, p2hi) = days_min_max(p2);
    if p1hi < p2lo {
        Some(true)
    } else if p1lo > p2hi {
        Some(false)
    } else {
        None
    }
}

impl PartialOrd for Period {
    /// Returns `None` when the comparison is undecidable (see [module docs](self)).
    fn partial_cmp(&self, other: &Period) -> Option<Ordering> {
        match (less(self, other), less(other, self)) {
            (Some(true), _) => Some(Ordering::Less),
            (_, Some(true)) => Some(Ordering::Greater),
            (Some(false), Some(false)) => Some(Ordering::Equal),
            _ => None,
        }
    }
}

impl PartialEq for Period {
    /// Semantic equality: `true` only for a decidably-equal pair (an undecidable
    /// comparison is not equal).
    fn eq(&self, other: &Period) -> bool {
        self.partial_cmp(other) == Some(Ordering::Equal)
    }
}

impl fmt::Display for Period {
    /// Short format, e.g. `2W`, `3M`, `-1Y` (QuantLib's `io::short_period`).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let unit = match self.units {
            TimeUnit::Days => "D",
            TimeUnit::Weeks => "W",
            TimeUnit::Months => "M",
            TimeUnit::Years => "Y",
            TimeUnit::Hours => "h",
            TimeUnit::Minutes => "m",
            TimeUnit::Seconds => "s",
            TimeUnit::Milliseconds => "ms",
            TimeUnit::Microseconds => "us",
        };
        write!(f, "{}{}", self.length, unit)
    }
}

impl Neg for Period {
    type Output = Period;
    fn neg(self) -> Period {
        Period::new(-self.length, self.units)
    }
}

impl AddAssign for Period {
    /// Adds `p` in place, converting units where the algebra allows.
    ///
    /// # Panics
    ///
    /// Panics on an impossible addition, e.g. a non-zero `Days` period added to
    /// a `Years` one (matching QuantLib's `QL_REQUIRE`).
    fn add_assign(&mut self, p: Period) {
        if self.length == 0 {
            self.length = p.length;
            self.units = p.units;
            return;
        }
        if self.units == p.units {
            self.length += p.length;
            return;
        }
        match (self.units, p.units) {
            (TimeUnit::Years, TimeUnit::Months) => {
                self.units = TimeUnit::Months;
                self.length = self.length * 12 + p.length;
            }
            (TimeUnit::Months, TimeUnit::Years) => self.length += p.length * 12,
            (TimeUnit::Weeks, TimeUnit::Days) => {
                self.units = TimeUnit::Days;
                self.length = self.length * 7 + p.length;
            }
            (TimeUnit::Days, TimeUnit::Weeks) => self.length += p.length * 7,
            _ => assert!(p.length == 0, "impossible addition between {self} and {p}"),
        }
    }
}

impl Add for Period {
    type Output = Period;
    fn add(mut self, p: Period) -> Period {
        self += p;
        self
    }
}

impl SubAssign for Period {
    fn sub_assign(&mut self, p: Period) {
        *self += -p;
    }
}

impl Sub for Period {
    type Output = Period;
    fn sub(self, p: Period) -> Period {
        self + (-p)
    }
}

impl MulAssign<Integer> for Period {
    fn mul_assign(&mut self, n: Integer) {
        self.length *= n;
    }
}

impl Mul<Integer> for Period {
    type Output = Period;
    fn mul(self, n: Integer) -> Period {
        Period::new(self.length * n, self.units)
    }
}

impl Mul<Period> for Integer {
    type Output = Period;
    fn mul(self, p: Period) -> Period {
        Period::new(self * p.length, p.units)
    }
}

/// `n * unit` construction sugar, mirroring QuantLib's `operator*(T, TimeUnit)`:
/// `3 * TimeUnit::Days == Period::new(3, Days)`.
impl Mul<TimeUnit> for Integer {
    type Output = Period;
    fn mul(self, units: TimeUnit) -> Period {
        Period::new(self, units)
    }
}

impl Mul<Integer> for TimeUnit {
    type Output = Period;
    fn mul(self, n: Integer) -> Period {
        Period::new(n, self)
    }
}

impl DivAssign<Integer> for Period {
    /// Divides in place, keeping the original unit when it divides evenly and
    /// otherwise falling back to the finer unit (years -> months, weeks -> days).
    ///
    /// # Panics
    ///
    /// Panics if `n` is zero, or if the period is not divisible by `n` even
    /// after refining the unit (matching QuantLib's `QL_REQUIRE`).
    fn div_assign(&mut self, n: Integer) {
        assert!(n != 0, "cannot divide {self} by zero");
        if self.length % n == 0 {
            self.length /= n;
            return;
        }
        // refine the unit and retry, e.g. halving a 1-year period yields 6 months
        let (mut length, mut units) = (self.length, self.units);
        match units {
            TimeUnit::Years => {
                length *= 12;
                units = TimeUnit::Months;
            }
            TimeUnit::Weeks => {
                length *= 7;
                units = TimeUnit::Days;
            }
            _ => {}
        }
        assert!(length % n == 0, "{self} cannot be divided by {n}");
        self.length = length / n;
        self.units = units;
    }
}

impl Div<Integer> for Period {
    type Output = Period;
    fn div(mut self, n: Integer) -> Period {
        self /= n;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TimeUnit::{Days, Months, Weeks, Years};

    #[test]
    fn stores_length_and_units() {
        let p = Period::new(3, Months);
        assert_eq!(p.length(), 3);
        assert_eq!(p.units(), Months);
    }

    #[test]
    fn construction_sugar() {
        assert_eq!(3 * Days, Period::new(3, Days));
        assert_eq!(Weeks * 2, Period::new(2, Weeks));
    }

    // test-suite/period.cpp testYearsMonthsAlgebra
    #[test]
    fn years_months_algebra() {
        let one_year = Period::new(1, Years);
        let three_months = Period::new(3, Months);
        assert_eq!(one_year / 12, Period::new(1, Months));
        assert_eq!(one_year / 4, Period::new(3, Months));

        let mut sum = three_months;
        sum += Period::new(6, Months);
        assert_eq!(sum, Period::new(9, Months));
        sum += one_year;
        assert_eq!(sum, Period::new(21, Months));
    }

    // test-suite/period.cpp testWeeksDaysAlgebra
    #[test]
    fn weeks_days_algebra() {
        let two_weeks = Period::new(2, Weeks);
        let one_week = Period::new(1, Weeks);
        let one_day = Period::new(1, Days);
        assert_eq!(two_weeks / 7, Period::new(2, Days));
        assert_eq!(one_week / 7, Period::new(1, Days));

        let mut sum = Period::new(3, Days);
        sum += one_week;
        assert_eq!(sum, Period::new(10, Days));
        sum += Period::new(1, Days);
        assert_eq!(sum, Period::new(11, Days));

        assert_eq!(one_week + Period::new(0, Days), one_week);
        assert_eq!(one_week + 3 * one_day, Period::new(10, Days));
        assert_eq!(one_week + 7 * one_day, two_weeks);
    }

    // test-suite/period.cpp testOperators
    #[test]
    fn operators() {
        let mut p = Period::new(3, Days);
        p *= 2;
        assert_eq!(p, Period::new(6, Days));
        p -= Period::new(2, Days);
        assert_eq!(p, Period::new(4, Days));
        assert_eq!(-Period::new(5, Months), Period::new(-5, Months));
    }

    #[test]
    #[should_panic(expected = "impossible addition")]
    fn impossible_addition_panics() {
        let _ = Period::new(1, Years) + Period::new(1, Days);
    }

    #[test]
    #[should_panic(expected = "cannot be divided")]
    fn indivisible_division_panics() {
        let _ = Period::new(5, Days) / 2;
    }

    // test-suite/period.cpp testNormalization (representative cases)
    #[test]
    fn normalization() {
        assert_eq!(Period::new(0, Months).normalized(), Period::new(0, Days));
        assert_eq!(Period::new(12, Months).normalized(), Period::new(1, Years));
        assert_eq!(Period::new(24, Months).normalized(), Period::new(2, Years));
        assert_eq!(Period::new(7, Days).normalized(), Period::new(1, Weeks));
        assert_eq!(Period::new(14, Days).normalized(), Period::new(2, Weeks));
        // already canonical / not evenly divisible: unchanged
        assert_eq!(
            Period::new(18, Months).normalized(),
            Period::new(18, Months)
        );
        assert_eq!(Period::new(3, Days).normalized(), Period::new(3, Days));
    }

    #[test]
    fn semantic_equality_and_ordering() {
        // equal across units
        assert_eq!(Period::new(12, Months), Period::new(1, Years));
        assert_eq!(Period::new(7, Days), Period::new(1, Weeks));
        // strict ordering, exact
        assert!(Period::new(6, Months) < Period::new(1, Years));
        assert!(Period::new(2, Weeks) > Period::new(10, Days));
        // zero-length special cases
        assert!(Period::new(0, Days) < Period::new(1, Days));
        assert!(Period::new(-1, Days) < Period::new(0, Days));
    }

    #[test]
    fn undecidable_comparison_is_none() {
        // 1 month (28-31 days) vs 30 days: ranges overlap -> undecidable
        let a = Period::new(1, Months);
        let b = Period::new(30, Days);
        assert_eq!(a.partial_cmp(&b), None);
        assert!(a != b);
    }
}
