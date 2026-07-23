//! Facade for the reusable flat yield curve: [`PyFlatForward`].

use crate::PyQlError;
use crate::time::{PyDate, PyDayCounter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;

/// Python `FlatForward`: a flat continuously-compounded yield curve behind a
/// [`Handle`] (`termstructures::yields::FlatForward`).
///
/// Built with `Compounding::Continuous` and `Frequency::Annual` - the
/// convention every downstream Heston/Hull-White oracle assumes. The `Handle`
/// is assembled internally so it never crosses the PyO3 boundary; the pricing
/// facades (H1/W1) take a clone of it through the crate-internal accessor.
#[pyclass(name = "FlatForward", unsendable)]
pub struct PyFlatForward {
    inner: Handle<dyn YieldTermStructure>,
}

#[pymethods]
impl PyFlatForward {
    #[new]
    fn new(reference_date: &PyDate, rate: f64, day_counter: &PyDayCounter) -> Self {
        let curve = shared(FlatForward::with_rate(
            reference_date.inner(),
            rate,
            day_counter.inner(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>;
        PyFlatForward {
            inner: Handle::new(curve),
        }
    }

    /// The discount factor at year-fraction `t`.
    fn discount(&self, t: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .discount(t, true)
            .map_err(PyQlError::from)?)
    }

    /// The continuously-compounded zero rate at year-fraction `t`, read back
    /// with the convention the curve was built with.
    fn zero_rate(&self, t: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .zero_rate(t, Compounding::Continuous, Frequency::Annual, true)
            .map_err(PyQlError::from)?
            .rate())
    }
}

impl PyFlatForward {
    /// A clone of the inner curve handle for the process/model ctors (H1/W1).
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn YieldTermStructure> {
        self.inner.clone()
    }
}
