//! Facade for the currency specification Currency.
//!
//! Currency values are reusable in index constructors. Text fields retain UTF-8
//! and embedded NULs; equality inside the core remains name-based. Rounding,
//! triangulation and minor-unit metadata remain outside this stored-field slice.

use libitofin::currency::Currency;
use pyo3::prelude::*;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// An ISO 4217 currency specification.
///
/// A custom code contains exactly three ASCII letters, without registry lookup.
/// Numeric fields retain the core's signed 32-bit integer semantics.
#[gen_stub_pyclass]
#[pyclass(name = "Currency", unsendable, module = "itofin.indexes")]
pub struct PyCurrency {
    inner: Currency,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyCurrency {
    /// Construct a reusable currency specification, copying every stored field.
    ///
    /// Args:
    ///     name (str): Name used for core currency equality.
    ///     code (str): Exactly three ASCII letters; no registry lookup or normalization.
    ///     numeric_code (int): Signed 32-bit numeric identifier.
    ///     symbol (str): Currency symbol, retaining Unicode and embedded NULs.
    ///     fraction_symbol (str): Fraction symbol, retaining Unicode and embedded NULs.
    ///     fractions_per_unit (int): Signed 32-bit number of fractions in one unit.
    ///
    /// Raises:
    ///     ValueError: If code is not three ASCII letters.
    #[new]
    fn new(
        name: String,
        code: String,
        numeric_code: i32,
        symbol: String,
        fraction_symbol: String,
        fractions_per_unit: i32,
    ) -> PyResult<Self> {
        if code.len() != 3 || !code.bytes().all(|b| b.is_ascii_alphabetic()) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "currency code needs three ASCII letters",
            ));
        }
        Ok(Self {
            inner: Currency::new(
                name,
                code,
                numeric_code,
                symbol,
                fraction_symbol,
                fractions_per_unit,
            ),
        })
    }

    /// Return the name used for core currency equality.
    fn name(&self) -> &str {
        self.inner.name()
    }

    /// Return the signed numeric identifier without registry lookup.
    fn numeric_code(&self) -> i32 {
        self.inner.numeric_code()
    }

    /// Return the symbol, retaining Unicode and embedded NULs.
    fn symbol(&self) -> &str {
        self.inner.symbol()
    }

    /// Return the fraction symbol, retaining Unicode and embedded NULs.
    fn fraction_symbol(&self) -> &str {
        self.inner.fraction_symbol()
    }

    /// Return the stored number of fractions per unit.
    fn fractions_per_unit(&self) -> i32 {
        self.inner.fractions_per_unit()
    }

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
