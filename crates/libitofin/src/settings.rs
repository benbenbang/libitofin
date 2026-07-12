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
//!
//! # Past index fixings (D11)
//!
//! QuantLib keeps historical index fixings in `IndexManager::instance()`, a
//! global-singleton `map<string, TimeSeries<Real>>`. D5 forbids that singleton,
//! so per D11 the fixing history lives here on `Settings` (a `fixing_store`,
//! exactly like `evaluation_date`): explicit rather than hidden, yet shared
//! across every handle to the same index, since two handles to "the same"
//! Euribor must observe one fixing history - the guarantee C++'s global map
//! provides. Index names are matched case-insensitively, as in `IndexManager`.
//! Each index name carries its own [`Observable`], so adding a fixing notifies
//! exactly the observers of that index (mirroring `IndexManager::notifier`);
//! the notifiers outlive [`clear_fixings`](Settings::clear_fixings), so an index
//! registered once keeps hearing later fixings.

use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};

use crate::math::comparison::close;
use crate::patterns::observable::{Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;
use crate::types::Rate;

/// Run-time settings governing pricing.
///
/// `D` is the evaluation-date payload (a `Date` once EPIC-2 is ported).
pub struct Settings<D> {
    evaluation_date: Cell<Option<D>>,
    eval_date_observable: Observable,
    include_reference_date_events: Cell<bool>,
    include_todays_cash_flows: Cell<Option<bool>>,
    enforces_todays_historic_fixings: Cell<bool>,
    fixing_store: RefCell<HashMap<String, BTreeMap<Date, Rate>>>,
    fixing_notifiers: RefCell<HashMap<String, Shared<Observable>>>,
}

impl<D> Default for Settings<D> {
    fn default() -> Self {
        Settings {
            evaluation_date: Cell::new(None),
            eval_date_observable: Observable::new(),
            include_reference_date_events: Cell::new(false),
            include_todays_cash_flows: Cell::new(None),
            enforces_todays_historic_fixings: Cell::new(false),
            fixing_store: RefCell::new(HashMap::new()),
            fixing_notifiers: RefCell::new(HashMap::new()),
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
    ///
    /// The today's-fixing rule this flag governs (whether a missing fixing on
    /// the evaluation date is an error or a forecast) is applied by the index
    /// layer that reads the store, not by the store itself - as in QuantLib,
    /// where `IndexManager` records fixings and `InterestRateIndex::fixing`
    /// consults the flag.
    pub fn set_enforces_todays_historic_fixings(&self, value: bool) {
        self.enforces_todays_historic_fixings.set(value);
    }

    /// Case-insensitive lookup key for an index name (`IndexManager` semantics).
    fn fixing_key(index_name: &str) -> String {
        index_name.to_uppercase()
    }

    /// The [`Observable`] for an index name, created on first use and shared by
    /// every observer and mutation of that name's fixings.
    fn fixing_notifier(&self, key: &str) -> Shared<Observable> {
        let mut notifiers = self.fixing_notifiers.borrow_mut();
        if let Some(notifier) = notifiers.get(key) {
            return Shared::clone(notifier);
        }
        let notifier = Shared::new(Observable::new());
        notifiers.insert(key.to_string(), Shared::clone(&notifier));
        notifier
    }

    /// Registers an observer to be notified when a fixing of `index_name` is
    /// added or cleared.
    pub fn register_fixing_observer(
        &self,
        index_name: &str,
        observer: &SharedMut<dyn Observer>,
    ) -> bool {
        let key = Self::fixing_key(index_name);
        self.fixing_notifier(&key).register_observer(observer)
    }

    /// Records a single past fixing for `index_name`, notifying its observers.
    ///
    /// See [`add_fixings`](Self::add_fixings) for the overwrite rule.
    pub fn add_fixing(
        &self,
        index_name: &str,
        date: Date,
        rate: Rate,
    ) -> crate::errors::QlResult<()> {
        self.add_fixings(index_name, [(date, rate)])
    }

    /// Records several past fixings for `index_name`, notifying its observers once.
    ///
    /// A fixing that repeats an existing date with a [`close`] value is a no-op;
    /// one that conflicts with a stored value is rejected (mirroring
    /// `IndexManager::addFixings`' duplicated-fixing guard) and leaves the store
    /// unchanged. The observers of `index_name` are notified only once the store
    /// has been updated and its borrow released, so an observer reading a fixing
    /// back from its `update` sees the new value.
    pub fn add_fixings(
        &self,
        index_name: &str,
        fixings: impl IntoIterator<Item = (Date, Rate)>,
    ) -> crate::errors::QlResult<()> {
        let key = Self::fixing_key(index_name);
        let notifier = self.fixing_notifier(&key);
        {
            let mut store = self.fixing_store.borrow_mut();
            let history = store.entry(key).or_default();
            let pending: Vec<(Date, Rate)> = fixings.into_iter().collect();
            for &(date, rate) in &pending {
                if let Some(&existing) = history.get(&date) {
                    require!(
                        close(existing, rate),
                        "duplicated fixing {rate} on {date:?} while {existing} is already present"
                    );
                }
            }
            for (date, rate) in pending {
                history.insert(date, rate);
            }
        }
        notifier.notify_observers();
        Ok(())
    }

    /// The stored fixing of `index_name` on `date`, if one was recorded.
    pub fn fixing(&self, index_name: &str, date: Date) -> Option<Rate> {
        let key = Self::fixing_key(index_name);
        self.fixing_store
            .borrow()
            .get(&key)
            .and_then(|history| history.get(&date).copied())
    }

    /// Whether a fixing of `index_name` is on record for `date`.
    ///
    /// Mirrors `IndexManager::hasHistoricalFixing`.
    pub fn has_historical_fixing(&self, index_name: &str, date: Date) -> bool {
        self.fixing(index_name, date).is_some()
    }

    /// Clears the recorded fixings of a single index, notifying its observers.
    ///
    /// Mirrors `IndexManager::clearHistory`, which notifies unconditionally.
    pub fn clear_fixing(&self, index_name: &str) {
        let key = Self::fixing_key(index_name);
        self.fixing_notifier(&key).notify_observers();
        self.fixing_store.borrow_mut().remove(&key);
    }

    /// Clears every recorded fixing, notifying the observers of each index that
    /// had a history.
    ///
    /// Mirrors `IndexManager::clearHistories`. The per-index notifiers survive,
    /// so an observer registered before the clear still hears later fixings.
    pub fn clear_fixings(&self) {
        let names: Vec<String> = self.fixing_store.borrow().keys().cloned().collect();
        for name in &names {
            self.fixing_notifier(name).notify_observers();
        }
        self.fixing_store.borrow_mut().clear();
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

    use crate::time::date::Month;

    fn a_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// Reproduces `indexes.cpp::testFixingObservability`: an observer of an index
    /// is notified when a fixing of that index is added, and only that index.
    /// Reproduced against the store and a minimal observer double, since the
    /// `Index` hierarchy is #68's scope, not this ticket's.
    #[test]
    fn adding_a_fixing_notifies_observers_of_that_index_only() {
        let settings: Settings<i64> = Settings::new();
        let date = a_date();

        let f1 = shared_mut(Flag::default());
        settings.register_fixing_observer("Euribor6M", &(f1.clone() as SharedMut<dyn Observer>));
        let f2 = shared_mut(Flag::default());
        settings.register_fixing_observer("BMA", &(f2.clone() as SharedMut<dyn Observer>));

        settings.add_fixing("Euribor6M", date, -0.003).unwrap();
        assert!(f1.borrow().up);
        assert!(!f2.borrow().up);

        settings.add_fixing("BMA", date, 0.01).unwrap();
        assert!(f2.borrow().up);
    }

    /// Reproduces `indexes.cpp::testFixingHasHistoricalFixing`: a fixing is found
    /// only for the index it was registered under and only until the histories
    /// are cleared. The shared-history semantics of two C++ handles to the same
    /// index are inherent here, as any reader of the name sees the one store.
    #[test]
    fn has_historical_fixing_tracks_registered_fixings() {
        let settings: Settings<i64> = Settings::new();
        let date = a_date();

        settings.add_fixing("Euribor6M", date, 0.01).unwrap();

        assert!(!settings.has_historical_fixing("Euribor3M", date));
        assert!(settings.has_historical_fixing("Euribor6M", date));
        assert_eq!(settings.fixing("Euribor6M", date), Some(0.01));

        settings.clear_fixings();

        assert!(!settings.has_historical_fixing("Euribor3M", date));
        assert!(!settings.has_historical_fixing("Euribor6M", date));
    }

    #[test]
    fn index_names_are_case_insensitive() {
        let settings: Settings<i64> = Settings::new();
        let date = a_date();
        settings.add_fixing("Euribor6M", date, 0.02).unwrap();
        assert_eq!(settings.fixing("EURIBOR6M", date), Some(0.02));
        assert!(settings.has_historical_fixing("euribor6m", date));
    }

    #[test]
    fn conflicting_fixing_is_rejected_while_identical_is_idempotent() {
        let settings: Settings<i64> = Settings::new();
        let date = a_date();
        settings.add_fixing("Euribor6M", date, 0.02).unwrap();
        assert!(settings.add_fixing("Euribor6M", date, 0.02).is_ok());
        assert!(settings.add_fixing("Euribor6M", date, 0.03).is_err());
        assert_eq!(settings.fixing("Euribor6M", date), Some(0.02));
    }

    #[test]
    fn fixing_observers_persist_across_clear() {
        let settings: Settings<i64> = Settings::new();
        let date = a_date();
        let flag = shared_mut(Flag::default());
        settings.register_fixing_observer("Euribor6M", &(flag.clone() as SharedMut<dyn Observer>));

        settings.add_fixing("Euribor6M", date, 0.01).unwrap();
        assert!(flag.borrow().up);

        flag.borrow_mut().up = false;
        settings.clear_fixings();
        assert!(flag.borrow().up);

        flag.borrow_mut().up = false;
        settings.add_fixing("Euribor6M", date, 0.02).unwrap();
        assert!(flag.borrow().up);
    }

    /// Observer that reads a fixing back while being notified of its addition,
    /// the fixing-store analogue of [`Reader`]: it must not meet the mutable
    /// store borrow the notifying `add_fixing` held.
    struct FixingReader {
        settings: Shared<Settings<i64>>,
        date: Date,
        seen: Option<Rate>,
    }

    impl Observer for FixingReader {
        fn update(&mut self) {
            self.seen = self.settings.fixing("Euribor6M", self.date);
        }
    }

    #[test]
    fn observer_may_read_the_fixing_during_notification() {
        let settings = shared(Settings::<i64>::new());
        let date = a_date();
        let reader = shared_mut(FixingReader {
            settings: Shared::clone(&settings),
            date,
            seen: None,
        });
        settings
            .register_fixing_observer("Euribor6M", &(reader.clone() as SharedMut<dyn Observer>));

        settings.add_fixing("Euribor6M", date, 0.05).unwrap();
        assert_eq!(reader.borrow().seen, Some(0.05));
    }
}
