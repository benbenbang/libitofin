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
//! The `Bond`/`FixedRateBond` helpers still need instrument facades that do not
//! exist yet and are deferred to their own follow-up ticket (#530); they are
//! omitted here rather than stubbed. The `OIS` helper, with its [`PyEstr`]
//! overnight index and [`PyRateAveraging`] convention, lands in this module
//! (#551).

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyEuribor;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod,
};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::{Estr, Index, OvernightIndex};
use libitofin::instruments::FuturesType;
use libitofin::quotes::Quote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::{
    DepositRateHelper, FraRateHelper, FuturesRateHelper, OISRateHelper, Pillar, SwapRateHelper,
};
use libitofin::types::{Integer, Natural};
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

/// Python `Pillar`: the date the curve node a helper fits sits at
/// (`termstructures::yields::Pillar`).
///
/// A fieldless pyo3 enum exposing the two schedule-derived choices `MaturityDate`
/// and `LastRelevantDate` (the C++ default). `Pillar::CustomDate` is deferred in
/// the core (#343) - it needs an explicit pillar date threaded through
/// construction plus its bounds check - so its omission here is deliberate, not
/// an oversight.
#[pyclass(name = "Pillar", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyPillar {
    MaturityDate,
    LastRelevantDate,
}

impl PyPillar {
    /// The core [`Pillar`] this variant stands for.
    pub(crate) fn inner(&self) -> Pillar {
        match self {
            PyPillar::MaturityDate => Pillar::MaturityDate,
            PyPillar::LastRelevantDate => Pillar::LastRelevantDate,
        }
    }
}

/// Python `FraRateHelper`: a helper fitting a forward-rate-agreement rate over the
/// window starting `period_to_start` after spot and spanning the index tenor
/// (`termstructures::yields::ratehelpers::FraRateHelper`).
///
/// All four constructors are infallible, so `__init__` and the staticmethods hand
/// back the initializer directly. `use_indexed_coupon` selects the implied-quote
/// mode (C++ default `True`, the index fixing forecast off the curve; `False` is
/// the par simple-forward over the raw window); it and `pillar` (default
/// `Pillar.LastRelevantDate`) are exposed explicitly. Like the other relative
/// helpers the quote-form constructors retain the caller's [`PySimpleQuote`] so a
/// later `set_value` re-drives the bootstrap; `from_rate` wraps a fixed rate in a
/// fresh, un-retained quote. `from_dates` fixes the window at construction, and it
/// does not shift when the evaluation date changes.
#[pyclass(name = "FraRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyFraRateHelper;

#[pymethods]
impl PyFraRateHelper {
    /// A FRA helper fitting `quote` over the window `period_to_start` past spot
    /// spanning `index`'s tenor. The constructor the mixed strip uses.
    #[new]
    #[pyo3(signature = (
        quote,
        period_to_start,
        index,
        use_indexed_coupon = true,
        pillar = PyPillar::LastRelevantDate,
    ))]
    fn new(
        quote: &PySimpleQuote,
        period_to_start: &PyPeriod,
        index: &PyEuribor,
        use_indexed_coupon: bool,
        pillar: PyPillar,
    ) -> PyClassInitializer<Self> {
        let idx = index.inner();
        let helper = FraRateHelper::new(
            quote.handle(),
            period_to_start.inner(),
            &idx,
            use_indexed_coupon,
            pillar.inner(),
        ) as Shared<dyn RateHelper>;
        PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyFraRateHelper)
    }

    /// A FRA helper fitting a fixed `rate`, wrapped in an internal quote the caller
    /// cannot later mutate.
    #[staticmethod]
    #[pyo3(signature = (
        rate,
        period_to_start,
        index,
        use_indexed_coupon = true,
        pillar = PyPillar::LastRelevantDate,
    ))]
    fn from_rate(
        py: Python<'_>,
        rate: f64,
        period_to_start: &PyPeriod,
        index: &PyEuribor,
        use_indexed_coupon: bool,
        pillar: PyPillar,
    ) -> PyResult<Py<Self>> {
        let idx = index.inner();
        let helper = FraRateHelper::from_rate(
            rate,
            period_to_start.inner(),
            &idx,
            use_indexed_coupon,
            pillar.inner(),
        ) as Shared<dyn RateHelper>;
        Py::new(
            py,
            PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyFraRateHelper),
        )
    }

    /// A FRA helper whose start is `months_to_start` months after spot.
    #[staticmethod]
    #[pyo3(signature = (
        quote,
        months_to_start,
        index,
        use_indexed_coupon = true,
        pillar = PyPillar::LastRelevantDate,
    ))]
    fn from_months(
        py: Python<'_>,
        quote: &PySimpleQuote,
        months_to_start: Natural,
        index: &PyEuribor,
        use_indexed_coupon: bool,
        pillar: PyPillar,
    ) -> PyResult<Py<Self>> {
        let idx = index.inner();
        let helper = FraRateHelper::from_months(
            quote.handle(),
            months_to_start,
            &idx,
            use_indexed_coupon,
            pillar.inner(),
        ) as Shared<dyn RateHelper>;
        Py::new(
            py,
            PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyFraRateHelper),
        )
    }

    /// A FRA helper over the explicit `[start_date, end_date]` window. Its schedule
    /// is fixed at construction and does not shift on an evaluation-date change.
    #[staticmethod]
    #[pyo3(signature = (
        quote,
        start_date,
        end_date,
        index,
        use_indexed_coupon = true,
        pillar = PyPillar::LastRelevantDate,
    ))]
    fn from_dates(
        py: Python<'_>,
        quote: &PySimpleQuote,
        start_date: &PyDate,
        end_date: &PyDate,
        index: &PyEuribor,
        use_indexed_coupon: bool,
        pillar: PyPillar,
    ) -> PyResult<Py<Self>> {
        let idx = index.inner();
        let helper = FraRateHelper::from_dates(
            quote.handle(),
            start_date.inner(),
            end_date.inner(),
            &idx,
            use_indexed_coupon,
            pillar.inner(),
        ) as Shared<dyn RateHelper>;
        Py::new(
            py,
            PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyFraRateHelper),
        )
    }
}

/// Python `Estr`: the Euro Short-Term Rate overnight index (`indexes::Estr`).
///
/// `Estr::new` returns an `OvernightIndex` by value (it adds no behaviour over
/// the base, so the C++ subclass is pure configuration, `estr.rs:7`); it is
/// wrapped in `shared()` so [`PyOISRateHelper`] can hold the same object and
/// hand the core ctor the `&OvernightIndex` it takes. Passing `None` for the
/// curve builds the index over an empty forwarding handle, the form the OIS
/// bootstrap needs. Infallible (unlike `Euribor::new`, which rejects daily
/// tenors): the overnight tenor is fixed to `1*Days` by the base.
#[pyclass(name = "Estr", unsendable)]
pub struct PyEstr {
    inner: Shared<OvernightIndex>,
}

#[pymethods]
impl PyEstr {
    /// An ESTR index forwarding off `curve`, or off an empty handle when `curve`
    /// is `None`.
    #[new]
    #[pyo3(signature = (curve, settings))]
    fn new(curve: Option<&PyYieldTermStructure>, settings: &PySettings) -> Self {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        PyEstr {
            inner: shared(Estr::new(forwarding, settings.inner())),
        }
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

impl PyEstr {
    /// A clone of the inner overnight index for the OIS helper facade, which
    /// takes a `&OvernightIndex`.
    pub(crate) fn inner(&self) -> Shared<OvernightIndex> {
        Shared::clone(&self.inner)
    }
}

/// Python `RateAveraging`: how an overnight coupon combines its daily fixings
/// (`cashflows::RateAveraging`).
///
/// A fieldless pyo3 enum exposing `Simple` (arithmetic average) and `Compound`
/// (daily compounding, the coupon default the OIS oracle uses).
#[pyclass(name = "RateAveraging", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyRateAveraging {
    Simple,
    Compound,
}

impl PyRateAveraging {
    /// The core [`RateAveraging`] this variant stands for.
    pub(crate) fn inner(&self) -> RateAveraging {
        match self {
            PyRateAveraging::Simple => RateAveraging::Simple,
            PyRateAveraging::Compound => RateAveraging::Compound,
        }
    }
}

/// Python `OISRateHelper`: a helper fitting an overnight-indexed swap rate
/// (`termstructures::yields::ratehelpers::OISRateHelper`).
///
/// The spot-starting OIS the bootstrap oracle builds: a swap of `tenor` starting
/// `settlement_days` after the evaluation date, floating off `overnight_index`.
/// The Python signature lists the required knobs first (so `settings` can sit
/// among them rather than illegally after the defaulted trailing ones), then the
/// four optional knobs the issue defaults; the body reorders these into the core
/// ctor's 13-argument positional order. `discounting_curve` `None` discounts off
/// the bootstrapping curve; `overnight_spread` `None` becomes an empty (zero)
/// spread handle. The deferred core knobs past `averaging_method` (telescopic
/// value dates, lookback, lockout, observation shift, custom pillar, per-leg
/// calendars, `ratehelpers.rs:1036-1039`) take their benign defaults.
#[pyclass(name = "OISRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyOISRateHelper;

#[pymethods]
impl PyOISRateHelper {
    /// An OIS helper fitting `quote` with the schedule of a spot-starting OIS of
    /// `tenor` floating off `overnight_index`. The caller keeps `quote` (and
    /// `overnight_spread`); mutating either later re-drives the bootstrap.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        settlement_days,
        tenor,
        quote,
        overnight_index,
        payment_lag,
        payment_convention,
        payment_frequency,
        forward_start,
        settings,
        discounting_curve = None,
        overnight_spread = None,
        pillar = PyPillar::LastRelevantDate,
        averaging_method = PyRateAveraging::Compound,
    ))]
    fn new(
        settlement_days: Natural,
        tenor: &PyPeriod,
        quote: &PySimpleQuote,
        overnight_index: &PyEstr,
        payment_lag: Integer,
        payment_convention: &PyBusinessDayConvention,
        payment_frequency: &PyFrequency,
        forward_start: &PyPeriod,
        settings: &PySettings,
        discounting_curve: Option<&PyYieldTermStructure>,
        overnight_spread: Option<&PySimpleQuote>,
        pillar: PyPillar,
        averaging_method: PyRateAveraging,
    ) -> PyClassInitializer<Self> {
        let idx = overnight_index.inner();
        let helper = OISRateHelper::new(
            settlement_days,
            tenor.inner(),
            quote.handle(),
            &idx,
            discounting_curve.map(|curve| curve.handle()),
            payment_lag,
            payment_convention.inner(),
            payment_frequency.inner(),
            forward_start.inner(),
            overnight_spread
                .map(|spread| spread.handle())
                .unwrap_or_else(Handle::empty),
            pillar.inner(),
            averaging_method.inner(),
            settings.inner(),
        ) as Shared<dyn RateHelper>;
        PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyOISRateHelper)
    }
}
