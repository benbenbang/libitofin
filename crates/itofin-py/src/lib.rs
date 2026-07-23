//! Python bindings for `libitofin`, published as the `itofin` extension module.
//!
//! This crate is the walking skeleton (issue #484): it builds an `abi3-py313`
//! wheel, imports as `itofin`, and bridges [`QlError`] to the Python-visible
//! [`struct@ItofinError`] exception. The pricing facades land in follow-up
//! tickets (#485-#487).

mod calibration;
mod curve;
mod heston;
mod hullwhite;
mod market;
mod option;
mod settings;
mod time;

use calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use curve::PyFlatForward;
use heston::{PyHestonModel, PyHestonModelHelper, PyHestonProcess};
use hullwhite::{PyEuribor, PyHullWhite, PySwaptionHelper};
use libitofin::errors::QlError;
use market::{PyBlackScholesProcess, PySimpleQuote};
use option::{PyOptionType, PyVanillaOption};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use settings::PySettings;
use time::{PyCalendar, PyDate, PyDayCounter, PyPeriod};

create_exception!(itofin, ItofinError, PyException);

/// Newtype bridging [`QlError`] to [`PyErr`] across the crate boundary.
///
/// A direct `impl From<QlError> for PyErr` is an orphan-rule violation
/// (E0117): both types are foreign to this crate. This wrapper carries the
/// two conversions instead, so fallible facades can return
/// `Result<T, PyQlError>` and use `?` on any `QlResult`. The Python-visible
/// contract is unchanged: the error surfaces as an [`struct@ItofinError`]
/// carrying the located `Display` form (`"file:line: message"`).
pub struct PyQlError(QlError);

impl From<QlError> for PyQlError {
    fn from(err: QlError) -> Self {
        PyQlError(err)
    }
}

impl From<PyQlError> for PyErr {
    fn from(err: PyQlError) -> Self {
        ItofinError::new_err(err.0.to_string())
    }
}

#[pymodule]
fn itofin(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("ItofinError", m.py().get_type::<ItofinError>())?;
    m.add_class::<PySettings>()?;
    m.add_class::<PyDate>()?;
    m.add_class::<PyDayCounter>()?;
    m.add_class::<PyCalendar>()?;
    m.add_class::<PyPeriod>()?;
    m.add_class::<PySimpleQuote>()?;
    m.add_class::<PyBlackScholesProcess>()?;
    m.add_class::<PyFlatForward>()?;
    m.add_class::<PyOptionType>()?;
    m.add_class::<PyVanillaOption>()?;
    m.add_class::<PyHestonProcess>()?;
    m.add_class::<PyHestonModel>()?;
    m.add_class::<PyHestonModelHelper>()?;
    m.add_class::<PyHullWhite>()?;
    m.add_class::<PyEuribor>()?;
    m.add_class::<PySwaptionHelper>()?;
    m.add_class::<PyLevenbergMarquardt>()?;
    m.add_class::<PyEndCriteria>()?;
    m.add_class::<PyCalibrationErrorType>()?;
    Ok(())
}
