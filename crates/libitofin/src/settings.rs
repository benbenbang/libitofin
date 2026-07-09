//! Run-time evaluation settings.
//!
//! Port of `ql/settings.{hpp,cpp}` (design decision D5). QuantLib's `Settings`
//! is a global singleton holding the evaluation date and a handful of pricing
//! flags. Per D5 we deliberately do **not** reproduce the singleton: `Settings`
//! is an explicit value object, passed by reference (`&Context`-style), so the
//! evaluation date is visible to and overridable by callers (including future
//! `rayon` compute threads, D6) rather than hidden in thread-local state.
//!
//! The evaluation date is generic over its payload `D` here because the
//! concrete `Date` type belongs to EPIC-2; once `Date` lands, downstream code
//! uses `Settings<Date>`. Assigning a *different* date notifies observers;
//! assigning the same date does not, matching `settings.cpp`.
//!
//! Mutation goes through `&self` (the state lives in [`Cell`]s), like
//! [`SimpleQuote`](crate::quotes::SimpleQuote): settings are shared as
//! [`Shared<Settings<D>>`](crate::shared::Shared) so that an observer may read
//! the evaluation date while being notified of its change, rather than meeting
//! a mutable borrow the notifying caller still holds.

use std::cell::Cell;

use crate::patterns::observable::{Observable, Observer};
use crate::shared::SharedMut;

/// Run-time settings governing pricing.
///
/// `D` is the evaluation-date payload (a `Date` once EPIC-2 is ported).
pub struct Settings<D> {
    evaluation_date: Cell<Option<D>>,
    eval_date_observable: Observable,
    include_reference_date_events: Cell<bool>,
    include_todays_cash_flows: Cell<Option<bool>>,
    enforces_todays_historic_fixings: Cell<bool>,
}

impl<D> Default for Settings<D> {
    fn default() -> Self {
        Settings {
            evaluation_date: Cell::new(None),
            eval_date_observable: Observable::new(),
            include_reference_date_events: Cell::new(false),
            include_todays_cash_flows: Cell::new(None),
            enforces_todays_historic_fixings: Cell::new(false),
        }
    }
}

impl<D> Settings<D> {
    /// Creates settings with no explicit evaluation date set.
    pub fn new() -> Self {
        Settings::default()
    }

    /// The currently set evaluation date, if any.
    ///
    /// `None` corresponds to QuantLib's "use today's date" default; the
    /// concrete today's-date fallback is resolved by the caller once `Date`
    /// exists.
    pub fn evaluation_date(&self) -> Option<D>
    where
        D: Copy,
    {
        self.evaluation_date.get()
    }

    /// Sets the evaluation date, notifying observers only if it actually changed.
    ///
    /// The new date is in place before the notification goes out, so an
    /// observer reading it back through [`evaluation_date`](Self::evaluation_date)
    /// sees the value that triggered its update.
    pub fn set_evaluation_date(&self, date: D)
    where
        D: Copy + PartialEq,
    {
        if self.evaluation_date.get() != Some(date) {
            self.evaluation_date.set(Some(date));
            self.eval_date_observable.notify_observers();
        }
    }

    /// Resets the evaluation date to the floating "use today's date" state,
    /// notifying observers only if a concrete date had been set.
    ///
    /// Mirrors QuantLib's `resetEvaluationDate()` (which assigns the null
    /// `Date()`). Its companion `anchorEvaluationDate()` is deferred until
    /// EPIC-2 provides a concrete today's-date for the payload type.
    pub fn reset_evaluation_date(&self) {
        if self.evaluation_date.replace(None).is_some() {
            self.eval_date_observable.notify_observers();
        }
    }

    /// Registers an observer to be notified on evaluation-date changes.
    pub fn register_eval_date_observer(&self, observer: &SharedMut<dyn Observer>) -> bool {
        self.eval_date_observable.register_observer(observer)
    }

    /// Whether events on the reference date are, by default, treated as not yet
    /// occurred.
    pub fn include_reference_date_events(&self) -> bool {
        self.include_reference_date_events.get()
    }

    /// Sets the [`include_reference_date_events`](Self::include_reference_date_events) flag.
    pub fn set_include_reference_date_events(&self, value: bool) {
        self.include_reference_date_events.set(value);
    }

    /// Whether cash flows on today's date enter the NPV, when set.
    pub fn include_todays_cash_flows(&self) -> Option<bool> {
        self.include_todays_cash_flows.get()
    }

    /// Sets the [`include_todays_cash_flows`](Self::include_todays_cash_flows) flag.
    pub fn set_include_todays_cash_flows(&self, value: Option<bool>) {
        self.include_todays_cash_flows.set(value);
    }

    /// Whether today's historic fixings are enforced.
    pub fn enforces_todays_historic_fixings(&self) -> bool {
        self.enforces_todays_historic_fixings.get()
    }

    /// Sets the [`enforces_todays_historic_fixings`](Self::enforces_todays_historic_fixings) flag.
    pub fn set_enforces_todays_historic_fixings(&self, value: bool) {
        self.enforces_todays_historic_fixings.set(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};

    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    /// Mirrors `settings.cpp::testNotificationsOnDateChange`, using an integer
    /// serial date in place of the EPIC-2 `Date`.
    #[test]
    fn notifies_only_on_actual_date_change() {
        let settings: Settings<i64> = Settings::new();
        let d1 = 44238_i64; // 11 Feb 2021
        let d2 = 44239_i64; // 12 Feb 2021

        settings.set_evaluation_date(d1);

        let flag = shared_mut(Flag::default());
        settings.register_eval_date_observer(&(flag.clone() as SharedMut<dyn Observer>));

        // setting to the same date sends no notification
        settings.set_evaluation_date(d1);
        assert!(!flag.borrow().up);

        // setting to a different date notifies
        settings.set_evaluation_date(d2);
        assert!(flag.borrow().up);
        assert_eq!(settings.evaluation_date(), Some(d2));
    }

    #[test]
    fn reset_returns_to_floating_and_notifies_once() {
        let settings: Settings<i64> = Settings::new();
        settings.set_evaluation_date(44238_i64);

        let flag = shared_mut(Flag::default());
        settings.register_eval_date_observer(&(flag.clone() as SharedMut<dyn Observer>));

        // resetting away from a concrete date notifies and returns to None
        settings.reset_evaluation_date();
        assert!(flag.borrow().up);
        assert_eq!(settings.evaluation_date(), None);

        // resetting again while already floating sends no notification
        flag.borrow_mut().up = false;
        settings.reset_evaluation_date();
        assert!(!flag.borrow().up);
    }

    #[test]
    fn flags_round_trip() {
        let settings: Settings<i64> = Settings::new();
        assert!(!settings.include_reference_date_events());
        assert_eq!(settings.include_todays_cash_flows(), None);
        assert!(!settings.enforces_todays_historic_fixings());

        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(true));
        settings.set_enforces_todays_historic_fixings(true);

        assert!(settings.include_reference_date_events());
        assert_eq!(settings.include_todays_cash_flows(), Some(true));
        assert!(settings.enforces_todays_historic_fixings());
    }

    /// Observer that reads the settings back while being notified of an
    /// evaluation-date change, as any structure recomputing off the new date
    /// does from its `update()`.
    struct Reader {
        settings: Shared<Settings<i64>>,
        seen: Option<i64>,
    }

    impl Observer for Reader {
        fn update(&mut self) {
            self.seen = self.settings.evaluation_date();
        }
    }

    #[test]
    fn observer_may_read_the_evaluation_date_during_notification() {
        let settings = shared(Settings::<i64>::new());
        let reader = shared_mut(Reader {
            settings: Shared::clone(&settings),
            seen: None,
        });
        settings.register_eval_date_observer(&(reader.clone() as SharedMut<dyn Observer>));

        settings.set_evaluation_date(44239_i64);
        assert_eq!(reader.borrow().seen, Some(44239));

        settings.reset_evaluation_date();
        assert_eq!(reader.borrow().seen, None);
    }
}
