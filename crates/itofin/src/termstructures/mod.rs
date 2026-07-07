//! Term-structure base machinery.
//!
//! Port of `ql/termstructure.{hpp,cpp}`: the [`TermStructure`] trait is the
//! curve contract (C++'s virtual interface) and [`TermStructureBase`] is the
//! shared holder concrete curves embed (C++'s data members and default
//! behaviour). A term structure keeps track of its reference date in one of
//! three ways: a fixed date, a date moving off the evaluation date (advanced
//! by a number of settlement days on a calendar), or a date managed by the
//! concrete curve itself (which then overrides
//! [`reference_date`](TermStructure::reference_date)).
//!
//! ## Divergences from QuantLib
//!
//! - QuantLib's moving mode reads the global `Settings` singleton; per D5 the
//!   moving constructor takes the shared [`Settings`] handle explicitly and
//!   registers with its evaluation-date observable.
//! - A base asked for a reference date it does not manage returns an `Err`
//!   where C++ silently returns the null date.
//! - C++'s `Extrapolator` base class is folded into the holder as a flag; it
//!   can be extracted once the interpolation layer needs it.
//! - The empty `Calendar`/`DayCounter` states are `Option`s here, per the
//!   [`DayCounter`] port convention.

use std::cell::Cell;

use crate::errors::QlResult;
use crate::math::comparison::close_enough;
use crate::patterns::observable::{AsObservable, Observable, Observer};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Time};
use crate::{fail, require};

/// The lazily recomputed reference date shared between the holder and its
/// observer half (C++'s `mutable referenceDate_`/`updated_`/`moving_`).
struct ReferenceState {
    date: Cell<Date>,
    updated: Cell<bool>,
    moving: bool,
}

/// Observer half of a term structure (the C++ `TermStructure::update()`):
/// drops the cached reference date when the structure is moving and passes
/// the notification on to the structure's own observers.
struct Updater {
    reference: Shared<ReferenceState>,
    observable: Shared<Observable>,
}

impl Observer for Updater {
    fn update(&mut self) {
        if self.reference.moving {
            self.reference.updated.set(false);
        }
        self.observable.notify_observers();
    }
}

/// Shared base holder for term structures.
///
/// Concrete curves embed one, delegate the [`TermStructure`] trait's
/// `base()` accessor to it, and expose its [`observable`](Self::observable)
/// through [`AsObservable`].
pub struct TermStructureBase {
    calendar: Option<Calendar>,
    settlement_days: Option<Natural>,
    day_counter: Option<DayCounter>,
    settings: Option<SharedMut<Settings<Date>>>,
    extrapolation: Cell<bool>,
    reference: Shared<ReferenceState>,
    observable: Shared<Observable>,
    updater: SharedMut<Updater>,
}

impl TermStructureBase {
    fn assemble(
        calendar: Option<Calendar>,
        settlement_days: Option<Natural>,
        day_counter: Option<DayCounter>,
        settings: Option<SharedMut<Settings<Date>>>,
        moving: bool,
        reference_date: Date,
        updated: bool,
    ) -> TermStructureBase {
        let reference = shared(ReferenceState {
            date: Cell::new(reference_date),
            updated: Cell::new(updated),
            moving,
        });
        let observable = shared(Observable::new());
        let updater = shared_mut(Updater {
            reference: Shared::clone(&reference),
            observable: Shared::clone(&observable),
        });
        TermStructureBase {
            calendar,
            settlement_days,
            day_counter,
            settings,
            extrapolation: Cell::new(false),
            reference,
            observable,
            updater,
        }
    }

    /// Base for a curve that manages its own reference date by overriding
    /// [`TermStructure::reference_date`] (C++'s default constructor).
    pub fn new(day_counter: Option<DayCounter>) -> TermStructureBase {
        Self::assemble(None, None, day_counter, None, false, Date::null(), true)
    }

    /// Base with a fixed reference date.
    pub fn with_reference_date(
        reference_date: Date,
        calendar: Option<Calendar>,
        day_counter: Option<DayCounter>,
    ) -> TermStructureBase {
        Self::assemble(
            calendar,
            None,
            day_counter,
            None,
            false,
            reference_date,
            true,
        )
    }

    /// Base whose reference date moves off the evaluation date, advanced by
    /// `settlement_days` business days on `calendar`.
    ///
    /// Registers with the settings' evaluation-date observable: a date change
    /// invalidates the cached reference date and notifies the structure's
    /// observers.
    pub fn moving(
        settlement_days: Natural,
        calendar: Calendar,
        day_counter: Option<DayCounter>,
        settings: SharedMut<Settings<Date>>,
    ) -> TermStructureBase {
        let base = Self::assemble(
            Some(calendar),
            Some(settlement_days),
            day_counter,
            Some(settings),
            true,
            Date::null(),
            false,
        );
        base.settings
            .as_ref()
            .expect("a moving term structure holds settings")
            .borrow()
            .register_eval_date_observer(&(base.updater.clone() as SharedMut<dyn Observer>));
        base
    }

    /// The date at which discount = 1.0 and/or variance = 0.0, recomputing it
    /// off the evaluation date first when the structure is moving.
    pub fn reference_date(&self) -> QlResult<Date> {
        if !self.reference.updated.get() {
            let settings = self
                .settings
                .as_ref()
                .expect("a moving term structure holds settings");
            let Ok(settings) = settings.try_borrow() else {
                fail!("evaluation-date settings are locked during notification");
            };
            let Some(today) = settings.evaluation_date().copied() else {
                fail!("no evaluation date set: a moving term structure needs one");
            };
            let days = self
                .settlement_days
                .expect("a moving term structure holds settlement days");
            let Ok(n) = Integer::try_from(days) else {
                fail!("settlement days ({days}) overflow Integer");
            };
            let calendar = self
                .calendar
                .as_ref()
                .expect("a moving term structure holds a calendar");
            let advanced = calendar.advance(
                today,
                n,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            self.reference.date.set(advanced);
            self.reference.updated.set(true);
        }
        let date = self.reference.date.get();
        require!(
            date != Date::null(),
            "no reference date provided: construct with one or override reference_date"
        );
        Ok(date)
    }

    /// The day counter used for date/time conversion, when provided.
    pub fn day_counter(&self) -> Option<DayCounter> {
        self.day_counter.clone()
    }

    /// The calendar used for reference-date calculation, when provided.
    pub fn calendar(&self) -> Option<Calendar> {
        self.calendar.clone()
    }

    /// The settlement days used for reference-date calculation.
    pub fn settlement_days(&self) -> QlResult<Natural> {
        match self.settlement_days {
            Some(days) => Ok(days),
            None => fail!("settlement days not provided for this instance"),
        }
    }

    /// The observable notifying the structure's observers.
    pub fn observable(&self) -> &Observable {
        &self.observable
    }

    /// The structure's observer half, for registering with further
    /// observables (a quote handle, another curve); notifications received
    /// through it behave like C++'s `TermStructure::update()`.
    pub fn updater(&self) -> SharedMut<dyn Observer> {
        self.updater.clone()
    }

    /// Whether the curve answers dates/times beyond its maximum.
    pub fn allows_extrapolation(&self) -> bool {
        self.extrapolation.get()
    }

    /// Allows extrapolation past the maximum date/time.
    pub fn enable_extrapolation(&self) {
        self.extrapolation.set(true);
    }

    /// Forbids extrapolation past the maximum date/time.
    pub fn disable_extrapolation(&self) {
        self.extrapolation.set(false);
    }
}

/// Basic term-structure functionality.
///
/// Mirrors QuantLib's `TermStructure` interface; the provided methods
/// delegate to the embedded [`TermStructureBase`] exactly as the C++ base
/// class implements them, and a concrete curve overrides the ones it manages
/// itself (typically [`reference_date`](Self::reference_date) when built via
/// [`TermStructureBase::new`]).
pub trait TermStructure: AsObservable {
    /// The embedded shared holder.
    fn base(&self) -> &TermStructureBase;

    /// The latest date for which the curve can return values.
    fn max_date(&self) -> Date;

    /// The day counter used for date/time conversion, when provided.
    fn day_counter(&self) -> Option<DayCounter> {
        self.base().day_counter()
    }

    /// The calendar used for reference-date calculation, when provided.
    fn calendar(&self) -> Option<Calendar> {
        self.base().calendar()
    }

    /// The settlement days used for reference-date calculation.
    fn settlement_days(&self) -> QlResult<Natural> {
        self.base().settlement_days()
    }

    /// The date at which discount = 1.0 and/or variance = 0.0.
    fn reference_date(&self) -> QlResult<Date> {
        self.base().reference_date()
    }

    /// The period from the reference date to `date` as a year fraction.
    fn time_from_reference(&self, date: Date) -> QlResult<Time> {
        let Some(day_counter) = self.day_counter() else {
            fail!("no day counter provided for this term structure");
        };
        Ok(day_counter.year_fraction(self.reference_date()?, date))
    }

    /// The latest time for which the curve can return values.
    fn max_time(&self) -> QlResult<Time> {
        self.time_from_reference(self.max_date())
    }

    /// Whether the curve answers dates/times beyond its maximum.
    fn allows_extrapolation(&self) -> bool {
        self.base().allows_extrapolation()
    }

    /// Allows extrapolation past the maximum date/time.
    fn enable_extrapolation(&self) {
        self.base().enable_extrapolation()
    }

    /// Forbids extrapolation past the maximum date/time.
    fn disable_extrapolation(&self) {
        self.base().disable_extrapolation()
    }

    /// Date-range check: `date` must not precede the reference date nor,
    /// unless extrapolation applies, exceed the maximum date.
    fn check_range_date(&self, date: Date, extrapolate: bool) -> QlResult<()> {
        let reference = self.reference_date()?;
        require!(
            date >= reference,
            "date ({date}) before reference date ({reference})"
        );
        require!(
            extrapolate || self.allows_extrapolation() || date <= self.max_date(),
            "date ({date}) is past max curve date ({max})",
            max = self.max_date()
        );
        Ok(())
    }

    /// Time-range check: `t` must be non-negative and, unless extrapolation
    /// applies, within the maximum time.
    fn check_range_time(&self, t: Time, extrapolate: bool) -> QlResult<()> {
        if t < 0.0 || t.is_nan() {
            fail!("negative time ({t}) given");
        }
        let max_time = self.max_time()?;
        require!(
            extrapolate
                || self.allows_extrapolation()
                || t <= max_time
                || close_enough(t, max_time),
            "time ({t}) is past max curve time ({max_time})"
        );
        Ok(())
    }
}
