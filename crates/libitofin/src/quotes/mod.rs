//! Market quotes.
//!
//! Port of `ql/quote.hpp`: the [`Quote`] trait is the purely virtual base
//! class for market observables. QuantLib's `Quote` inherits `Observable`;
//! here the trait exposes the embedded observable through its
//! [`AsObservable`] supertrait, which is also what lets a
//! [`Handle`](crate::handle::Handle) forward pointee changes to its
//! observers.
//!
//! `handleFromVariant` (a `std::variant<Real, Handle<Quote>>` convenience for
//! C++ term-structure constructors) has no caller in the core yet and is not
//! ported.
//!
//! The remaining `ql/quotes/` classes depend on layers not yet ported and
//! follow with them: `ForwardValueQuote` and `LastFixingQuote` (Index),
//! `ForwardSwapQuote` (SwapIndex/VanillaSwap), `FuturesConvAdjustmentQuote`
//! (IborIndex), `ImpliedStdDevQuote` and
//! `EurodollarFuturesImpliedStdDevQuote` (`blackFormulaImpliedStdDev`).

mod compositequote;
mod deltavolquote;
mod derivedquote;
mod multicompositequote;
mod simplequote;

pub use compositequote::CompositeQuote;
pub use deltavolquote::{AtmType, DeltaType, DeltaVolQuote};
pub use derivedquote::DerivedQuote;
pub use multicompositequote::MultiCompositeQuote;
pub use simplequote::{SimpleQuote, make_quote_handle};

use std::cell::Cell;

use crate::errors::QlResult;
use crate::patterns::observable::{AsObservable, Observable, ResetThenNotify};
use crate::shared::{Shared, SharedMut, shared};
use crate::types::Real;

/// Purely virtual base class for market observables.
///
/// Mirrors QuantLib's `Quote`. Implementors embed an
/// [`Observable`](crate::patterns::observable::Observable) and notify it when
/// their value changes; observers of a quote register through
/// [`observable`](AsObservable::observable).
pub trait Quote: AsObservable {
    /// Returns the current value, or an error if the quote holds none.
    fn value(&self) -> QlResult<Real>;

    /// Whether the quote holds a valid value.
    fn is_valid(&self) -> bool;
}

/// Builds the empty cache, the quote's observable and the invalidating
/// listener wired to both - the shared construction step of every derived
/// quote (the C++ `update()` of `DerivedQuote` and friends drops the cached
/// value and passes the notification on).
///
/// Derived quotes register the listener with their source handle(s) and hold
/// the strong reference; the observer registry keeps only a weak one.
fn invalidator() -> (
    Shared<Cell<Option<Real>>>,
    Shared<Observable>,
    SharedMut<ResetThenNotify>,
) {
    let cache = shared(Cell::new(None));
    let observable = shared(Observable::new());
    let listener = ResetThenNotify::broadcasting(Shared::clone(&observable), {
        let cache = Shared::clone(&cache);
        move || cache.set(None)
    });
    (cache, observable, listener)
}
