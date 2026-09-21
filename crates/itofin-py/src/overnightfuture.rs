//! Engine-free overnight futures and bootstrap helpers.

use crate::PyQlError;
use crate::helpers::{PyOvernightIndex, PyRateAveraging};
use crate::market::PySimpleQuote;
use crate::time::PyDate;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::OvernightIndexFuture;
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
