//! Market element returning a stored value.
//!
//! Port of `ql/quotes/simplequote.hpp`. QuantLib marks an unset quote with the
//! `Null<Real>` sentinel; the port stores `Option<Real>` instead, so
//! `SimpleQuote::new(None)` is the C++ default-constructed (invalid) quote.
//!
//! Mutation goes through `&self` (the value lives in a [`Cell`]): quotes are
//! shared as [`Shared<SimpleQuote>`](crate::shared::Shared) so that observers
//! may read the quote while being notified of its change, exactly like
//! [`Handle`] releases its borrows before notifying.

use std::cell::Cell;

use crate::ensure;
use crate::errors::QlResult;
use crate::handle::RelinkableHandle;
use crate::patterns::observable::Observable;
use crate::shared::shared;
use crate::types::Real;

use super::Quote;

/// Market element returning a stored value.
///
/// Mirrors QuantLib's `SimpleQuote`: a settable value that notifies its
/// observers on every actual change.
pub struct SimpleQuote {
    value: Cell<Option<Real>>,
    observable: Observable,
}

impl SimpleQuote {
    /// Creates a quote holding `value`; pass `None` for an invalid quote
    /// (the C++ `SimpleQuote()` default).
    pub fn new(value: impl Into<Option<Real>>) -> Self {
        SimpleQuote {
            value: Cell::new(value.into()),
            observable: Observable::new(),
        }
    }

    /// Sets a new value, notifying observers when it actually changes.
    ///
    /// Returns the difference between the new and the old value when both are
    /// valid, `None` otherwise (QuantLib computes the same difference against
    /// the `Null<Real>` sentinel, which is meaningless as a number).
    pub fn set_value(&self, value: impl Into<Option<Real>>) -> Option<Real> {
        let new = value.into();
        let old = self.value.get();
        if new != old {
            self.value.set(new);
            self.observable.notify_observers();
        }
        match (new, old) {
            (Some(n), Some(o)) => Some(n - o),
            _ => None,
        }
    }

    /// Invalidates the quote, notifying observers if it held a value.
    pub fn reset(&self) {
        self.set_value(None);
    }
}

impl Default for SimpleQuote {
    fn default() -> Self {
        SimpleQuote::new(None)
    }
}

impl Quote for SimpleQuote {
    fn observable(&self) -> &Observable {
        &self.observable
    }

    fn value(&self) -> QlResult<Real> {
        ensure!(self.is_valid(), "invalid SimpleQuote");
        Ok(self.value.get().expect("validity was just checked"))
    }

    fn is_valid(&self) -> bool {
        self.value.get().is_some()
    }
}

/// Builds a relinkable handle on a fresh [`SimpleQuote`].
///
/// Mirrors QuantLib's `makeQuoteHandle`.
pub fn make_quote_handle(value: Real) -> RelinkableHandle<dyn Quote> {
    RelinkableHandle::new(shared(SimpleQuote::new(value)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Handle;
    use crate::patterns::observable::Observer;
    use crate::shared::{Shared, SharedMut, shared_mut};

    /// Port of the test-suite `Flag` utility: records whether it was raised.
    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Flag {
        fn new() -> SharedMut<Flag> {
            shared_mut(Flag::default())
        }

        fn lower(flag: &SharedMut<Flag>) {
            flag.borrow_mut().up = false;
        }

        fn is_up(flag: &SharedMut<Flag>) -> bool {
            flag.borrow().up
        }
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    fn as_observer(flag: &SharedMut<Flag>) -> SharedMut<dyn Observer> {
        flag.clone()
    }

    #[test]
    #[allow(clippy::approx_constant)]
    fn observable_quote_notifies_on_set_value() {
        let me = shared(SimpleQuote::new(0.0));
        let f = Flag::new();
        me.observable().register_observer(&as_observer(&f));

        me.set_value(3.14);

        assert!(Flag::is_up(&f), "observer was not notified of quote change");
        assert_eq!(me.value().unwrap(), 3.14);
    }

    #[test]
    fn observable_handle_notifies_on_relink() {
        let me1: Shared<SimpleQuote> = shared(SimpleQuote::new(0.0));
        let h: RelinkableHandle<dyn Quote> = RelinkableHandle::new(me1);
        let f = Flag::new();
        h.handle().register_observer(&as_observer(&f));

        let me2: Shared<dyn Quote> = shared(SimpleQuote::new(0.0));
        h.link_to(me2);

        assert!(Flag::is_up(&f), "observer was not notified of relink");
    }

    #[test]
    fn set_value_returns_difference_and_skips_no_op_notifications() {
        let me = SimpleQuote::new(1.0);
        let f = Flag::new();
        me.observable().register_observer(&as_observer(&f));

        assert_eq!(me.set_value(3.0), Some(2.0));
        assert!(Flag::is_up(&f));

        Flag::lower(&f);
        assert_eq!(me.set_value(3.0), Some(0.0));
        assert!(!Flag::is_up(&f), "unchanged value must not notify");
    }

    #[test]
    fn invalid_quote_reports_and_errors() {
        let me = SimpleQuote::default();
        assert!(!me.is_valid());
        let err = me.value().unwrap_err();
        assert_eq!(err.message(), "invalid SimpleQuote");

        me.set_value(1.0);
        assert!(me.is_valid());
        assert_eq!(me.value().unwrap(), 1.0);
    }

    #[test]
    fn reset_invalidates_and_notifies() {
        let me = SimpleQuote::new(1.0);
        let f = Flag::new();
        me.observable().register_observer(&as_observer(&f));

        me.reset();

        assert!(!me.is_valid());
        assert!(Flag::is_up(&f));

        Flag::lower(&f);
        me.reset();
        assert!(
            !Flag::is_up(&f),
            "resetting an invalid quote must not notify"
        );
    }

    #[test]
    fn valid_to_invalid_transitions_report_no_difference() {
        let me = SimpleQuote::default();
        assert_eq!(me.set_value(2.0), None);
        assert_eq!(me.set_value(None), None);
    }

    #[test]
    fn quote_is_usable_through_a_handle() {
        let me = shared(SimpleQuote::new(0.25));
        let h: Handle<dyn Quote> = Handle::new(me.clone());

        let linked = h.current_link().unwrap();
        assert!(linked.is_valid());
        assert_eq!(linked.value().unwrap(), 0.25);

        me.set_value(0.5);
        assert_eq!(h.current_link().unwrap().value().unwrap(), 0.5);
    }

    #[test]
    fn make_quote_handle_builds_a_linked_relinkable_handle() {
        let h = make_quote_handle(0.03);
        assert!(!h.handle().is_empty());
        assert_eq!(h.handle().current_link().unwrap().value().unwrap(), 0.03);
    }

    /// Observer that reads the quote while being notified - must not conflict
    /// with the borrow taken for the mutation that triggered the update.
    struct Reader {
        quote: Shared<SimpleQuote>,
        seen: Option<Real>,
    }

    impl Observer for Reader {
        fn update(&mut self) {
            self.seen = self.quote.value().ok();
        }
    }

    #[test]
    fn observer_may_read_quote_during_notification() {
        let me = shared(SimpleQuote::new(1.0));
        let reader = shared_mut(Reader {
            quote: me.clone(),
            seen: None,
        });
        me.observable()
            .register_observer(&(reader.clone() as SharedMut<dyn Observer>));

        me.set_value(2.0);

        assert_eq!(reader.borrow().seen, Some(2.0));
    }
}
