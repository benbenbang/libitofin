//! Python bindings for `libitofin`, published as the `itofin` extension module.
//!
//! This crate is the walking skeleton (issue #484): it builds an `abi3-py313`
//! wheel, imports as `itofin`, and bridges [`QlError`] to the Python-visible
//! [`struct@ItofinError`] exception. The pricing facades land in follow-up
//! tickets (#485-#487).

mod calibration;
mod capfloor;
mod capfloorengine;
mod capfloortermvol;
mod cashflows;
mod credit;
mod creditengine;
mod credithelpers;
mod currency;
mod curve;
mod fra;
mod helpers;
mod heston;
mod hullwhite;
mod inflation;
mod market;
mod mcengine;
mod ois;
mod option;
mod optionletvol;
mod results;
mod settings;
mod smilesection;
mod swap;
mod swapindex;
mod swaption;
mod swaptionengine;
mod swaptionvol;
mod time;
mod vol;

use calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use capfloor::{PyCapFloor, PyCapFloorType};
use capfloorengine::PyBlackCapFloorEngine;
use capfloortermvol::PyCapFloorTermVolSurface;
use cashflows::{
    PyCappedFlooredYoYInflationCoupon, PyCashFlow, PyIborLeg, PyLeg, PyYoYInflationCoupon,
    PyYoYInflationLeg, PyYoYInflationOptionletCouponPricer,
};
use credit::{
    PyCreditDefaultSwap, PyDefaultProbabilityTermStructure, PyFlatHazardRate,
    PyInterpolatedHazardRateCurve, PyMakeCreditDefaultSwap, PyPiecewiseDefaultCurve,
    PyPricingModel, PyProtectionSide,
};
use creditengine::{
    PyAccrualBias, PyForwardsInCouponPeriod, PyIsdaCdsEngine, PyMidPointCdsEngine, PyNumericalFix,
};
use credithelpers::{PyDefaultProbabilityHelper, PySpreadCdsHelper};
use currency::PyCurrency;
use curve::{
    PyDiscountCurve, PyFlatForward, PyForwardCurve, PyPiecewiseConvexMonotoneForward,
    PyPiecewiseCubicZero, PyPiecewiseFlatForward, PyPiecewiseLinearForward, PyPiecewiseLinearZero,
    PyPiecewiseLogLinearDiscount, PyPiecewiseYieldCurve, PyYieldTermStructure, PyZeroCurve,
};
use fra::{PyForwardRateAgreement, PyPosition};
use helpers::{
    PyDepositRateHelper, PyEstr, PyFraRateHelper, PyFuturesRateHelper, PyFuturesType,
    PyOISRateHelper, PyOvernightIndex, PyPillar, PyRateAveraging, PyRateHelper, PySwapRateHelper,
};
use heston::{PyHestonModel, PyHestonModelHelper, PyHestonProcess};
use hullwhite::{
    PyEurLibor, PyEuribor, PyGbpLibor, PyHullWhite, PyIborIndex, PyJpyLibor, PySwaptionHelper,
    PyUsdLibor,
};
use inflation::{
    PyConstantYoYOptionletVolatility, PyCpiInterpolationType, PyDiscountingSwapEngine,
    PyInterpolatedYoYInflationCurve, PyInterpolatedZeroInflationCurve,
    PyKInterpolatedYoYOptionletVolatilitySurface, PyMakeYoYInflationCapFloor,
    PyMultiplicativePriceSeasonality, PyPiecewiseYoYInflationCurve, PyPiecewiseZeroInflationCurve,
    PyYearOnYearInflationSwap, PyYearOnYearInflationSwapHelper, PyYoYCapFloorTermPriceSurface,
    PyYoYInflationCapFloor, PyYoYInflationCapFloorEngine, PyYoYInflationHelper,
    PyYoYInflationIndex, PyYoYInflationTermStructure, PyZeroCouponInflationSwap,
    PyZeroCouponInflationSwapHelper, PyZeroInflationHelper, PyZeroInflationIndex,
    PyZeroInflationTermStructure,
};
use libitofin::errors::QlError;
use market::{PyBlackScholesProcess, PySimpleQuote};
use mcengine::{PyMCAmericanEngine, PyMCEuropeanEngine, PyMCEuropeanHestonEngine};
use ois::{PyMakeOis, PyOvernightIndexedSwap};
use option::{PyOptionType, PyVanillaOption};
use optionletvol::{
    PyConstantOptionletVolatility, PyOptionletStripper1, PyOptionletVolatilityStructure,
    PyStrippedOptionletAdapter,
};
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use results::Results;
use settings::PySettings;
use smilesection::PySabrSmileSection;
use swap::{PyMakeVanillaSwap, PySwapType, PyVanillaSwap};
use swapindex::PySwapIndex;
use swaption::{PyEuropeanExercise, PySettlementMethod, PySettlementType, PySwaption};
use swaptionengine::{PyBachelierSwaptionEngine, PyBlackSwaptionEngine, PyCashAnnuityModel};
use swaptionvol::{
    PyConstantSwaptionVolatility, PyInterpolatedSwaptionVolatilityCube,
    PySabrSwaptionVolatilityCube, PySwaptionVolatilityMatrix, PySwaptionVolatilityStructure,
    PyVolatilityType,
};
use time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDateGeneration, PyDayCounter, PyFrequency,
    PyPeriod, PySchedule,
};
use vol::{
    PyBlackConstantVol, PyBlackVarianceCurve, PyBlackVarianceSurface, PyBlackVolTermStructure,
    PyBlackVolTimeExtrapolation,
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

/// Registers the eleven `ql/`-faithful submodules on `itofin`.
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
    time.add_class::<PyDateGeneration>()?;
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
    termstructures.add_class::<PyBlackVolTimeExtrapolation>()?;
    termstructures.add_class::<PyBlackVarianceCurve>()?;
    termstructures.add_class::<PyBlackVarianceSurface>()?;
    termstructures.add_class::<PyRateHelper>()?;
    termstructures.add_class::<PyDepositRateHelper>()?;
    termstructures.add_class::<PySwapRateHelper>()?;
    termstructures.add_class::<PyFuturesType>()?;
    termstructures.add_class::<PyFuturesRateHelper>()?;
    termstructures.add_class::<PyPillar>()?;
    termstructures.add_class::<PyFraRateHelper>()?;
    termstructures.add_class::<PyRateAveraging>()?;
    termstructures.add_class::<PyOISRateHelper>()?;
    termstructures.add_class::<PyPiecewiseYieldCurve>()?;
    termstructures.add_class::<PyPiecewiseLogLinearDiscount>()?;
    termstructures.add_class::<PyPiecewiseLinearZero>()?;
    termstructures.add_class::<PyPiecewiseCubicZero>()?;
    termstructures.add_class::<PyPiecewiseLinearForward>()?;
    termstructures.add_class::<PyPiecewiseConvexMonotoneForward>()?;
    termstructures.add_class::<PyPiecewiseFlatForward>()?;
    termstructures.add_class::<PySwaptionVolatilityStructure>()?;
    termstructures.add_class::<PyVolatilityType>()?;
    termstructures.add_class::<PyConstantSwaptionVolatility>()?;
    termstructures.add_class::<PySwaptionVolatilityMatrix>()?;
    termstructures.add_class::<PyInterpolatedSwaptionVolatilityCube>()?;
    termstructures.add_class::<PySabrSwaptionVolatilityCube>()?;
    termstructures.add_class::<PySabrSmileSection>()?;
    termstructures.add_class::<PyOptionletVolatilityStructure>()?;
    termstructures.add_class::<PyConstantOptionletVolatility>()?;
    termstructures.add_class::<PyCapFloorTermVolSurface>()?;
    termstructures.add_class::<PyOptionletStripper1>()?;
    termstructures.add_class::<PyStrippedOptionletAdapter>()?;
    termstructures.add_class::<PyDefaultProbabilityTermStructure>()?;
    termstructures.add_class::<PyFlatHazardRate>()?;
    termstructures.add_class::<PyInterpolatedHazardRateCurve>()?;
    termstructures.add_class::<PyDefaultProbabilityHelper>()?;
    termstructures.add_class::<PySpreadCdsHelper>()?;
    termstructures.add_class::<PyPiecewiseDefaultCurve>()?;
    termstructures.add_class::<PyZeroInflationTermStructure>()?;
    termstructures.add_class::<PyInterpolatedZeroInflationCurve>()?;
    termstructures.add_class::<PyZeroInflationHelper>()?;
    termstructures.add_class::<PyZeroCouponInflationSwapHelper>()?;
    termstructures.add_class::<PyPiecewiseZeroInflationCurve>()?;
    termstructures.add_class::<PyMultiplicativePriceSeasonality>()?;
    termstructures.add_class::<PyYoYInflationTermStructure>()?;
    termstructures.add_class::<PyInterpolatedYoYInflationCurve>()?;
    termstructures.add_class::<PyYoYInflationHelper>()?;
    termstructures.add_class::<PyYearOnYearInflationSwapHelper>()?;
    termstructures.add_class::<PyPiecewiseYoYInflationCurve>()?;
    termstructures.add_class::<PyConstantYoYOptionletVolatility>()?;
    termstructures.add_class::<PyYoYCapFloorTermPriceSurface>()?;
    termstructures.add_class::<PyKInterpolatedYoYOptionletVolatilitySurface>()?;

    let processes = PyModule::new(py, "processes")?;
    processes.add_class::<PyBlackScholesProcess>()?;
    processes.add_class::<PyHestonProcess>()?;

    let indexes = PyModule::new(py, "indexes")?;
    indexes.add_class::<PyCurrency>()?;
    indexes.add_class::<PyIborIndex>()?;
    indexes.add_class::<PyEuribor>()?;
    indexes.add_class::<PyUsdLibor>()?;
    indexes.add_class::<PyJpyLibor>()?;
    indexes.add_class::<PyGbpLibor>()?;
    indexes.add_class::<PyEurLibor>()?;
    indexes.add_class::<PyOvernightIndex>()?;
    indexes.add_class::<PyEstr>()?;
    indexes.add_class::<PySwapIndex>()?;
    indexes.add_class::<PyCpiInterpolationType>()?;
    indexes.add_class::<PyZeroInflationIndex>()?;
    indexes.add_class::<PyYoYInflationIndex>()?;

    let cashflows = PyModule::new(py, "cashflows")?;
    cashflows.add_class::<PyYoYInflationCoupon>()?;
    cashflows.add_class::<PyCappedFlooredYoYInflationCoupon>()?;
    cashflows.add_class::<PyYoYInflationOptionletCouponPricer>()?;
    cashflows.add_class::<PyYoYInflationLeg>()?;
    cashflows.add_class::<PyIborLeg>()?;
    cashflows.add_class::<PyCashFlow>()?;
    cashflows.add_class::<PyLeg>()?;
    cashflows.add_function(wrap_pyfunction!(cashflows::npv, &cashflows)?)?;

    let instruments = PyModule::new(py, "instruments")?;
    instruments.add_class::<PyOptionType>()?;
    instruments.add_class::<PyVanillaOption>()?;
    instruments.add_class::<PySwapType>()?;
    instruments.add_class::<PyVanillaSwap>()?;
    instruments.add_class::<PyMakeVanillaSwap>()?;
    instruments.add_class::<PyPosition>()?;
    instruments.add_class::<PyForwardRateAgreement>()?;
    instruments.add_class::<PyOvernightIndexedSwap>()?;
    instruments.add_class::<PyMakeOis>()?;
    instruments.add_class::<PyEuropeanExercise>()?;
    instruments.add_class::<PySettlementType>()?;
    instruments.add_class::<PySettlementMethod>()?;
    instruments.add_class::<PySwaption>()?;
    instruments.add_class::<PyCapFloorType>()?;
    instruments.add_class::<PyCapFloor>()?;
    instruments.add_class::<PyProtectionSide>()?;
    instruments.add_class::<PyPricingModel>()?;
    instruments.add_class::<PyCreditDefaultSwap>()?;
    instruments.add_class::<PyMakeCreditDefaultSwap>()?;
    instruments.add_class::<PyZeroCouponInflationSwap>()?;
    instruments.add_class::<PyYearOnYearInflationSwap>()?;
    instruments.add_class::<PyMakeYoYInflationCapFloor>()?;
    instruments.add_class::<PyYoYInflationCapFloor>()?;

    let models = PyModule::new(py, "models")?;
    models.add_class::<PyHestonModel>()?;
    models.add_class::<PyHullWhite>()?;
    models.add_class::<PyHestonModelHelper>()?;
    models.add_class::<PySwaptionHelper>()?;
    models.add_class::<PyCalibrationErrorType>()?;

    let pricingengines = PyModule::new(py, "pricingengines")?;
    pricingengines.add_class::<PyCashAnnuityModel>()?;
    pricingengines.add_class::<PyBlackSwaptionEngine>()?;
    pricingengines.add_class::<PyBachelierSwaptionEngine>()?;
    pricingengines.add_class::<PyBlackCapFloorEngine>()?;
    pricingengines.add_class::<PyMidPointCdsEngine>()?;
    pricingengines.add_class::<PyIsdaCdsEngine>()?;
    pricingengines.add_class::<PyNumericalFix>()?;
    pricingengines.add_class::<PyAccrualBias>()?;
    pricingengines.add_class::<PyForwardsInCouponPeriod>()?;
    pricingengines.add_class::<PyDiscountingSwapEngine>()?;
    pricingengines.add_class::<PyYoYInflationCapFloorEngine>()?;
    pricingengines.add_class::<PyMCEuropeanEngine>()?;
    pricingengines.add_class::<PyMCEuropeanHestonEngine>()?;
    pricingengines.add_class::<PyMCAmericanEngine>()?;

    let optimization = PyModule::new(py, "optimization")?;
    optimization.add_class::<PyLevenbergMarquardt>()?;
    optimization.add_class::<PyEndCriteria>()?;

    let results = PyModule::new(py, "results")?;
    results.add_class::<Results>()?;

    let submodules = [
        ("time", &time),
        ("quotes", &quotes),
        ("termstructures", &termstructures),
        ("processes", &processes),
        ("indexes", &indexes),
        ("cashflows", &cashflows),
        ("instruments", &instruments),
        ("models", &models),
        ("pricingengines", &pricingengines),
        ("optimization", &optimization),
        ("results", &results),
    ];

    let sys_modules = PyModule::import(py, "sys")?.getattr("modules")?;
    let sys_modules = sys_modules.cast::<PyDict>()?;
    for (name, submodule) in submodules {
        m.add_submodule(submodule)?;
        sys_modules.set_item(format!("itofin.{name}"), submodule)?;
    }

    Ok(())
}
