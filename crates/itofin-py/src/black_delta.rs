//! Immutable Black FX delta calculators and quote convention enums.
use crate::option::PyOptionType;
use crate::{ItofinError, PyQlError};
use libitofin::pricingengines::BlackDeltaCalculator;
use libitofin::quotes::{AtmType, DeltaType};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};

/// Spot, forward and premium-adjusted delta conventions.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "DeltaType",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.quotes"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyDeltaType {
    Spot = 0,
    Fwd = 1,
    PaSpot = 2,
    PaFwd = 3,
}

impl PyDeltaType {
    fn inner(self) -> DeltaType {
        match self {
            Self::Spot => DeltaType::Spot,
            Self::Fwd => DeltaType::Fwd,
            Self::PaSpot => DeltaType::PaSpot,
            Self::PaFwd => DeltaType::PaFwd,
        }
    }
}

/// ATM conventions. Null is invalid for calculation; PutCall50 requires Fwd.
#[gen_stub_pyclass_enum]
#[pyclass(name = "AtmType", eq, eq_int, from_py_object, module = "itofin.quotes")]
#[derive(Clone, Copy, PartialEq)]
pub enum PyAtmType {
    Null = 0,
    Spot = 1,
    Fwd = 2,
    DeltaNeutral = 3,
    VegaMax = 4,
    GammaMax = 5,
    PutCall50 = 6,
}

impl PyAtmType {
    fn inner(self) -> AtmType {
        match self {
            Self::Null => AtmType::Null,
            Self::Spot => AtmType::Spot,
            Self::Fwd => AtmType::Fwd,
            Self::DeltaNeutral => AtmType::DeltaNeutral,
            Self::VegaMax => AtmType::VegaMax,
            Self::GammaMax => AtmType::GammaMax,
            Self::PutCall50 => AtmType::PutCall50,
        }
    }
}

fn finite(value: f64) -> PyResult<f64> {
    if !value.is_finite() {
        return Err(ItofinError::new_err("Black delta result must be finite"));
    }
    Ok(value)
}

/// Immutable standalone FX delta/strike conversion, not a pricing engine.
/// Premium-adjusted calls invert on the high-strike branch.
#[gen_stub_pyclass]
#[pyclass(
    name = "BlackDeltaCalculator",
    frozen,
    module = "itofin.pricingengines"
)]
pub struct PyBlackDeltaCalculator {
    inner: BlackDeltaCalculator,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBlackDeltaCalculator {
    /// Copy positive finite spot/discounts and finite nonnegative std_dev.
    /// std_dev is volatility times sqrt(time). Construct a new calculator to
    /// change option or delta convention; no mutators are exposed.
    #[new]
    fn new(
        option_type: PyOptionType,
        delta_type: PyDeltaType,
        spot: f64,
        domestic_discount: f64,
        foreign_discount: f64,
        std_dev: f64,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: BlackDeltaCalculator::new(
                option_type.inner(),
                delta_type.inner(),
                spot,
                domestic_discount,
                foreign_discount,
                std_dev,
            )
            .map_err(PyQlError::from)?,
        })
    }
    /// Signed delta at finite nonnegative strike, including zero deviation.
    /// Premium-adjusted put deltas may be below minus one.
    fn delta_from_strike(&self, strike: f64) -> PyResult<f64> {
        finite(
            self.inner
                .delta_from_strike(strike)
                .map_err(PyQlError::from)?,
        )
    }
    /// Invert a signed interior delta with positive deviation. A premium-adjusted
    /// call returns the high-strike root, not necessarily the original strike.
    fn strike_from_delta(&self, delta: f64) -> PyResult<f64> {
        finite(
            self.inner
                .strike_from_delta(delta)
                .map_err(PyQlError::from)?,
        )
    }
    /// ATM strike; Null errors and PutCall50 is valid only for Fwd delta.
    fn atm_strike(&self, atm_type: PyAtmType) -> PyResult<f64> {
        finite(
            self.inner
                .atm_strike(atm_type.inner())
                .map_err(PyQlError::from)?,
        )
    }
}
