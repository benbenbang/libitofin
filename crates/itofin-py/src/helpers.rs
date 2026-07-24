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
use libitofin::handle::Handle;
use libitofin::instruments::FuturesType;
use libitofin::quotes::Quote;
use libitofin::shared::Shared;
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::{DepositRateHelper, FuturesRateHelper, SwapRateHelper};
use libitofin::types::Natural;
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

    /// The earliest date the helper needs curve data at.
    fn earliest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.earliest_date())
    }

    /// The latest date the helper needs curve data at (equal to the pillar date).
    fn latest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_date())
    }

    /// The latest date whose data the helper is relevant for.
    fn latest_relevant_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_relevant_date())
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

/// Python `FuturesType`: the date convention an interest-rate future settles on
/// (`instruments::FuturesType`).
///
/// A fieldless pyo3 enum exposing `Imm`, `Asx` and `Custom`. `Imm` and `Custom`
/// are fully usable from Python; `Asx` validates and prices against an explicitly
/// supplied ASX start date, but the ASX date navigators (`is_asx_date`/
/// `next_asx_date`, the analogues of the faced IMM functions) are deferred, so
/// there is no helper to derive the next ASX date from Python yet.
#[pyclass(name = "FuturesType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyFuturesType {
    Imm,
    Asx,
    Custom,
}

impl PyFuturesType {
    /// The core [`FuturesType`] this variant stands for.
    pub(crate) fn inner(&self) -> FuturesType {
        match self {
            PyFuturesType::Imm => FuturesType::Imm,
            PyFuturesType::Asx => FuturesType::Asx,
            PyFuturesType::Custom => FuturesType::Custom,
        }
    }
}

/// Python `FuturesRateHelper`: a helper fitting an exchange-traded interest-rate
/// future's quoted price at a fixed IMM/ASX window
/// (`termstructures::yields::ratehelpers::FuturesRateHelper`).
///
/// Unlike the deposit and swap helpers the window is absolute: it is computed
/// once from the supplied dates and never rebuilt on an evaluation-date change.
/// The convexity adjustment is usually absent; pass `None` to leave it empty
/// (an empty handle reports a zero adjustment). The subclass retains the concrete
/// `Shared<FuturesRateHelper>` so [`Self::convexity_adjustment`], which is not on
/// the [`RateHelper`] trait, stays reachable.
#[pyclass(name = "FuturesRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyFuturesRateHelper {
    futures: Shared<FuturesRateHelper>,
}

#[pymethods]
impl PyFuturesRateHelper {
    /// A futures helper over a length-in-months window off `ibor_start_date`: the
    /// maturity is the start advanced `length_in_months` months on `calendar`
    /// under `convention`/`end_of_month`. `conv_adj` is the convexity quote, or
    /// `None` for an empty (zero) adjustment. Fallible: an `Imm`/`Asx` start that
    /// is not a valid date of that convention is rejected.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        price,
        ibor_start_date,
        length_in_months,
        calendar,
        convention,
        end_of_month,
        day_counter,
        conv_adj,
        futures_type,
    ))]
    fn new(
        price: &PySimpleQuote,
        ibor_start_date: &PyDate,
        length_in_months: Natural,
        calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
        end_of_month: bool,
        day_counter: &PyDayCounter,
        conv_adj: Option<&PySimpleQuote>,
        futures_type: &PyFuturesType,
    ) -> PyResult<PyClassInitializer<Self>> {
        let helper = FuturesRateHelper::new(
            price.handle(),
            ibor_start_date.inner(),
            length_in_months,
            calendar.inner(),
            convention.inner(),
            end_of_month,
            day_counter.inner(),
            empty_or_handle(conv_adj),
            futures_type.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(init(helper))
    }

    /// A futures helper over an explicit window. With `ibor_end_date` `None` the
    /// maturity is three IMM/ASX periods past the start; with a date, that date
    /// (which must be past the start). Divergence from C++: a `Custom` helper with
    /// no end date is an error here, not a null-maturity helper. Fallible for that
    /// case and for a start that is not a valid date of the chosen convention.
    #[staticmethod]
    #[pyo3(signature = (
        price,
        ibor_start_date,
        ibor_end_date,
        day_counter,
        conv_adj,
        futures_type,
    ))]
    fn from_end_date(
        py: Python<'_>,
        price: &PySimpleQuote,
        ibor_start_date: &PyDate,
        ibor_end_date: Option<&PyDate>,
        day_counter: &PyDayCounter,
        conv_adj: Option<&PySimpleQuote>,
        futures_type: &PyFuturesType,
    ) -> PyResult<Py<Self>> {
        let helper = FuturesRateHelper::from_end_date(
            price.handle(),
            ibor_start_date.inner(),
            ibor_end_date.map(PyDate::inner),
            day_counter.inner(),
            empty_or_handle(conv_adj),
            futures_type.inner(),
        )
        .map_err(PyQlError::from)?;
        Py::new(py, init(helper))
    }

    /// A futures helper whose window follows `index`'s conventions: the maturity
    /// is the start advanced by the index tenor on the index's fixing calendar,
    /// and the year fraction uses the index day counter. Fallible for a start that
    /// is not a valid date of the chosen convention.
    #[staticmethod]
    #[pyo3(signature = (price, ibor_start_date, index, conv_adj, futures_type))]
    fn from_index(
        py: Python<'_>,
        price: &PySimpleQuote,
        ibor_start_date: &PyDate,
        index: &PyEuribor,
        conv_adj: Option<&PySimpleQuote>,
        futures_type: &PyFuturesType,
    ) -> PyResult<Py<Self>> {
        let idx = index.inner();
        let helper = FuturesRateHelper::from_index(
            price.handle(),
            ibor_start_date.inner(),
            &idx,
            empty_or_handle(conv_adj),
            futures_type.inner(),
        )
        .map_err(PyQlError::from)?;
        Py::new(py, init(helper))
    }

    /// The convexity adjustment applied to the forward: the convexity quote's
    /// value, or zero when none was supplied. The quantity the convexity oracle
    /// pins.
    fn convexity_adjustment(&self) -> PyResult<f64> {
        Ok(self
            .futures
            .convexity_adjustment()
            .map_err(PyQlError::from)?)
    }
}

/// The convexity handle for a futures helper: the caller's quote, or an empty
/// handle when `None`. `PySimpleQuote::handle` is never empty, so the empty case
/// (the zero-adjustment default the core tests pass) must be built here.
fn empty_or_handle(conv_adj: Option<&PySimpleQuote>) -> Handle<dyn Quote> {
    match conv_adj {
        Some(quote) => quote.handle(),
        None => Handle::empty(),
    }
}

/// The base/subclass initializer shared by the three constructors: the erased
/// upcast helper feeds the [`PyRateHelper`] base, and the concrete clone is
/// retained on the subclass for [`PyFuturesRateHelper::convexity_adjustment`].
fn init(helper: Shared<FuturesRateHelper>) -> PyClassInitializer<PyFuturesRateHelper> {
    let base = PyRateHelper {
        inner: Shared::clone(&helper) as Shared<dyn RateHelper>,
    };
    PyClassInitializer::from(base).add_subclass(PyFuturesRateHelper { futures: helper })
}
