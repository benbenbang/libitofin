//! Python bindings for `libitofin`, published as the `itofin` extension module.
//!
//! This crate is the walking skeleton (issue #484): it builds an `abi3-py313`
//! wheel, imports as `itofin`, and bridges [`QlError`] to the Python-visible
//! [`struct@ItofinError`] exception. The pricing facades land in follow-up
//! tickets (#485-#487).

mod calibration;
mod curve;
mod helpers;
mod heston;
mod hullwhite;
mod market;
mod option;
mod settings;
mod swap;
mod swaption;
mod time;
mod vol;

use calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use curve::{
    PyDiscountCurve, PyFlatForward, PyForwardCurve, PyPiecewiseFlatForward,
    PyPiecewiseLinearForward, PyPiecewiseLinearZero, PyPiecewiseLogLinearDiscount,
    PyPiecewiseYieldCurve, PyYieldTermStructure, PyZeroCurve,
};
use helpers::{
    PyDepositRateHelper, PyFuturesRateHelper, PyFuturesType, PyRateHelper, PySwapRateHelper,
};
use heston::{PyHestonModel, PyHestonModelHelper, PyHestonProcess};
use hullwhite::{PyEuribor, PyHullWhite, PySwaptionHelper};
use libitofin::errors::QlError;
use market::{PyBlackScholesProcess, PySimpleQuote};
use option::{PyOptionType, PyVanillaOption};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use settings::PySettings;
use swap::{PySwapType, PyVanillaSwap};
use swaption::{PyEuropeanExercise, PySettlementMethod, PySettlementType, PySwaption};
use time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod, PySchedule,
};
use vol::{
    PyBlackConstantVol, PyBlackVarianceCurve, PyBlackVarianceSurface, PyBlackVolTermStructure,
};

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

/// Registers the eight `ql/`-faithful submodules on `itofin`.
///
/// Nested PyO3 modules give attribute access (`itofin.time.Date`) but do not
/// form a Python package, so `import itofin.time` / `from itofin.time import
/// Date` fail unless each submodule is also inserted into `sys.modules` under
/// its dotted name. The loop below does both: `add_submodule` for attribute
/// access and `sys.modules["itofin.<name>"]` for real imports.
#[pymodule]
fn itofin(m: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = m.py();

    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("ItofinError", py.get_type::<ItofinError>())?;
    m.add_class::<PySettings>()?;

    let time = PyModule::new(py, "time")?;
    time.add_class::<PyDate>()?;
    time.add_class::<PyPeriod>()?;
    time.add_class::<PyCalendar>()?;
    time.add_class::<PyDayCounter>()?;
    time.add_class::<PyFrequency>()?;
    time.add_class::<PyBusinessDayConvention>()?;
    time.add_class::<PySchedule>()?;
    crate::time::add_functions(&time)?;

    let quotes = PyModule::new(py, "quotes")?;
    quotes.add_class::<PySimpleQuote>()?;

    let termstructures = PyModule::new(py, "termstructures")?;
    termstructures.add_class::<PyYieldTermStructure>()?;
    termstructures.add_class::<PyBlackVolTermStructure>()?;
    termstructures.add_class::<PyFlatForward>()?;
    termstructures.add_class::<PyZeroCurve>()?;
    termstructures.add_class::<PyDiscountCurve>()?;
    termstructures.add_class::<PyForwardCurve>()?;
    termstructures.add_class::<PyBlackConstantVol>()?;
    termstructures.add_class::<PyBlackVarianceCurve>()?;
    termstructures.add_class::<PyBlackVarianceSurface>()?;
    termstructures.add_class::<PyRateHelper>()?;
    termstructures.add_class::<PyDepositRateHelper>()?;
    termstructures.add_class::<PySwapRateHelper>()?;
    termstructures.add_class::<PyFuturesType>()?;
    termstructures.add_class::<PyFuturesRateHelper>()?;
    termstructures.add_class::<PyPiecewiseYieldCurve>()?;
    termstructures.add_class::<PyPiecewiseLogLinearDiscount>()?;
    termstructures.add_class::<PyPiecewiseLinearZero>()?;
    termstructures.add_class::<PyPiecewiseLinearForward>()?;
    termstructures.add_class::<PyPiecewiseFlatForward>()?;

    let processes = PyModule::new(py, "processes")?;
    processes.add_class::<PyBlackScholesProcess>()?;
    processes.add_class::<PyHestonProcess>()?;

    let indexes = PyModule::new(py, "indexes")?;
    indexes.add_class::<PyEuribor>()?;

    let instruments = PyModule::new(py, "instruments")?;
    instruments.add_class::<PyOptionType>()?;
    instruments.add_class::<PyVanillaOption>()?;
    instruments.add_class::<PySwapType>()?;
    instruments.add_class::<PyVanillaSwap>()?;
    instruments.add_class::<PyEuropeanExercise>()?;
    instruments.add_class::<PySettlementType>()?;
    instruments.add_class::<PySettlementMethod>()?;
    instruments.add_class::<PySwaption>()?;

    let models = PyModule::new(py, "models")?;
    models.add_class::<PyHestonModel>()?;
    models.add_class::<PyHullWhite>()?;
    models.add_class::<PyHestonModelHelper>()?;
    models.add_class::<PySwaptionHelper>()?;
    models.add_class::<PyCalibrationErrorType>()?;

    let optimization = PyModule::new(py, "optimization")?;
    optimization.add_class::<PyLevenbergMarquardt>()?;
    optimization.add_class::<PyEndCriteria>()?;

    let submodules = [
        ("time", &time),
        ("quotes", &quotes),
        ("termstructures", &termstructures),
        ("processes", &processes),
        ("indexes", &indexes),
        ("instruments", &instruments),
        ("models", &models),
        ("optimization", &optimization),
    ];

    let sys_modules = PyModule::import(py, "sys")?.getattr("modules")?;
    let sys_modules = sys_modules.cast::<PyDict>()?;
    for (name, submodule) in submodules {
        m.add_submodule(submodule)?;
        sys_modules.set_item(format!("itofin.{name}"), submodule)?;
    }

    Ok(())
}
