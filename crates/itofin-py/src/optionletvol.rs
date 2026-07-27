//! Facades for the optionlet (caplet/floorlet) volatility stack: the
//! [`PyOptionletVolatilityStructure`] base and the constant surface
//! [`PyConstantOptionletVolatility`].
//!
//! The base holds the erased `Handle<dyn OptionletVolatilityStructure>` and
//! exposes the queries every concrete surface inherits; concrete surfaces
//! subclass it and supply only their constructor. They build the base through
//! [`from_handle`](PyOptionletVolatilityStructure::from_handle) rather than a
//! struct literal, so the surfaces stacking on it in later tickets never need
//! access to the private field.
//!
//! Unlike the swaption surfaces, an optionlet surface has a single option axis:
//! a query takes one option tenor (or date) and a strike, not an option/swap
//! tenor pair.
//!
//! Deferred (visible): the MOVING `ConstantOptionletVolatility` constructors
//! (`moving` / `moving_with_quote`, whose reference date floats off the
//! evaluation date) are not exposed; only the fixed-reference-date `new` and
//! `with_quote` are, as for the constant swaption surface. Tracked as #627.
//! `BlackCapFloorEngine.with_flat_vol` builds a moving surface internally, but
//! that is the engine's business, not this facade's.

use crate::PyQlError;
use crate::market::PySimpleQuote;
use crate::swaptionvol::PyVolatilityType;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    ConstantOptionletVolatility, OptionletVolatilityStructure,
};
use pyo3::prelude::*;

/// Python `OptionletVolatilityStructure`: the shared base for every caplet
/// volatility surface
/// (`termstructures::volatility::OptionletVolatilityStructure`).
///
/// The option axis is addressed by tenor, the form the surfaces are quoted in;
/// the core resolves the tenor against the surface's reference date and calendar
/// before reading the volatility. The date form is exposed too, since the
/// optionlet stripper and the cap/floor engine both address the surface by a
/// coupon's fixing date.
#[pyclass(name = "OptionletVolatilityStructure", subclass, unsendable)]
pub struct PyOptionletVolatilityStructure {
    inner: Handle<dyn OptionletVolatilityStructure>,
}

#[pymethods]
impl PyOptionletVolatilityStructure {
    /// The caplet volatility for an option tenor and strike.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn volatility(&self, option_tenor: &PyPeriod, strike: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The caplet volatility for an option date and strike.
    #[pyo3(signature = (option_date, strike, extrapolate = false))]
    fn volatility_date(
        &self,
        option_date: &PyDate,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_date(option_date.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The Black variance (`vol^2 * option_time`) for an option tenor and strike.
    #[pyo3(signature = (option_tenor, strike, extrapolate = false))]
    fn black_variance(
        &self,
        option_tenor: &PyPeriod,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance_tenor(option_tenor.inner(), strike, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Whether the surface answers dates/times beyond its maximum.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allows extrapolation past the maximum date/time.
    ///
    /// A stripped surface ends at its last optionlet fixing, so a cap whose own
    /// last caplet fixes there queries the boundary; the core's round-trip
    /// fixture enables extrapolation before repricing
    /// (`strippedoptionletadapter.rs:393`).
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbids extrapolation past the maximum date/time.
    fn disable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .disable_extrapolation();
        Ok(())
    }

    /// The lognormal shift applied to forwards and strikes; `0.0` for the
    /// unshifted lognormal and the normal model.
    ///
    /// This is what `BlackCapFloorEngine` checks a caller-supplied displacement
    /// against, so it is the number to read before pinning one on the engine.
    fn displacement(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .displacement())
    }
}

impl PyOptionletVolatilityStructure {
    /// A clone of the inner surface handle for the engine facades.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn OptionletVolatilityStructure> {
        self.inner.clone()
    }

    /// The base a concrete surface's `#[new]` extends, built from its erased
    /// handle. The named constructor keeps the private field an implementation
    /// detail of this module.
    pub(crate) fn from_handle(inner: Handle<dyn OptionletVolatilityStructure>) -> Self {
        PyOptionletVolatilityStructure { inner }
    }
}

/// Python `ConstantOptionletVolatility`: a single caplet volatility with no
/// option-time or strike dependence
/// (`termstructures::volatility::ConstantOptionletVolatility`).
///
/// Extends [`PyOptionletVolatilityStructure`] and supplies only the
/// constructors; the query surface is inherited. Unbounded in time and strike,
/// so queries never need extrapolation enabled. Both forms pin the reference
/// date, so the option time every query measures runs from `reference_date`, not
/// from the evaluation date.
#[pyclass(name = "ConstantOptionletVolatility", extends = PyOptionletVolatilityStructure, unsendable)]
pub struct PyConstantOptionletVolatility;

#[pymethods]
impl PyConstantOptionletVolatility {
    /// A constant surface at a fixed `volatility`, wrapped in an internal quote
    /// the caller cannot later mutate.
    #[new]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, displacement = 0.0))]
    fn new(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: f64,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        displacement: f64,
    ) -> PyClassInitializer<Self> {
        let surface = shared(ConstantOptionletVolatility::new(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility,
            day_counter.inner(),
            volatility_type.inner(),
            displacement,
        )) as Shared<dyn OptionletVolatilityStructure>;
        PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
            surface,
        )))
        .add_subclass(PyConstantOptionletVolatility)
    }

    /// A constant surface reading `volatility` from the caller's quote; a later
    /// `set_value` on that quote notifies the surface's observers.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, displacement = 0.0))]
    fn with_quote(
        py: Python<'_>,
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: &PySimpleQuote,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        displacement: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantOptionletVolatility::with_quote(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility.handle(),
            day_counter.inner(),
            volatility_type.inner(),
            displacement,
        )) as Shared<dyn OptionletVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyOptionletVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantOptionletVolatility),
        )
    }
}
