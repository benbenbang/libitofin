//! Dated events.
//!
//! Port of `ql/event.hpp`: the base class for anything that happens on a date
//! and can say whether it has already happened.
//!
//! [`has_occurred`](Event::has_occurred) diverges from the C++ base class in
//! taking the [`Settings`] explicitly (D5) and returning [`QlResult`]: an unset
//! evaluation date is an error here rather than a silent fall back to the
//! system clock (D10). It is also a *required* method rather than a virtual one
//! with a base implementation: C++ lets `CashFlow` override the base rule, and
//! Rust has no specialization, so an inherited default would silently give a
//! cash flow the plain-event rule. Implementors forward to
//! [`event_has_occurred`] instead. The `accept(AcyclicVisitor&)` override and
//! the `detail::simple_event` placeholder have no counterpart in the port.

use crate::errors::QlResult;
use crate::fail;
use crate::patterns::observable::AsObservable;
use crate::settings::Settings;
use crate::time::date::Date;

/// Something that happens on a date.
///
/// Mirrors QuantLib's `Event`, whose only state is the date it occurs on.
/// Events are observable so that instruments holding them are notified when an
/// event's terms change.
pub trait Event: AsObservable {
    /// The date at which the event occurs.
    fn date(&self) -> Date;

    /// Whether the event has already occurred as of `ref_date`.
    ///
    /// `ref_date` defaults to the evaluation date, and `include_ref_date` to
    /// [`Settings::include_reference_date_events`]; when that flag is set, an
    /// event falling exactly on the reference date has *not* yet occurred.
    ///
    /// Plain events forward to [`event_has_occurred`]; cash flows follow a
    /// different rule on the evaluation date, so this is required rather than
    /// provided.
    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
        include_ref_date: Option<bool>,
    ) -> QlResult<bool>;
}

/// The `Event::hasOccurred` rule of `event.cpp`.
///
/// `include_ref_date` falls back to
/// [`Settings::include_reference_date_events`]; when it holds, an event on the
/// reference date has not yet occurred.
pub fn event_has_occurred(
    date: Date,
    settings: &Settings<Date>,
    ref_date: Option<Date>,
    include_ref_date: Option<bool>,
) -> QlResult<bool> {
    let reference = reference_date(settings, ref_date)?;
    let include = include_ref_date.unwrap_or_else(|| settings.include_reference_date_events());
    Ok(if include {
        date < reference
    } else {
        date <= reference
    })
}

/// Resolves the reference date an occurrence is measured against.
///
/// QuantLib substitutes the evaluation date for a null `refDate`, and the
/// system clock for an unset evaluation date; the port stops at the missing
/// evaluation date instead (D10).
pub(crate) fn reference_date(settings: &Settings<Date>, ref_date: Option<Date>) -> QlResult<Date> {
    match ref_date.or_else(|| settings.evaluation_date()) {
        Some(reference) => Ok(reference),
        None => fail!("no evaluation date set: an event needs a reference date"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patterns::observable::Observable;
    use crate::time::date::Month;

    struct SimpleEvent {
        date: Date,
        observable: Observable,
    }

    impl SimpleEvent {
        fn new(date: Date) -> Self {
            SimpleEvent {
                date,
                observable: Observable::new(),
            }
        }
    }

    impl AsObservable for SimpleEvent {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl Event for SimpleEvent {
        fn date(&self) -> Date {
            self.date
        }

        fn has_occurred(
            &self,
            settings: &Settings<Date>,
            ref_date: Option<Date>,
            include_ref_date: Option<bool>,
        ) -> QlResult<bool> {
            event_has_occurred(self.date, settings, ref_date, include_ref_date)
        }
    }

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    #[test]
    fn an_event_occurs_once_the_reference_date_passes_it() {
        let settings = Settings::new();
        let event = SimpleEvent::new(today());

        assert!(
            !event
                .has_occurred(&settings, Some(today() - 1), None)
                .unwrap()
        );
        assert!(
            event
                .has_occurred(&settings, Some(today() + 1), None)
                .unwrap()
        );
    }

    #[test]
    fn an_event_on_the_reference_date_occurs_unless_reference_date_events_are_included() {
        let settings = Settings::new();
        let event = SimpleEvent::new(today());

        settings.set_include_reference_date_events(false);
        assert!(event.has_occurred(&settings, Some(today()), None).unwrap());

        settings.set_include_reference_date_events(true);
        assert!(!event.has_occurred(&settings, Some(today()), None).unwrap());
    }

    #[test]
    fn an_explicit_include_ref_date_overrides_the_settings_flag() {
        let settings = Settings::new();
        let event = SimpleEvent::new(today());

        settings.set_include_reference_date_events(false);
        assert!(
            !event
                .has_occurred(&settings, Some(today()), Some(true))
                .unwrap()
        );

        settings.set_include_reference_date_events(true);
        assert!(
            event
                .has_occurred(&settings, Some(today()), Some(false))
                .unwrap()
        );
    }

    #[test]
    fn the_evaluation_date_stands_in_for_a_missing_reference_date() {
        let settings = Settings::new();
        settings.set_evaluation_date(today() + 1);
        let event = SimpleEvent::new(today());

        assert!(event.has_occurred(&settings, None, None).unwrap());
    }

    #[test]
    fn an_unset_evaluation_date_is_an_error() {
        let settings = Settings::new();
        let event = SimpleEvent::new(today());

        assert!(event.has_occurred(&settings, None, None).is_err());
        assert!(event.has_occurred(&settings, Some(today()), None).is_ok());
    }
}
