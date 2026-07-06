//! Market element whose value depends on two other market elements.
//!
//! Port of `ql/quotes/compositequote.hpp`. The C++ class template is generic
//! over any binary function object; here the function is a
//! `Fn(Real, Real) -> Real` type parameter. `makeCompositeQuote` exists only
//! for C++ template-argument deduction and is not ported.

use std::cell::Cell;

use crate::ensure;
use crate::errors::QlResult;
use crate::handle::{AsObservable, Handle};
use crate::patterns::observable::{Observable, Observer};
use crate::shared::{Shared, SharedMut};
use crate::types::Real;

use super::{Invalidator, Quote};

/// Market element whose value depends on two other market elements.
///
/// Mirrors QuantLib's `CompositeQuote`: `f(source1, source2)` is computed
/// lazily and cached; any notification from either source handle (a pointee
/// change or a relink) drops the cache and reaches this quote's observers.
pub struct CompositeQuote<F> {
    element1: Handle<dyn Quote>,
    element2: Handle<dyn Quote>,
    cache: Shared<Cell<Option<Real>>>,
    observable: Shared<Observable>,
    f: F,
    _listener: SharedMut<Invalidator>,
}

impl<F: Fn(Real, Real) -> Real> CompositeQuote<F> {
    /// Creates a quote combining `element1` and `element2` through `f`,
    /// registering with both handles like the C++ constructor's
    /// `registerWith` calls.
    pub fn new(element1: Handle<dyn Quote>, element2: Handle<dyn Quote>, f: F) -> Self {
        let (cache, observable, listener) = Invalidator::new();
        let observer = listener.clone() as SharedMut<dyn Observer>;
        element1.register_observer(&observer);
        element2.register_observer(&observer);
        CompositeQuote {
            element1,
            element2,
            cache,
            observable,
            f,
            _listener: listener,
        }
    }

    /// The current value of the first source quote.
    pub fn value1(&self) -> QlResult<Real> {
        self.element1.current_link()?.value()
    }

    /// The current value of the second source quote.
    pub fn value2(&self) -> QlResult<Real> {
        self.element2.current_link()?.value()
    }
}

impl<F> AsObservable for CompositeQuote<F> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<F: Fn(Real, Real) -> Real> Quote for CompositeQuote<F> {
    fn value(&self) -> QlResult<Real> {
        if let Some(cached) = self.cache.get() {
            return Ok(cached);
        }
        ensure!(self.is_valid(), "invalid CompositeQuote");
        let value = (self.f)(self.value1()?, self.value2()?);
        self.cache.set(Some(value));
        Ok(value)
    }

    fn is_valid(&self) -> bool {
        self.element1.current_link().is_ok_and(|q| q.is_valid())
            && self.element2.current_link().is_ok_and(|q| q.is_valid())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};

    /// Port of `testComposite` (test-suite/quotes.cpp): the composite value
    /// tracks `f(source1, source2)` across source changes for several
    /// functions.
    #[test]
    fn composite_quote_tracks_function_of_sources() {
        let funcs: [fn(Real, Real) -> Real; 3] = [|x, y| x + y, |x, y| x * y, |x, y| x - y];
        let values = [12.0, 23.0, 34.0];

        let me1 = shared(SimpleQuote::default());
        let me2 = shared(SimpleQuote::default());
        let h1: Handle<dyn Quote> = Handle::new(me1.clone());
        let h2: Handle<dyn Quote> = Handle::new(me2.clone());

        for f in funcs {
            let composite = CompositeQuote::new(h1.clone(), h2.clone(), f);
            for value in values {
                me1.set_value(value);
                me2.set_value(value + 1.0);
                let x = composite.value().unwrap();
                let y = f(value, value + 1.0);
                assert!(
                    (x - y).abs() <= 1.0e-10,
                    "composite quote yields {x}, function result is {y}"
                );
            }
        }
    }

    #[test]
    fn either_source_change_notifies_observers_and_recomputes() {
        let me1 = shared(SimpleQuote::new(1.0));
        let me2 = shared(SimpleQuote::new(2.0));
        let composite = CompositeQuote::new(
            Handle::new(me1.clone()),
            Handle::new(me2.clone()),
            |x, y| x + y,
        );
        assert_eq!(composite.value().unwrap(), 3.0);
        assert_eq!(composite.value1().unwrap(), 1.0);
        assert_eq!(composite.value2().unwrap(), 2.0);

        let flag = Flag::new();
        composite
            .observable()
            .register_observer(&as_observer(&flag));

        me1.set_value(10.0);
        assert!(Flag::is_up(&flag), "first source change must notify");
        assert_eq!(composite.value().unwrap(), 12.0);

        Flag::lower(&flag);
        me2.set_value(20.0);
        assert!(Flag::is_up(&flag), "second source change must notify");
        assert_eq!(composite.value().unwrap(), 30.0);
    }

    #[test]
    fn composite_is_invalid_unless_both_sources_are_valid() {
        let me1 = shared(SimpleQuote::new(1.0));
        let composite = CompositeQuote::new(Handle::new(me1), Handle::empty(), |x, y| x + y);
        assert!(!composite.is_valid());
        assert_eq!(
            composite.value().unwrap_err().message(),
            "invalid CompositeQuote"
        );
    }
}
