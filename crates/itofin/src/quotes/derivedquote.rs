//! Market quote whose value depends on another quote.
//!
//! Port of `ql/quotes/derivedquote.hpp`. The C++ class template is generic
//! over any unary function object; here the function is a `Fn(Real) -> Real`
//! type parameter. `makeDerivedQuote` exists only for C++ template-argument
//! deduction and is not ported.

use std::cell::Cell;

use crate::ensure;
use crate::errors::QlResult;
use crate::handle::{AsObservable, Handle};
use crate::patterns::observable::{Observable, Observer};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::types::Real;

use super::{Invalidator, Quote};

/// Market quote whose value depends on another quote.
///
/// Mirrors QuantLib's `DerivedQuote`: `f(source)` is computed lazily and
/// cached; any notification from the source handle (a pointee change or a
/// relink) drops the cache and reaches this quote's observers.
pub struct DerivedQuote<F> {
    element: Handle<dyn Quote>,
    cache: Shared<Cell<Option<Real>>>,
    observable: Shared<Observable>,
    f: F,
    _listener: SharedMut<Invalidator>,
}

impl<F: Fn(Real) -> Real> DerivedQuote<F> {
    /// Creates a quote deriving its value from `element` through `f`,
    /// registering with the handle like the C++ constructor's `registerWith`.
    pub fn new(element: Handle<dyn Quote>, f: F) -> Self {
        let cache = shared(Cell::new(None));
        let observable = shared(Observable::new());
        let listener = shared_mut(Invalidator {
            cache: Shared::clone(&cache),
            observable: Shared::clone(&observable),
        });
        element.register_observer(&(listener.clone() as SharedMut<dyn Observer>));
        DerivedQuote {
            element,
            cache,
            observable,
            f,
            _listener: listener,
        }
    }
}

impl<F> AsObservable for DerivedQuote<F> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<F: Fn(Real) -> Real> Quote for DerivedQuote<F> {
    fn value(&self) -> QlResult<Real> {
        if let Some(cached) = self.cache.get() {
            return Ok(cached);
        }
        ensure!(self.is_valid(), "invalid DerivedQuote");
        let value = (self.f)(self.element.current_link()?.value()?);
        self.cache.set(Some(value));
        Ok(value)
    }

    fn is_valid(&self) -> bool {
        self.element.current_link().is_ok_and(|q| q.is_valid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;

    #[derive(Default)]
    struct Flag {
        up: bool,
    }

    impl Observer for Flag {
        fn update(&mut self) {
            self.up = true;
        }
    }

    /// Port of `testDerived` (test-suite/quotes.cpp): the derived value tracks
    /// `f(source)` across source changes for several functions.
    #[test]
    fn derived_quote_tracks_function_of_source() {
        let funcs: [fn(Real) -> Real; 3] = [|x| x + 10.0, |x| x * 10.0, |x| x - 10.0];
        let values = [12.0, 23.0, 34.0];

        let me = shared(SimpleQuote::default());
        let h: Handle<dyn Quote> = Handle::new(me.clone());

        for f in funcs {
            let derived = DerivedQuote::new(h.clone(), f);
            for value in values {
                me.set_value(value);
                let x = derived.value().unwrap();
                let y = f(value);
                assert!(
                    (x - y).abs() <= 1.0e-10,
                    "derived quote yields {x}, function result is {y}"
                );
            }
        }
    }

    #[test]
    fn source_change_notifies_observers_and_recomputes() {
        let me = shared(SimpleQuote::new(1.0));
        let derived = DerivedQuote::new(Handle::new(me.clone()), |x| 2.0 * x);
        assert_eq!(derived.value().unwrap(), 2.0);

        let flag = shared_mut(Flag::default());
        derived
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        me.set_value(3.0);

        assert!(flag.borrow().up, "source change must reach quote observers");
        assert_eq!(derived.value().unwrap(), 6.0);
    }

    #[test]
    fn relink_notifies_observers_and_recomputes() {
        let rh = crate::quotes::make_quote_handle(1.0);
        let derived = DerivedQuote::new(rh.handle(), |x| x + 1.0);
        assert_eq!(derived.value().unwrap(), 2.0);

        let flag = shared_mut(Flag::default());
        derived
            .observable()
            .register_observer(&(flag.clone() as SharedMut<dyn Observer>));

        rh.link_to(shared(SimpleQuote::new(5.0)));

        assert!(flag.borrow().up, "relink must reach quote observers");
        assert_eq!(derived.value().unwrap(), 6.0);
    }

    #[test]
    fn empty_or_invalid_source_is_invalid() {
        let empty = DerivedQuote::new(Handle::empty(), |x| x);
        assert!(!empty.is_valid());
        assert_eq!(empty.value().unwrap_err().message(), "invalid DerivedQuote");

        let me = shared(SimpleQuote::default());
        let derived = DerivedQuote::new(Handle::new(me), |x| x);
        assert!(!derived.is_valid());
        assert!(derived.value().is_err());
    }
}
