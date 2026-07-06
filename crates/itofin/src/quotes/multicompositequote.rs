//! Market element whose value depends on any number of other market elements.
//!
//! Port of `ql/quotes/multicompositequote.hpp`. The C++ class template is
//! generic over any function object taking an `Array`; here the function is a
//! `Fn(&Array) -> Real` type parameter, borrowing the argument array instead
//! of consuming it.

use std::cell::Cell;

use crate::ensure;
use crate::errors::QlResult;
use crate::handle::{AsObservable, Handle};
use crate::math::array::Array;
use crate::patterns::observable::{Observable, Observer};
use crate::require;
use crate::shared::{Shared, SharedMut};
use crate::types::{Real, Size};

use super::{Invalidator, Quote};

/// Market element whose value depends on any number of other market elements.
///
/// Mirrors QuantLib's `MultiCompositeQuote`: `f(sources)` is computed lazily
/// and cached; any notification from any source handle (a pointee change or a
/// relink) drops the cache and reaches this quote's observers.
pub struct MultiCompositeQuote<F> {
    elements: Vec<Handle<dyn Quote>>,
    cache: Shared<Cell<Option<Real>>>,
    observable: Shared<Observable>,
    f: F,
    _listener: SharedMut<Invalidator>,
}

impl<F: Fn(&Array) -> Real> MultiCompositeQuote<F> {
    /// Creates a quote combining `elements` through `f`, registering with
    /// every handle like the C++ constructor's `registerWith` loop.
    pub fn new(elements: Vec<Handle<dyn Quote>>, f: F) -> Self {
        let (cache, observable, listener) = Invalidator::new();
        let observer = listener.clone() as SharedMut<dyn Observer>;
        for element in &elements {
            element.register_observer(&observer);
        }
        MultiCompositeQuote {
            elements,
            cache,
            observable,
            f,
            _listener: listener,
        }
    }

    /// The current value of the `i`-th source quote.
    ///
    /// Mirrors the C++ `inputValue`, whose `at(i)` throws on an out-of-range
    /// index.
    pub fn input_value(&self, i: Size) -> QlResult<Real> {
        require!(
            i < self.elements.len(),
            "index {i} out of range for {} quotes",
            self.elements.len()
        );
        self.elements[i].current_link()?.value()
    }
}

impl<F> AsObservable for MultiCompositeQuote<F> {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl<F: Fn(&Array) -> Real> Quote for MultiCompositeQuote<F> {
    fn value(&self) -> QlResult<Real> {
        if let Some(cached) = self.cache.get() {
            return Ok(cached);
        }
        ensure!(self.is_valid(), "invalid MultiCompositeQuote");
        let args = self
            .elements
            .iter()
            .map(|element| element.current_link()?.value())
            .collect::<QlResult<Array>>()?;
        let value = (self.f)(&args);
        self.cache.set(Some(value));
        Ok(value)
    }

    fn is_valid(&self) -> bool {
        self.elements
            .iter()
            .all(|element| element.current_link().is_ok_and(|q| q.is_valid()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::test_support::{Flag, as_observer};

    /// Port of `testMultiComposite` (test-suite/quotes.cpp): the composite
    /// value tracks `f(sources)` for growing source sets and several array
    /// functions, and `input_value` mirrors each source.
    #[test]
    fn multi_composite_quote_tracks_function_of_sources() {
        let funcs: [fn(&Array) -> Real; 3] =
            [|a| a.iter().sum(), |a| a.iter().product(), |a| a.norm2()];

        for f in funcs {
            let mut mes: Vec<Shared<SimpleQuote>> = Vec::new();
            let mut handles: Vec<Handle<dyn Quote>> = Vec::new();
            for i in 0..3usize {
                mes.push(shared(SimpleQuote::new((i + 1) as Real)));
                handles.push(Handle::new(mes[i].clone()));
                let composite = MultiCompositeQuote::new(handles.clone(), f);
                for j in 0..=i {
                    mes[j].set_value((j * 10 + 1) as Real);
                    let args: Array = mes.iter().map(|me| me.value().unwrap()).collect();
                    let x = composite.value().unwrap();
                    let y = f(&args);
                    assert!(
                        (x - y).abs() <= 1.0e-10,
                        "composite quote yields {x}, function result is {y}"
                    );
                    for (k, me) in mes.iter().enumerate() {
                        assert_eq!(composite.input_value(k).unwrap(), me.value().unwrap());
                    }
                }
            }
        }
    }

    #[test]
    fn any_source_change_notifies_observers_and_recomputes() {
        let me1 = shared(SimpleQuote::new(1.0));
        let me2 = shared(SimpleQuote::new(2.0));
        let me3 = shared(SimpleQuote::new(3.0));
        let handles: Vec<Handle<dyn Quote>> =
            vec![Handle::new(me1), Handle::new(me2), Handle::new(me3.clone())];
        let composite = MultiCompositeQuote::new(handles, |a| a.iter().sum());
        assert_eq!(composite.value().unwrap(), 6.0);

        let flag = Flag::new();
        composite
            .observable()
            .register_observer(&as_observer(&flag));

        me3.set_value(30.0);

        assert!(Flag::is_up(&flag), "any source change must notify");
        assert_eq!(composite.value().unwrap(), 33.0);
    }

    #[test]
    fn invalid_source_and_bad_index_error() {
        let me1 = shared(SimpleQuote::new(1.0));
        let handles: Vec<Handle<dyn Quote>> = vec![Handle::new(me1), Handle::empty()];
        let composite = MultiCompositeQuote::new(handles, |a| a.iter().sum());
        assert!(!composite.is_valid());
        assert_eq!(
            composite.value().unwrap_err().message(),
            "invalid MultiCompositeQuote"
        );
        assert!(composite.input_value(2).is_err());
    }
}
