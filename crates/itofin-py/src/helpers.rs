//! Facades for the bootstrap rate helpers: the [`PyRateHelper`] base and the
//! concrete [`PyDepositRateHelper`] and [`PySwapRateHelper`] instruments.
//!
//! A rate helper wraps a market quote plus the schedule of a single instrument;
//! a piecewise curve is bootstrapped so every helper reprices its own quote.
//! The base holds the already-upcast `Shared<dyn RateHelper>` (the four
//! inspectors are all `&self`, so no interior mutability is needed here) and
//! the concrete subclasses supply only their constructors, mirroring the
//! [`crate::curve::PyYieldTermStructure`] base/subclass idiom.
//!
//! The `Futures`, `Fra`, `OIS`, and `Bond`/`FixedRateBond` helpers each need
//! enum or index facades that do not exist yet and are deferred to their own
//! follow-up ticket (#530); they are omitted here rather than stubbed.

use crate::PyQlError;
use crate::hullwhite::PyEuribor;
use crate::market::PySimpleQuote;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod,
};
use libitofin::shared::Shared;
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::{DepositRateHelper, SwapRateHelper};
use pyo3::prelude::*;

/// Python `RateHelper`: the shared base for every bootstrap helper
/// (`termstructures::bootstraphelper::RateHelper`).
///
/// Holds the erased `Shared<dyn RateHelper>` and exposes the inspectors the
/// bootstrap and its oracles read: the curve-implied quote and its error
/// (fallible, needing a linked curve), the maturity and pillar dates
/// (infallible), and the fitted market quote's current value. Concrete helpers
/// such as [`PyDepositRateHelper`] subclass this and supply only their
/// constructor.
#[pyclass(name = "RateHelper", subclass, unsendable)]
pub struct PyRateHelper {
    inner: Shared<dyn RateHelper>,
}

#[pymethods]
impl PyRateHelper {
    /// The quote implied by the curve the helper is linked to. Fallible: with
    /// no curve set (the pre-bootstrap state) there is nothing to imply from.
    fn implied_quote(&self) -> PyResult<f64> {
        Ok(self.inner.implied_quote().map_err(PyQlError::from)?)
    }

    /// The bootstrap root the solver drives to zero: market quote minus implied
    /// quote. Fallible for the same reason as [`Self::implied_quote`].
    fn quote_error(&self) -> PyResult<f64> {
        Ok(self.inner.quote_error().map_err(PyQlError::from)?)
    }

    /// The current value of the market quote the helper fits. Reads back through
    /// the retained quote handle, so a `set_value` on the `SimpleQuote` passed
    /// to the constructor is observed here (the same-object wiring the laziness
    /// contract relies on). Fallible: the quote handle may be empty.
    fn quote_value(&self) -> PyResult<f64> {
        Ok(self.inner.base().quote_value().map_err(PyQlError::from)?)
    }

    /// The instrument's maturity date.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.maturity_date())
    }

    /// The pillar date, at which the curve node this helper sets sits.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }
}

impl PyRateHelper {
    /// A clone of the upcast helper, for the piecewise-curve facade (T5), which
    /// takes a list of helpers and threads each into the bootstrap.
    #[allow(dead_code)]
    pub(crate) fn inner(&self) -> Shared<dyn RateHelper> {
        Shared::clone(&self.inner)
    }
}

/// Python `DepositRateHelper`: a helper fitting a deposit rate
/// (`termstructures::yields::ratehelpers::DepositRateHelper`).
///
/// The quote-form constructor retains the caller's [`PySimpleQuote`] so a later
/// `set_value` re-drives the bootstrap; `from_rate` is a convenience that wraps
/// a fixed rate in a fresh, un-retained quote.
#[pyclass(name = "DepositRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyDepositRateHelper;

#[pymethods]
impl PyDepositRateHelper {
    /// A deposit helper fitting `quote`, whose schedule comes from `index`. The
    /// caller keeps `quote`; mutating it later invalidates the bootstrap.
    #[new]
    fn new(quote: &PySimpleQuote, index: &PyEuribor) -> PyClassInitializer<Self> {
        let idx = index.inner();
        let helper = DepositRateHelper::new(quote.handle(), &idx) as Shared<dyn RateHelper>;
        PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyDepositRateHelper)
    }

    /// A deposit helper fitting a fixed `rate`, wrapped in an internal quote the
    /// caller cannot later mutate.
    #[staticmethod]
    fn from_rate(py: Python<'_>, rate: f64, index: &PyEuribor) -> PyResult<Py<Self>> {
        let idx = index.inner();
        let helper = DepositRateHelper::from_rate(rate, &idx) as Shared<dyn RateHelper>;
        Py::new(
            py,
            PyClassInitializer::from(PyRateHelper { inner: helper })
                .add_subclass(PyDepositRateHelper),
        )
    }
}

/// Python `SwapRateHelper`: a helper fitting a par swap rate
/// (`termstructures::yields::ratehelpers::SwapRateHelper`).
///
/// The spot-starting form the curve-consistency oracle builds: no spread, no
/// forward start, no exogenous discounting curve, and the default
/// `Pillar::LastRelevantDate`.
#[pyclass(name = "SwapRateHelper", extends = PyRateHelper, unsendable)]
pub struct PySwapRateHelper;

#[pymethods]
impl PySwapRateHelper {
    /// A swap helper fitting `quote` with the schedule of a spot-starting swap
    /// of `tenor`, its fixed leg built from the given frequency, convention, and
    /// day count, floating off `ibor_index`.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: &PySimpleQuote,
        tenor: &PyPeriod,
        calendar: &PyCalendar,
        fixed_frequency: &PyFrequency,
        fixed_convention: &PyBusinessDayConvention,
        fixed_day_count: &PyDayCounter,
        ibor_index: &PyEuribor,
    ) -> PyClassInitializer<Self> {
        let idx = ibor_index.inner();
        let helper = SwapRateHelper::new(
            quote.handle(),
            tenor.inner(),
            calendar.inner(),
            fixed_frequency.inner(),
            fixed_convention.inner(),
            fixed_day_count.inner(),
            &idx,
        ) as Shared<dyn RateHelper>;
        PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PySwapRateHelper)
    }
}
