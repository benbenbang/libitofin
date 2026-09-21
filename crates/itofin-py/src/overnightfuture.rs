//! Engine-free overnight futures and bootstrap helpers.

use crate::PyQlError;
use crate::helpers::{PyOvernightIndex, PyPillar, PyRateAveraging, PyRateHelper};
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{PyDate, PyFrequency};
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::OvernightIndexFuture;
use libitofin::termstructures::yields::{OvernightIndexFutureRateHelper, SofrFutureRateHelper};
use libitofin::time::date::Month;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// An overnight futures price, with live index fixings and convexity adjustment.
#[gen_stub_pyclass]
#[pyclass(
    name = "OvernightIndexFuture",
    unsendable,
    module = "itofin.instruments"
)]
pub struct PyOvernightIndexFuture {
    inner: OvernightIndexFuture,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyOvernightIndexFuture {
    /// Construct a fixed reference-period future; Compound is the default.
    #[new]
    #[pyo3(signature = (index, value_date, maturity_date, convexity_adjustment = None, averaging_method = PyRateAveraging::Compound))]
    fn new(
        index: &PyOvernightIndex,
        value_date: &PyDate,
        maturity_date: &PyDate,
        convexity_adjustment: Option<&PySimpleQuote>,
        averaging_method: PyRateAveraging,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: OvernightIndexFuture::new(
                index.inner(),
                value_date.inner(),
                maturity_date.inner(),
                convexity_adjustment.map_or_else(Handle::empty, PySimpleQuote::handle),
                averaging_method.inner(),
            )
            .map_err(PyQlError::from)?,
        })
    }

    /// Return the futures price, or zero after expiry.
    fn npv(&mut self) -> PyResult<f64> {
        self.inner.npv().map_err(|e| PyQlError::from(e).into())
    }
    /// Return the current convexity adjustment.
    fn convexity_adjustment(&self) -> PyResult<f64> {
        self.inner
            .convexity_adjustment()
            .map_err(|e| PyQlError::from(e).into())
    }
    /// First accrual date.
    fn value_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.value_date())
    }
    /// Exclusive end of the reference period.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.maturity_date())
    }
    /// Whether the settlement event has occurred under the index settings.
    fn is_expired(&self) -> PyResult<bool> {
        self.inner
            .is_expired()
            .map_err(|e| PyQlError::from(e).into())
    }
}

/// Bootstrap a quoted overnight futures price over explicit dates.
#[gen_stub_pyclass]
#[pyclass(name = "OvernightIndexFutureRateHelper", extends = PyRateHelper, unsendable, module = "itofin.termstructures")]
pub struct PyOvernightIndexFutureRateHelper;

#[gen_stub_pymethods]
#[pymethods]
impl PyOvernightIndexFutureRateHelper {
    /// Construct a helper retaining the index history, price and convexity quote.
    #[gen_stub(override_return_type(type_repr = "OvernightIndexFutureRateHelper"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (price, value_date, maturity_date, index, convexity_adjustment = None, averaging_method = PyRateAveraging::Compound, pillar = PyPillar::LastRelevantDate, custom_pillar_date = None))]
    fn new(
        price: &PySimpleQuote,
        value_date: &PyDate,
        maturity_date: &PyDate,
        index: &PyOvernightIndex,
        convexity_adjustment: Option<&PySimpleQuote>,
        averaging_method: PyRateAveraging,
        pillar: PyPillar,
        custom_pillar_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let helper = OvernightIndexFutureRateHelper::new(
            price.handle(),
            value_date.inner(),
            maturity_date.inner(),
            &index.inner(),
            convexity_adjustment.map_or_else(Handle::empty, PySimpleQuote::handle),
            averaging_method.inner(),
            pillar.with_custom(custom_pillar_date)?,
        )
        .map_err(PyQlError::from)?;
        Ok(PyClassInitializer::from(PyRateHelper::from_inner(helper)).add_subclass(Self))
    }
}

/// Monthly simple-average and quarterly compounded CME SOFR futures helpers.
#[gen_stub_pyclass]
#[pyclass(name = "SofrFutureRateHelper", extends = PyRateHelper, unsendable, module = "itofin.termstructures")]
pub struct PySofrFutureRateHelper;

#[gen_stub_pymethods]
#[pymethods]
impl PySofrFutureRateHelper {
    /// Construct a SOFR helper using the supplied settings and shared SOFR history.
    #[gen_stub(override_return_type(type_repr = "SofrFutureRateHelper"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (price, reference_month, reference_year, reference_frequency, settings, convexity_adjustment = None, pillar = PyPillar::LastRelevantDate, custom_pillar_date = None))]
    fn new(
        price: &PySimpleQuote,
        reference_month: u32,
        reference_year: i32,
        reference_frequency: &PyFrequency,
        settings: &PySettings,
        convexity_adjustment: Option<&PySimpleQuote>,
        pillar: PyPillar,
        custom_pillar_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        if !(1..=12).contains(&reference_month) {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "month outside 1-12",
            ));
        }
        let month = Month::from_ordinal(reference_month as i32);
        let helper = SofrFutureRateHelper::new(
            price.handle(),
            month,
            reference_year,
            reference_frequency.inner(),
            convexity_adjustment.map_or_else(Handle::empty, PySimpleQuote::handle),
            pillar.with_custom(custom_pillar_date)?,
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(PyClassInitializer::from(PyRateHelper::from_inner(helper)).add_subclass(Self))
    }
}

/// The SOFR overnight index, retaining its forecast curve and shared history.
#[gen_stub_pyclass]
#[pyclass(name = "Sofr", extends = PyOvernightIndex, unsendable, module = "itofin.indexes")]
pub struct PySofr;

#[gen_stub_pymethods]
#[pymethods]
impl PySofr {
    /// Construct SOFR with an optional forecast curve.
    #[gen_stub(override_return_type(type_repr = "Sofr"))]
    #[new]
    #[pyo3(signature = (curve, settings))]
    fn new(
        curve: Option<&crate::curve::PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyClassInitializer<Self> {
        let index = libitofin::shared::shared(libitofin::indexes::ibor::Sofr::new(
            curve.map_or_else(Handle::empty, crate::curve::PyYieldTermStructure::handle),
            settings.inner(),
        ));
        PyClassInitializer::from(PyOvernightIndex::from_inner(index)).add_subclass(Self)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyOvernightIndex {
    /// Read a historical fixing or forecast through this index's curve.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        use libitofin::indexes::Index;
        self.inner()
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(|e| PyQlError::from(e).into())
    }

    /// Add a finite historical fixing shared by indices with this name/settings.
    fn add_fixing(&self, date: &PyDate, value: f64) -> PyResult<()> {
        use libitofin::indexes::Index;
        if !value.is_finite() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "fixing must be finite",
            ));
        }
        self.inner()
            .add_fixing(date.inner(), value)
            .map_err(|e| PyQlError::from(e).into())
    }
}
