//! Facade for the currency specification [`PyCurrency`] (`currency::Currency`).
//!
//! An index carries a currency, so the general [`PyIborIndex`](crate::hullwhite::PyIborIndex)
//! constructor cannot be called without one. Only the four named currencies the
//! core provides are exposed as staticmethods; the general
//! [`Currency::new`](libitofin::currency::Currency::new) form is deliberately
//! omitted, since the core itself ports only the currencies the indexes need and
//! the full `ql/currencies/*` catalogue is deferred there (`currency.rs:9`).
//!
//! It is registered under `itofin.indexes` rather than a submodule of its own:
//! the index constructors are its only consumers today, both the general
//! [`PyIborIndex`](crate::hullwhite::PyIborIndex) one and, since #868,
//! [`PySwapIndex`](crate::swapindex::PySwapIndex), which used to hard-code EUR.

use libitofin::currency::Currency;
use pyo3::prelude::*;

/// Python `Currency`: an ISO 4217 currency specification (`currency::Currency`).
///
/// Two currencies compare equal on their name alone, matching the core's
/// `PartialEq` (`currency.rs:132`), and `repr` prints the ISO code, matching its
/// `Display` (`currency.rs:126`).
#[pyclass(name = "Currency", unsendable)]
pub struct PyCurrency {
    inner: Currency,
}

#[pymethods]
impl PyCurrency {
    /// The European Euro (ISO `EUR`).
    #[staticmethod]
    fn eur() -> Self {
        PyCurrency {
            inner: Currency::eur(),
        }
    }

    /// The U.S. dollar (ISO `USD`).
    #[staticmethod]
    fn usd() -> Self {
        PyCurrency {
            inner: Currency::usd(),
        }
    }

    /// The British pound sterling (ISO `GBP`).
    #[staticmethod]
    fn gbp() -> Self {
        PyCurrency {
            inner: Currency::gbp(),
        }
    }

    /// The Japanese yen (ISO `JPY`).
    #[staticmethod]
    fn jpy() -> Self {
        PyCurrency {
            inner: Currency::jpy(),
        }
    }

    /// The ISO 4217 three-letter code, e.g. `"EUR"`.
    fn code(&self) -> &str {
        self.inner.code()
    }

    fn __repr__(&self) -> String {
        format!("Currency({})", self.inner.code())
    }
}

impl PyCurrency {
    /// A clone of the inner currency for the index facades, whose constructors
    /// take a [`Currency`] by value.
    pub(crate) fn inner(&self) -> Currency {
        self.inner.clone()
    }

    /// Wraps a currency read back off a core index, for the facade getters.
    pub(crate) fn from_inner(inner: Currency) -> Self {
        PyCurrency { inner }
    }
}
