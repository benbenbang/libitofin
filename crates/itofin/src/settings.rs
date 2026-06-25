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

use crate::patterns::observable::{Observable, Observer};
use crate::shared::SharedMut;

/// Run-time settings governing pricing.
///
/// `D` is the evaluation-date payload (a `Date` once EPIC-2 is ported).
pub struct Settings<D> {
    evaluation_date: Option<D>,
    eval_date_observable: Observable,
    include_reference_date_events: bool,
    include_todays_cash_flows: Option<bool>,
    enforces_todays_historic_fixings: bool,
}

impl<D> Default for Settings<D> {
    fn default() -> Self {
        Settings {
            evaluation_date: None,
            eval_date_observable: Observable::new(),
            include_reference_date_events: false,
            include_todays_cash_flows: None,
            enforces_todays_historic_fixings: false,
        }
    }
}

impl<D: PartialEq + Clone> Settings<D> {
    /// Creates settings with no explicit evaluation date set.
    pub fn new() -> Self {
        Settings::default()
    }

    /// The currently set evaluation date, if any.
    ///
    /// `None` corresponds to QuantLib's "use today's date" default; the
    /// concrete today's-date fallback is resolved by the caller once `Date`
    /// exists.
    pub fn evaluation_date(&self) -> Option<&D> {
        self.evaluation_date.as_ref()
    }

    /// Sets the evaluation date, notifying observers only if it actually changed.
    pub fn set_evaluation_date(&mut self, date: D) {
        if self.evaluation_date.as_ref() != Some(&date) {
            self.evaluation_date = Some(date);
            self.eval_date_observable.notify_observers();
        }
    }

    /// Registers an observer to be notified on evaluation-date changes.
    pub fn register_eval_date_observer(&mut self, observer: &SharedMut<dyn Observer>) -> bool {
        self.eval_date_observable.register_observer(observer)
    }

    /// Whether events on the reference date are, by default, treated as not yet
    /// occurred.
    pub fn include_reference_date_events(&self) -> bool {
        self.include_reference_date_events
    }

    /// Sets the [`include_reference_date_events`](Self::include_reference_date_events) flag.
    pub fn set_include_reference_date_events(&mut self, value: bool) {
        self.include_reference_date_events = value;
    }

    /// Whether cash flows on today's date enter the NPV, when set.
    pub fn include_todays_cash_flows(&self) -> Option<bool> {
        self.include_todays_cash_flows
    }

    /// Sets the [`include_todays_cash_flows`](Self::include_todays_cash_flows) flag.
    pub fn set_include_todays_cash_flows(&mut self, value: Option<bool>) {
        self.include_todays_cash_flows = value;
    }

    /// Whether today's historic fixings are enforced.
    pub fn enforces_todays_historic_fixings(&self) -> bool {
        self.enforces_todays_historic_fixings
    }

    /// Sets the [`enforces_todays_historic_fixings`](Self::enforces_todays_historic_fixings) flag.
    pub fn set_enforces_todays_historic_fixings(&mut self, value: bool) {
        self.enforces_todays_historic_fixings = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{SharedMut, shared_mut};

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
        let mut settings: Settings<i64> = Settings::new();
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
        assert_eq!(settings.evaluation_date(), Some(&d2));
    }

    #[test]
    fn flags_round_trip() {
        let mut settings: Settings<i64> = Settings::new();
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
}
