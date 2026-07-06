//! Market quotes.
//!
//! Port of `ql/quote.hpp`: the [`Quote`] trait is the purely virtual base
//! class for market observables. QuantLib's `Quote` inherits `Observable`;
//! here the trait exposes the embedded [`Observable`] through
//! [`observable`](Quote::observable), following the crate's embedding
//! convention (see `patterns::lazyobject`).
//!
//! `handleFromVariant` (a `std::variant<Real, Handle<Quote>>` convenience for
//! C++ term-structure constructors) has no caller in the core yet and is not
//! ported.

mod simplequote;

pub use simplequote::{SimpleQuote, make_quote_handle};

use crate::errors::QlResult;
use crate::patterns::observable::Observable;
use crate::types::Real;

/// Purely virtual base class for market observables.
///
/// Mirrors QuantLib's `Quote`. Implementors embed an [`Observable`] and
/// notify it when their value changes; observers of a quote register through
/// [`observable`](Quote::observable).
pub trait Quote {
    /// Access to the embedded observable for registering observers.
    fn observable(&self) -> &Observable;

    /// Returns the current value, or an error if the quote holds none.
    fn value(&self) -> QlResult<Real>;

    /// Whether the quote holds a valid value.
    fn is_valid(&self) -> bool;
}
