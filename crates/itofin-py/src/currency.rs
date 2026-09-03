//! Facade for the currency specification Currency.
//!
//! An index carries a currency, so the general IborIndex constructor cannot be
//! called without one. Only the four named currencies the core provides are
//! exposed as staticmethods; the general Currency.new() form is deliberately
//! omitted, since the core itself ports only the currencies the indexes need
//! and the full `ql/currencies/*` catalogue is deferred there.
//!
//! It is registered under `itofin.indexes` rather than a submodule of its own:
//! the index constructors are its only consumers today, both the general
//! IborIndex one and, since #868, SwapIndex, which used to hard-code EUR.

use libitofin::currency::Currency;
use pyo3::prelude::*;

/// An ISO 4217 currency specification.
///
/// Only the four named currencies the core provides are exposed; the general
/// constructor is omitted, as the core ports only the currencies its indexes
/// need and the full catalogue is deferred there.
#[pyclass(name = "Currency", unsendable)]
pub struct PyCurrency {
    inner: Currency,
}

#[pymethods]
impl PyCurrency {
    /// Return the European Euro.
    ///
    /// Returns:
    ///     Currency: The euro, ISO code "EUR".
    #[staticmethod]
    fn eur() -> Self {
        PyCurrency {
            inner: Currency::eur(),
        }
    }

    /// Return the U.S. dollar.
    ///
    /// Returns:
    ///     Currency: The U.S. dollar, ISO code "USD".
    #[staticmethod]
    fn usd() -> Self {
        PyCurrency {
            inner: Currency::usd(),
        }
    }

    /// Return the British pound sterling.
    ///
    /// Returns:
    ///     Currency: The pound sterling, ISO code "GBP".
    #[staticmethod]
    fn gbp() -> Self {
        PyCurrency {
            inner: Currency::gbp(),
        }
    }

    /// Return the Japanese yen.
    ///
    /// Returns:
    ///     Currency: The yen, ISO code "JPY".
    #[staticmethod]
    fn jpy() -> Self {
        PyCurrency {
            inner: Currency::jpy(),
        }
    }

    /// Return the ISO 4217 three-letter code.
    ///
    /// Returns:
    ///     str: The three-letter code, e.g. "EUR".
    fn code(&self) -> &str {
        self.inner.code()
    }

    /// Return the printable representation, which prints the ISO code.
    ///
    /// Returns:
    ///     str: A string of the form Currency(EUR).
    fn __repr__(&self) -> String {
        format!("Currency({})", self.inner.code())
    }
}

impl PyCurrency {
    /// A clone of the inner currency for the index facades, whose constructors
    /// take a Currency by value.
    pub(crate) fn inner(&self) -> Currency {
        self.inner.clone()
    }

    /// Wraps a currency read back off a core index, for the facade getters.
    pub(crate) fn from_inner(inner: Currency) -> Self {
        PyCurrency { inner }
    }
}
