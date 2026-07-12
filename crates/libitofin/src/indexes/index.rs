//! The abstract index base.
//!
//! Port of `ql/index.hpp`. QuantLib's `Index` is a purely virtual
//! `Observable`/`Observer` that names a fixing series and reaches its history
//! through the `IndexManager` global singleton. Per D5/D11 the port keeps the
//! history on [`Settings`] instead (the `fixing_store`), so the concrete
//! methods that C++ answers from `IndexManager::instance()` are answered here
//! from a [`settings`](Index::settings) handle every index carries, keyed by
//! its [`name`](Index::name).
//!
//! The four genuinely abstract members - [`name`](Index::name),
//! [`fixing_calendar`](Index::fixing_calendar),
//! [`is_valid_fixing_date`](Index::is_valid_fixing_date) and
//! [`fixing`](Index::fixing) - are required; the history helpers
//! ([`past_fixing`](Index::past_fixing), [`add_fixing`](Index::add_fixing),
//! [`clear_fixings`](Index::clear_fixings), ...) are provided, reaching the
//! store through [`settings`](Index::settings) and [`name`](Index::name) as
//! `Index`'s inline C++ definitions reach `IndexManager` through `name()`.
//!
//! Observation ([`observable`](Index::observable)) is the [`Observable`] every
//! index broadcasts through: an [`InterestRateIndex`](super::InterestRateIndex)
//! forwards the evaluation-date and fixing-history notifications it registers
//! for onto this observable, the port of `Index::update -> notifyObservers`.

use crate::errors::QlResult;
use crate::patterns::observable::Observable;
use crate::require;
use crate::settings::Settings;
use crate::time::date::Date;
use crate::types::Rate;

/// The abstract base of every index (`ql/index.hpp`'s `Index`).
///
/// The abstract quartet - [`name`](Index::name),
/// [`fixing_calendar`](Index::fixing_calendar),
/// [`is_valid_fixing_date`](Index::is_valid_fixing_date) and
/// [`fixing`](Index::fixing) - is required, as are the two accessors D5 needs
/// to reach shared state: [`settings`](Index::settings), holding the fixing
/// history (D11), and [`observable`](Index::observable), the notifier the index
/// broadcasts through. Everything else is provided against those, so a concrete
/// index answers only what genuinely varies.
///
/// An [`InterestRateIndex`](super::InterestRateIndex) never implements this
/// trait by hand: the blanket `impl<T: InterestRateIndex> Index for T` answers
/// the whole quartet - including the delicate [`fixing`](Index::fixing)
/// decision tree - from the index's
/// [`InterestRateIndexBase`](super::InterestRateIndexBase), so the algorithm
/// where the store and the forecast curve meet cannot be re-derived, or got
/// wrong, downstream.
pub trait Index {
    /// The name identifying the index and its fixing history.
    ///
    /// Used for output and for keying the fixing store; not for
    /// switch-on-type code (`Index::name`'s warning).
    fn name(&self) -> String;

    /// The calendar defining valid fixing dates.
    fn fixing_calendar(&self) -> crate::time::calendar::Calendar;

    /// Whether `fixing_date` is a valid fixing date for this index.
    fn is_valid_fixing_date(&self, fixing_date: Date) -> bool;

    /// The fixing at `fixing_date`.
    ///
    /// The date is the actual calendar date of the fixing; no settlement days
    /// are applied. When `forecast_todays_fixing` and the date is today, the
    /// forecast is taken rather than any stored fixing.
    fn fixing(&self, fixing_date: Date, forecast_todays_fixing: bool) -> QlResult<Rate>;

    /// The settings holding this index's fixing history (D5/D11).
    fn settings(&self) -> &Settings<Date>;

    /// The observable this index broadcasts its changes through.
    fn observable(&self) -> &Observable;

    /// Whether the index accepts stored fixings.
    ///
    /// When `false`, [`add_fixing`](Index::add_fixing) and its kin raise an
    /// error (`Index::allowsNativeFixings`, whose default is `true`).
    fn allows_native_fixings(&self) -> bool {
        true
    }

    /// Whether a historical fixing is on record for `fixing_date`
    /// (`Index::hasHistoricalFixing`).
    fn has_historical_fixing(&self, fixing_date: Date) -> bool {
        self.settings()
            .has_historical_fixing(&self.name(), fixing_date)
    }

    /// The stored past fixing at `fixing_date`, if one was recorded.
    ///
    /// `Index::pastFixing` requires a valid fixing date, then reads the fixing
    /// series; the C++ `Null<Real>` for an absent fixing becomes `None` (D4).
    fn past_fixing(&self, fixing_date: Date) -> QlResult<Option<Rate>> {
        require!(
            self.is_valid_fixing_date(fixing_date),
            "{fixing_date:?} is not a valid fixing date"
        );
        Ok(self.settings().fixing(&self.name(), fixing_date))
    }

    /// Records a single past fixing, notifying the index's observers.
    ///
    /// `Index::addFixing`: rejected unless the index allows native fixings and
    /// the date is a valid fixing date. The C++ `forceOverwrite` flag is
    /// omitted: the D11 store rejects a conflicting value and accepts an
    /// identical one, i.e. `forceOverwrite = false`, and has no overwrite mode
    /// to switch on.
    fn add_fixing(&self, fixing_date: Date, value: Rate) -> QlResult<()> {
        self.add_fixings([(fixing_date, value)])
    }

    /// Records several past fixings, notifying the index's observers once.
    ///
    /// The template `Index::addFixings`: it checks that native fixings are
    /// allowed and every date is valid (the validity predicate C++ hands to
    /// `IndexManager::addFixings`) before delegating to the store.
    fn add_fixings(&self, fixings: impl IntoIterator<Item = (Date, Rate)>) -> QlResult<()> {
        require!(
            self.allows_native_fixings(),
            "native fixings not allowed for index {}",
            self.name()
        );
        let fixings: Vec<(Date, Rate)> = fixings.into_iter().collect();
        for &(date, _) in &fixings {
            require!(date != Date::null(), "cannot add fixing on a null date");
            require!(
                self.is_valid_fixing_date(date),
                "{date:?} is not a valid fixing date"
            );
        }
        self.settings().add_fixings(&self.name(), fixings)
    }

    /// Clears every stored fixing of this index, notifying its observers
    /// (`Index::clearFixings`).
    fn clear_fixings(&self) {
        self.settings().clear_fixing(&self.name());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::time::calendar::Calendar;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::{Date, Month};

    fn a_date() -> Date {
        Date::new(15, Month::June, 2026)
    }

    /// A minimal `Index` implemented directly (not through
    /// `InterestRateIndex`), exercising the provided history and observability
    /// helpers against the D11 store without the interest-rate machinery.
    struct TestIndex {
        name: String,
        calendar: Calendar,
        settings: Shared<Settings<Date>>,
        observable: Observable,
    }

    impl Index for TestIndex {
        fn name(&self) -> String {
            self.name.clone()
        }
        fn fixing_calendar(&self) -> Calendar {
            self.calendar.clone()
        }
        fn is_valid_fixing_date(&self, fixing_date: Date) -> bool {
            self.calendar.is_business_day(fixing_date)
        }
        fn fixing(&self, fixing_date: Date, _forecast_todays_fixing: bool) -> QlResult<Rate> {
            match self.past_fixing(fixing_date)? {
                Some(rate) => Ok(rate),
                None => crate::fail!("no fixing for {fixing_date:?}"),
            }
        }
        fn settings(&self) -> &Settings<Date> {
            &self.settings
        }
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    fn an_index() -> TestIndex {
        TestIndex {
            name: "TestIndex".into(),
            calendar: WeekendsOnly::new(),
            settings: shared(Settings::<Date>::new()),
            observable: Observable::new(),
        }
    }

    #[test]
    fn add_and_read_a_past_fixing() {
        let index = an_index();
        assert!(!index.has_historical_fixing(a_date()));
        index.add_fixing(a_date(), 0.02).unwrap();
        assert!(index.has_historical_fixing(a_date()));
        assert_eq!(index.past_fixing(a_date()).unwrap(), Some(0.02));
    }

    #[test]
    fn add_fixing_rejects_an_invalid_date() {
        let index = an_index();
        // 13 June 2026 is a Saturday: not a business day in WeekendsOnly.
        let saturday = Date::new(13, Month::June, 2026);
        assert!(!index.is_valid_fixing_date(saturday));
        assert!(index.add_fixing(saturday, 0.02).is_err());
    }

    #[test]
    fn clear_fixings_removes_the_history() {
        let index = an_index();
        index.add_fixing(a_date(), 0.02).unwrap();
        index.clear_fixings();
        assert!(!index.has_historical_fixing(a_date()));
    }

    struct Flag {
        up: bool,
    }

    impl crate::patterns::observable::Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    #[test]
    fn observers_of_the_index_hear_its_own_notifications() {
        let index = an_index();
        let flag = shared_mut(Flag { up: false });
        index.observable().register_observer(
            &(flag.clone() as SharedMut<dyn crate::patterns::observable::Observer>),
        );
        index.observable().notify_observers();
        assert!(flag.borrow().up);
    }
}
