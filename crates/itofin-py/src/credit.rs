//! Facades for the credit hierarchy: the [`PyDefaultProbabilityTermStructure`]
//! base, the concrete [`PyFlatHazardRate`] curve, and the
//! [`PyProtectionSide`] flag.

use crate::PyQlError;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use libitofin::handle::Handle;
use libitofin::instruments::ProtectionSide;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use libitofin::termstructures::credit::flathazardrate::FlatHazardRate;
use libitofin::types::Natural;
use pyo3::prelude::*;

/// Python `ProtectionSide`: which leg of a default-protection contract a party
/// holds (core `instruments::ProtectionSide`).
///
/// A fieldless pyo3 enum exposing `ProtectionSide.Buyer` /
/// `ProtectionSide.Seller`; the buyer pays the premium leg and receives the
/// default payment, the seller the reverse.
#[pyclass(name = "ProtectionSide", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyProtectionSide {
    Buyer,
    Seller,
}

impl PyProtectionSide {
    /// The core [`ProtectionSide`] this variant stands for.
    #[allow(dead_code)]
    pub(crate) fn inner(self) -> ProtectionSide {
        match self {
            PyProtectionSide::Buyer => ProtectionSide::Buyer,
            PyProtectionSide::Seller => ProtectionSide::Seller,
        }
    }
}

/// Python `DefaultProbabilityTermStructure`: the shared base for every credit
/// curve (`termstructures::credit::defaulttermstructure`).
///
/// Holds the erased `Handle<dyn DefaultProbabilityTermStructure>` and exposes
/// the query surface every concrete curve inherits: survival and default
/// probabilities, the default density, and the hazard rate, each in a
/// year-fraction and a date form. Concrete curves such as [`PyFlatHazardRate`]
/// subclass this and supply only their constructor.
#[pyclass(name = "DefaultProbabilityTermStructure", subclass, unsendable)]
pub struct PyDefaultProbabilityTermStructure {
    inner: Handle<dyn DefaultProbabilityTermStructure>,
}

#[pymethods]
impl PyDefaultProbabilityTermStructure {
    /// The survival probability from the reference date to year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn survival_probability(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .survival_probability(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The survival probability from the reference date to `date`.
    #[pyo3(signature = (date, extrapolate = false))]
    fn survival_probability_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .survival_probability_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default probability from the reference date to year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn default_probability(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_probability(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default probability from the reference date to `date`.
    #[pyo3(signature = (date, extrapolate = false))]
    fn default_probability_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_probability_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default density at year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn default_density(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_density(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default density at `date`.
    #[pyo3(signature = (date, extrapolate = false))]
    fn default_density_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_density_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The hazard rate at year-fraction `t`, annual frequency and continuous
    /// compounding.
    #[pyo3(signature = (t, extrapolate = false))]
    fn hazard_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .hazard_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The hazard rate at `date`, annual frequency and continuous compounding.
    #[pyo3(signature = (date, extrapolate = false))]
    fn hazard_rate_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .hazard_rate_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }
}

impl PyDefaultProbabilityTermStructure {
    /// The base half of a concrete curve's [`PyClassInitializer`] chain.
    pub(crate) fn from_handle(inner: Handle<dyn DefaultProbabilityTermStructure>) -> Self {
        PyDefaultProbabilityTermStructure { inner }
    }

    /// A clone of the inner curve handle for the CDS instrument and engine
    /// facades that take a credit curve.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn DefaultProbabilityTermStructure> {
        self.inner.clone()
    }
}

/// Python `FlatHazardRate`: a credit curve quoting one hazard rate for every
/// maturity, with the closed-form survival probability `exp(-h t)`
/// (`termstructures::credit::flathazardrate::FlatHazardRate`).
///
/// Extends [`PyDefaultProbabilityTermStructure`], which carries the whole query
/// surface. All four core constructors are infallible, so `__init__` hands back
/// the initializer directly and the three alternates are staticmethods
/// returning the built object, the shape [`PyFraRateHelper`](crate::helpers)
/// already uses. The quote-backed forms retain the caller's [`PySimpleQuote`],
/// so a later `set_value` moves the curve; the rate-backed forms wrap the value
/// in a fresh, un-retained quote. The `moving` forms take an explicit
/// [`PySettings`] (D5) and fix the reference date `settlement_days` business
/// days past the evaluation date, so a query before one is set returns
/// [`struct@crate::ItofinError`] rather than falling back to a system clock.
#[pyclass(name = "FlatHazardRate", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyFlatHazardRate;

#[pymethods]
impl PyFlatHazardRate {
    /// A curve reading `hazard_rate` live, with a fixed `reference_date`.
    #[new]
    fn new(
        reference_date: &PyDate,
        hazard_rate: &PySimpleQuote,
        day_counter: &PyDayCounter,
    ) -> PyClassInitializer<Self> {
        let curve = shared(FlatHazardRate::new(
            reference_date.inner(),
            hazard_rate.handle(),
            day_counter.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
            curve,
        )))
        .add_subclass(PyFlatHazardRate)
    }

    /// A curve at a fixed `rate`, with a fixed `reference_date`.
    #[staticmethod]
    fn with_rate(
        py: Python<'_>,
        reference_date: &PyDate,
        rate: f64,
        day_counter: &PyDayCounter,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatHazardRate::with_rate(
            reference_date.inner(),
            rate,
            day_counter.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(PyFlatHazardRate),
        )
    }

    /// A curve reading `hazard_rate` live, whose reference date moves off the
    /// evaluation date held by `settings`.
    #[staticmethod]
    fn moving(
        py: Python<'_>,
        settlement_days: Natural,
        calendar: &PyCalendar,
        hazard_rate: &PySimpleQuote,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatHazardRate::moving(
            settlement_days,
            calendar.inner(),
            hazard_rate.handle(),
            day_counter.inner(),
            settings.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(PyFlatHazardRate),
        )
    }

    /// A curve at a fixed `rate`, whose reference date moves off the evaluation
    /// date held by `settings`.
    #[staticmethod]
    fn moving_with_rate(
        py: Python<'_>,
        settlement_days: Natural,
        calendar: &PyCalendar,
        rate: f64,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatHazardRate::moving_with_rate(
            settlement_days,
            calendar.inner(),
            rate,
            day_counter.inner(),
            settings.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(PyFlatHazardRate),
        )
    }
}
