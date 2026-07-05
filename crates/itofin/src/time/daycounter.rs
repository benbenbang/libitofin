//! Day-counter base: measuring a period as day counts and year fractions.
//!
//! Port of `ql/time/daycounter.{hpp,cpp}`. QuantLib uses the Bridge pattern: a
//! `DayCounter` value holds a `shared_ptr<Impl>`, and each convention subclasses
//! `Impl` to answer [`DayCounterImpl::day_count`] and
//! [`DayCounterImpl::year_fraction`]. Here the same split is expressed with a
//! [`DayCounterImpl`] trait object behind a [`Shared`]: [`DayCounter`] holds the
//! shared implementation and forwards to it. The concrete conventions live in
//! [`daycounters`](crate::time::daycounters).
//!
//! ## Divergences from QuantLib
//!
//! QuantLib's default-constructed `DayCounter` is *empty* (a null `impl_`), and
//! its accessors `QL_REQUIRE` a non-null implementation. This port omits the
//! empty state: a [`DayCounter`] always wraps a concrete implementation, so the
//! wrapper's accessors never trip the null-implementation check QuantLib guards
//! against. (This does not make every call infallible: individual conventions
//! may still panic on their own preconditions - for example the Canadian and
//! ISMA counters require a valid reference period - as documented in their
//! `# Panics` sections.) The empty placeholder is only used by higher layers
//! (schedules, coupons) as a "not yet set" marker; it will be reintroduced as an
//! `Option<DayCounter>` at those call sites when they are ported, keeping the
//! counter type itself always-valid.

use std::fmt;

use crate::shared::Shared;
use crate::time::date::{Date, SerialNumber};
use crate::types::Time;

/// The convention-specific behaviour behind a [`DayCounter`].
///
/// Mirrors QuantLib's `DayCounter::Impl`: [`day_count`](Self::day_count) has a
/// default (the raw serial-number difference `d2 - d1`) that simple counters
/// inherit, while [`year_fraction`](Self::year_fraction) is always convention
/// specific. The four-date `year_fraction` signature keeps QuantLib's reference
/// period, which only the schedule-aware conventions (Actual/Actual ISMA,
/// Actual/365 Canadian) consult.
pub trait DayCounterImpl {
    /// The counter's name, used for display and equality.
    fn name(&self) -> String;

    /// The number of days between `d1` and `d2`.
    ///
    /// Defaults to the plain serial-number difference; conventions such as
    /// 30/360 or Actual/365 (No Leap) override it.
    fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        d2 - d1
    }

    /// The period `[d1, d2]` as a fraction of a year, given an optional
    /// reference period `[ref_period_start, ref_period_end]`.
    ///
    /// Conventions that ignore the reference period take
    /// [`Date::null`](crate::time::date::Date::null) for the unused arguments;
    /// [`DayCounter::year_fraction`] fills those in.
    fn year_fraction(
        &self,
        d1: Date,
        d2: Date,
        ref_period_start: Date,
        ref_period_end: Date,
    ) -> Time;
}

/// A day-counting convention: the length of a period as a day count and as a
/// fraction of a year.
///
/// Cloning is cheap and shares the underlying implementation. Concrete
/// conventions in [`daycounters`](crate::time::daycounters) build one via
/// [`from_impl`](Self::from_impl).
#[derive(Clone)]
pub struct DayCounter {
    imp: Shared<dyn DayCounterImpl>,
}

impl DayCounter {
    /// Wraps a concrete [`DayCounterImpl`] into a usable day counter. Concrete
    /// conventions call this from their own constructors.
    pub fn from_impl(imp: Shared<dyn DayCounterImpl>) -> DayCounter {
        DayCounter { imp }
    }

    /// The name of the day counter (e.g. `Actual/360`).
    pub fn name(&self) -> String {
        self.imp.name()
    }

    /// The number of days between `d1` and `d2` under this convention.
    pub fn day_count(&self, d1: Date, d2: Date) -> SerialNumber {
        self.imp.day_count(d1, d2)
    }

    /// The period `[d1, d2]` as a fraction of a year.
    ///
    /// Equivalent to QuantLib's two-argument `yearFraction`; the reference
    /// period defaults to null. Use [`year_fraction_ref`](Self::year_fraction_ref)
    /// for conventions (Actual/Actual ISMA, Actual/365 Canadian) that need it.
    pub fn year_fraction(&self, d1: Date, d2: Date) -> Time {
        self.imp.year_fraction(d1, d2, Date::null(), Date::null())
    }

    /// The period `[d1, d2]` as a fraction of a year, given an explicit
    /// reference period `[ref_period_start, ref_period_end]`.
    pub fn year_fraction_ref(
        &self,
        d1: Date,
        d2: Date,
        ref_period_start: Date,
        ref_period_end: Date,
    ) -> Time {
        self.imp
            .year_fraction(d1, d2, ref_period_start, ref_period_end)
    }
}

impl fmt::Debug for DayCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DayCounter")
            .field("name", &self.name())
            .finish()
    }
}

impl fmt::Display for DayCounter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name())
    }
}

impl PartialEq for DayCounter {
    /// Two day counters are equal iff they share the same name, matching
    /// QuantLib's `operator==` (which compares the derived-class name).
    ///
    /// Because equality is purely by name, a custom [`DayCounterImpl`] must
    /// return a name distinct from the built-in conventions (and from other
    /// customs) or it will compare equal to them despite different behaviour -
    /// the same caveat applies to user-defined day counters in QuantLib.
    fn eq(&self, other: &DayCounter) -> bool {
        self.name() == other.name()
    }
}

impl Eq for DayCounter {}
