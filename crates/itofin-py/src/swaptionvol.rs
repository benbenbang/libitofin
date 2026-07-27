//! Facades for the swaption volatility stack: the [`PySwaptionVolatilityStructure`]
//! base, the [`PyVolatilityType`] flag and the constant surface
//! [`PyConstantSwaptionVolatility`].
//!
//! The base holds the erased `Handle<dyn SwaptionVolatilityStructure>` and
//! exposes the queries every concrete surface inherits; concrete surfaces
//! subclass it and supply only their constructor. They build the base through
//! [`from_handle`](PySwaptionVolatilityStructure::from_handle) rather than a
//! struct literal, so the later surfaces in this file (and the matrix/cube
//! facades stacking on it) never need access to the private field.
//!
//! Deferred (visible): the MOVING `ConstantSwaptionVolatility` constructors
//! (`moving` / `moving_with_quote`, whose reference date floats off the
//! evaluation date) are not exposed; only the fixed-reference-date `new` and
//! `with_quote` are. `BlackSwaptionEngine.with_flat_vol` builds a moving
//! surface internally, but that is the engine's business, not this facade's.

use crate::PyQlError;
use crate::market::PySimpleQuote;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::handle::Handle;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::volatility::{
    ConstantSwaptionVolatility, SwaptionVolatilityStructure, VolatilityType,
};
use pyo3::prelude::*;

/// Python `SwaptionVolatilityStructure`: the shared base for every swaption
/// volatility surface (`termstructures::volatility::SwaptionVolatilityStructure`).
///
/// The option and swap axes are addressed by tenor, the form the surfaces are
/// quoted in; the core resolves each tenor against the surface's reference date
/// and calendar before reading the volatility.
#[pyclass(name = "SwaptionVolatilityStructure", subclass, unsendable)]
pub struct PySwaptionVolatilityStructure {
    inner: Handle<dyn SwaptionVolatilityStructure>,
}

#[pymethods]
impl PySwaptionVolatilityStructure {
    /// The volatility for an option tenor, swap tenor and strike.
    #[pyo3(signature = (option_tenor, swap_tenor, strike, extrapolate = false))]
    fn volatility(
        &self,
        option_tenor: &PyPeriod,
        swap_tenor: &PyPeriod,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .volatility_tenors(
                option_tenor.inner(),
                swap_tenor.inner(),
                strike,
                extrapolate,
            )
            .map_err(PyQlError::from)?)
    }

    /// The Black variance (`vol^2 * option_time`) for an option tenor, swap
    /// tenor and strike.
    #[pyo3(signature = (option_tenor, swap_tenor, strike, extrapolate = false))]
    fn black_variance(
        &self,
        option_tenor: &PyPeriod,
        swap_tenor: &PyPeriod,
        strike: f64,
        extrapolate: bool,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .black_variance_tenors(
                option_tenor.inner(),
                swap_tenor.inner(),
                strike,
                extrapolate,
            )
            .map_err(PyQlError::from)?)
    }

    /// The lognormal shift for an option date and swap length in years.
    ///
    /// Taken in the date form because the core trait has no tenor overload for
    /// the shift (only `shift` and `shift_time`), unlike the volatility and
    /// variance queries above. Errors on a normal-volatility surface, where a
    /// shift has no meaning.
    #[pyo3(signature = (option_date, swap_length, extrapolate = false))]
    fn shift(&self, option_date: &PyDate, swap_length: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .shift(option_date.inner(), swap_length, extrapolate)
            .map_err(PyQlError::from)?)
    }
}

impl PySwaptionVolatilityStructure {
    /// A clone of the inner surface handle for the engine facades.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn SwaptionVolatilityStructure> {
        self.inner.clone()
    }

    /// The base a concrete surface's `#[new]` extends, built from its erased
    /// handle. The named constructor keeps the private field an implementation
    /// detail of this module.
    pub(crate) fn from_handle(inner: Handle<dyn SwaptionVolatilityStructure>) -> Self {
        PySwaptionVolatilityStructure { inner }
    }
}

/// Python `VolatilityType`: whether a surface quotes shifted-lognormal (Black)
/// or normal (Bachelier) volatilities
/// (`termstructures::volatility::VolatilityType`).
///
/// A fieldless pyo3 enum. The engine checks the surface it is handed against
/// its own formula and errors at pricing time on a mismatch, so a `Normal`
/// surface fed to `BlackSwaptionEngine` surfaces as an `ItofinError` from
/// `npv()`, not from the constructor.
#[pyclass(name = "VolatilityType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyVolatilityType {
    ShiftedLognormal,
    Normal,
}

impl PyVolatilityType {
    /// The core [`VolatilityType`] this variant stands for.
    fn inner(&self) -> VolatilityType {
        match self {
            PyVolatilityType::ShiftedLognormal => VolatilityType::ShiftedLognormal,
            PyVolatilityType::Normal => VolatilityType::Normal,
        }
    }
}

/// Python `ConstantSwaptionVolatility`: a single volatility with no option-time,
/// swap-length or strike dependence
/// (`termstructures::volatility::ConstantSwaptionVolatility`).
///
/// Extends [`PySwaptionVolatilityStructure`] and supplies only the constructors;
/// the query surface is inherited. Unbounded in time and strike, so queries
/// never need extrapolation enabled. Both forms pin the reference date, so the
/// option time every query measures runs from `reference_date`, not from the
/// evaluation date.
#[pyclass(name = "ConstantSwaptionVolatility", extends = PySwaptionVolatilityStructure, unsendable)]
pub struct PyConstantSwaptionVolatility;

#[pymethods]
impl PyConstantSwaptionVolatility {
    /// A constant surface at a fixed `volatility`, wrapped in an internal quote
    /// the caller cannot later mutate.
    #[new]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, shift = 0.0))]
    fn new(
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: f64,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        shift: f64,
    ) -> PyClassInitializer<Self> {
        let surface = shared(ConstantSwaptionVolatility::new(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility,
            day_counter.inner(),
            volatility_type.inner(),
            shift,
        )) as Shared<dyn SwaptionVolatilityStructure>;
        PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
            surface,
        )))
        .add_subclass(PyConstantSwaptionVolatility)
    }

    /// A constant surface reading `volatility` from the caller's quote; a later
    /// `set_value` on that quote notifies the surface's observers.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (reference_date, calendar, business_day_convention, volatility, day_counter, volatility_type, shift = 0.0))]
    fn with_quote(
        py: Python<'_>,
        reference_date: &PyDate,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        volatility: &PySimpleQuote,
        day_counter: &PyDayCounter,
        volatility_type: PyVolatilityType,
        shift: f64,
    ) -> PyResult<Py<Self>> {
        let surface = shared(ConstantSwaptionVolatility::with_quote(
            reference_date.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            volatility.handle(),
            day_counter.inner(),
            volatility_type.inner(),
            shift,
        )) as Shared<dyn SwaptionVolatilityStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PySwaptionVolatilityStructure::from_handle(Handle::new(
                surface,
            )))
            .add_subclass(PyConstantSwaptionVolatility),
        )
    }
}
