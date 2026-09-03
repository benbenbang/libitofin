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
//! The `OIS` helper, with its [`PyEstr`] overnight index and
//! [`PyRateAveraging`] convention, lands in this module (#551), as does
//! [`PyFixedRateBondHelper`], which builds its own bond internally and so needs
//! no bond facade (#530). The generic `BondHelper`, over an arbitrary pre-built
//! bond, stays deferred: there is no bond-instrument facade to hand it one.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod, PySchedule,
};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::{Estr, Index, OvernightIndex};
use libitofin::instruments::{BondPriceType, FuturesType};
use libitofin::quotes::Quote;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::yields::{
    DepositRateHelper, FixedRateBondHelper, FraRateHelper, FuturesRateHelper, OISRateHelper,
    Pillar, SwapRateHelper,
};
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::types::{Integer, Natural, Real};
use pyo3::prelude::*;

/// Shared base for every bootstrap helper: implied/market quotes and dates.
///
/// A rate helper wraps a market quote plus the schedule of a single
/// instrument; a piecewise curve is bootstrapped so every helper reprices its
/// own quote. Concrete helpers subclass this and supply only their
/// constructor.
#[pyclass(name = "RateHelper", subclass, unsendable)]
pub struct PyRateHelper {
    inner: Shared<dyn RateHelper>,
}

#[pymethods]
impl PyRateHelper {
    /// Return the quote implied by the curve the helper is linked to.
    ///
    /// Returns:
    ///     float: The curve-implied quote.
    ///
    /// Raises:
    ///     ItofinError: With no curve set, the pre-bootstrap state, there
    ///         being nothing to imply from.
    fn implied_quote(&self) -> PyResult<f64> {
        Ok(self.inner.implied_quote().map_err(PyQlError::from)?)
    }

    /// Return the bootstrap root: market quote minus implied quote.
    ///
    /// Returns:
    ///     float: The residual the solver drives to zero.
    ///
    /// Raises:
    ///     ItofinError: On the same condition implied_quote reports.
    fn quote_error(&self) -> PyResult<f64> {
        Ok(self.inner.quote_error().map_err(PyQlError::from)?)
    }

    /// Return the current value of the market quote the helper fits.
    ///
    /// Reads back through the retained quote handle, so a set_value on the
    /// SimpleQuote passed to the constructor is observed here: the same-object
    /// wiring the laziness contract relies on.
    ///
    /// Returns:
    ///     float: The market quote's current value.
    fn quote_value(&self) -> PyResult<f64> {
        Ok(self.inner.base().quote_value().map_err(PyQlError::from)?)
    }

    /// Return the instrument's maturity date.
    ///
    /// Returns:
    ///     Date: The maturity.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.maturity_date())
    }

    /// Return the date the curve node this helper sets sits at.
    ///
    /// Returns:
    ///     Date: The pillar date.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// Return the earliest date the helper needs curve data at.
    ///
    /// Returns:
    ///     Date: The earliest relevant date.
    fn earliest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.earliest_date())
    }

    /// Return the latest date the helper needs curve data at.
    ///
    /// Returns:
    ///     Date: The latest date, equal to the pillar date.
    fn latest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_date())
    }

    /// Return the latest date whose data the helper is relevant for.
    ///
    /// Returns:
    ///     Date: The latest relevant date.
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

/// A helper fitting a deposit rate.
#[pyclass(name = "DepositRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyDepositRateHelper;

#[pymethods]
impl PyDepositRateHelper {
    /// Build the helper over a live quote.
    ///
    /// Args:
    ///     quote (SimpleQuote): The deposit rate; the caller keeps it, and
    ///         mutating it later invalidates the bootstrap.
    ///     index (IborIndex): The index supplying the deposit's schedule.
    #[new]
    fn new(quote: &PySimpleQuote, index: &PyIborIndex) -> PyClassInitializer<Self> {
        let idx = index.inner();
        let helper = DepositRateHelper::new(quote.handle(), &idx) as Shared<dyn RateHelper>;
        PyClassInitializer::from(PyRateHelper { inner: helper }).add_subclass(PyDepositRateHelper)
    }

    /// Build the helper over a fixed rate.
    ///
    /// Args:
    ///     rate (float): The deposit rate, wrapped in an internal quote the
    ///         caller cannot later mutate.
    ///     index (IborIndex): The index supplying the deposit's schedule.
    ///
    /// Returns:
    ///     DepositRateHelper: The helper fitting that rate.
    #[staticmethod]
    fn from_rate(py: Python<'_>, rate: f64, index: &PyIborIndex) -> PyResult<Py<Self>> {
        let idx = index.inner();
        let helper = DepositRateHelper::from_rate(rate, &idx) as Shared<dyn RateHelper>;
        Py::new(
            py,
            PyClassInitializer::from(PyRateHelper { inner: helper })
                .add_subclass(PyDepositRateHelper),
        )
    }
}

/// A helper fitting a par swap rate (spot-starting, no spread).
///
/// The spot-starting form the curve-consistency oracle builds: no spread, no
/// forward start, no exogenous discounting curve, and the default pillar.
#[pyclass(name = "SwapRateHelper", extends = PyRateHelper, unsendable)]
pub struct PySwapRateHelper;

#[pymethods]
impl PySwapRateHelper {
    /// Build the helper over the schedule of a spot-starting swap.
    ///
    /// Args:
    ///     quote (SimpleQuote): The par swap rate the helper fits.
    ///     tenor (Period): The length of the swap.
    ///     calendar (Calendar): The calendar the schedule rolls on.
    ///     fixed_frequency (Frequency): The fixed leg's payment frequency.
    ///     fixed_convention (BusinessDayConvention): The fixed leg's roll.
    ///     fixed_day_count (DayCounter): The fixed leg's day count.
    ///     ibor_index (IborIndex): The index the floating leg fixes off.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: &PySimpleQuote,
        tenor: &PyPeriod,
        calendar: &PyCalendar,
        fixed_frequency: &PyFrequency,
        fixed_convention: &PyBusinessDayConvention,
        fixed_day_count: &PyDayCounter,
        ibor_index: &PyIborIndex,
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

/// The date convention an interest-rate future settles on.
///
/// Imm and Custom are fully usable from Python. Asx validates and prices against
/// an explicitly supplied ASX start date, but the ASX date navigators (the
/// analogues of itofin.time.is_imm_date / next_imm_date) are deferred, so there
/// is no helper to derive the next ASX date from Python yet.
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

/// A helper fitting an exchange-traded interest-rate future's quoted price.
///
/// Unlike the deposit and swap helpers the window is absolute: it is computed
/// once from the supplied dates and never rebuilt on an evaluation-date
/// change. The convexity adjustment is usually absent; pass conv_adj=None to
/// leave it empty, which reports a zero adjustment.
#[pyclass(name = "FuturesRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyFuturesRateHelper {
    futures: Shared<FuturesRateHelper>,
}

#[pymethods]
impl PyFuturesRateHelper {
    /// Build the helper over a length-in-months window off the start date.
    ///
    /// Args:
    ///     price (SimpleQuote): The future's quoted price.
    ///     ibor_start_date (Date): The window's start.
    ///     length_in_months (int): The months the start is advanced by to
    ///         reach maturity.
    ///     calendar (Calendar): The calendar the maturity rolls on.
    ///     convention (BusinessDayConvention): The roll applied to the
    ///         maturity.
    ///     end_of_month (bool): Whether the maturity roll keeps to month ends.
    ///     day_counter (DayCounter): The day count the year fraction uses.
    ///     conv_adj (SimpleQuote | None): The convexity quote, or None for an
    ///         empty, zero adjustment.
    ///     futures_type (FuturesType): The date convention the future settles
    ///         on.
    ///
    /// Raises:
    ///     ItofinError: If an Imm or Asx start is not a valid date of that
    ///         convention.
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

    /// Build the helper over an explicit window.
    ///
    /// Args:
    ///     price (SimpleQuote): The future's quoted price.
    ///     ibor_start_date (Date): The window's start.
    ///     ibor_end_date (Date | None): The window's end, which must be past
    ///         the start; None puts the maturity three IMM/ASX periods past
    ///         the start.
    ///     day_counter (DayCounter): The day count the year fraction uses.
    ///     conv_adj (SimpleQuote | None): The convexity quote, or None for an
    ///         empty, zero adjustment.
    ///     futures_type (FuturesType): The date convention the future settles
    ///         on.
    ///
    /// Returns:
    ///     FuturesRateHelper: The helper over that window.
    ///
    /// Raises:
    ///     ItofinError: On a Custom helper with no end date - a divergence
    ///         from C++, which builds a null-maturity helper instead - and on
    ///         a start that is not a valid date of the chosen convention.
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

    /// Build the helper with a window following the index's conventions.
    ///
    /// The maturity is the start advanced by the index tenor on the index's
    /// fixing calendar, and the year fraction uses the index day counter.
    ///
    /// Args:
    ///     price (SimpleQuote): The future's quoted price.
    ///     ibor_start_date (Date): The window's start.
    ///     index (IborIndex): The index supplying the conventions.
    ///     conv_adj (SimpleQuote | None): The convexity quote, or None for an
    ///         empty, zero adjustment.
    ///     futures_type (FuturesType): The date convention the future settles
    ///         on.
    ///
    /// Returns:
    ///     FuturesRateHelper: The helper over that window.
    ///
    /// Raises:
    ///     ItofinError: If the start is not a valid date of the chosen
    ///         convention.
    #[staticmethod]
    #[pyo3(signature = (price, ibor_start_date, index, conv_adj, futures_type))]
    fn from_index(
        py: Python<'_>,
        price: &PySimpleQuote,
        ibor_start_date: &PyDate,
        index: &PyIborIndex,
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

    /// Return the convexity adjustment applied to the forward.
    ///
    /// Returns:
    ///     float: The convexity quote's value, or zero when none was supplied.
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

/// The date the curve node a helper fits sits at.
///
/// MaturityDate and LastRelevantDate (the default) are the two schedule-derived
/// choices. Pillar.CustomDate is deferred in the core (#343), so its omission
/// here is deliberate, not an oversight.
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

/// A helper fitting a forward-rate-agreement rate over the window starting
/// period_to_start after spot and spanning the index tenor. use_indexed_coupon
/// (default True) selects the indexed implied-quote mode; False is the par simple
/// forward. from_dates fixes the window at construction (it does not shift on an
/// evaluation-date change).
#[pyclass(name = "FraRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyFraRateHelper;

#[pymethods]
impl PyFraRateHelper {
    /// Build the helper over the window period_to_start past spot.
    ///
    /// Args:
    ///     quote (SimpleQuote): The FRA rate; the caller keeps it, so a later
    ///         set_value re-drives the bootstrap.
    ///     period_to_start (Period): How long after spot the window starts.
    ///     index (IborIndex): The index whose tenor the window spans.
    ///     use_indexed_coupon (bool): True selects the indexed implied-quote
    ///         mode, the index fixing forecast off the curve; False is the par
    ///         simple forward over the raw window.
    ///     pillar (Pillar): The date the curve node sits at; defaults to
    ///         LastRelevantDate.
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
        index: &PyIborIndex,
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

    /// Build the helper over a fixed rate.
    ///
    /// Args:
    ///     rate (float): The FRA rate, wrapped in an internal quote the caller
    ///         cannot later mutate.
    ///     period_to_start (Period): How long after spot the window starts.
    ///     index (IborIndex): The index whose tenor the window spans.
    ///     use_indexed_coupon (bool): The implied-quote mode; see __init__.
    ///     pillar (Pillar): The date the curve node sits at.
    ///
    /// Returns:
    ///     FraRateHelper: The helper fitting that rate.
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
        index: &PyIborIndex,
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

    /// Build the helper with a start given in months after spot.
    ///
    /// Args:
    ///     quote (SimpleQuote): The FRA rate the helper fits.
    ///     months_to_start (int): How many months after spot the window
    ///         starts.
    ///     index (IborIndex): The index whose tenor the window spans.
    ///     use_indexed_coupon (bool): The implied-quote mode; see __init__.
    ///     pillar (Pillar): The date the curve node sits at.
    ///
    /// Returns:
    ///     FraRateHelper: The helper over that window.
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
        index: &PyIborIndex,
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

    /// Build the helper over an explicit window.
    ///
    /// The schedule is fixed at construction and does not shift when the
    /// evaluation date changes.
    ///
    /// Args:
    ///     quote (SimpleQuote): The FRA rate the helper fits.
    ///     start_date (Date): The window's start.
    ///     end_date (Date): The window's end.
    ///     index (IborIndex): The index the forward is read off.
    ///     use_indexed_coupon (bool): The implied-quote mode; see __init__.
    ///     pillar (Pillar): The date the curve node sits at.
    ///
    /// Returns:
    ///     FraRateHelper: The helper over that window.
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
        index: &PyIborIndex,
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

/// The base of the overnight index families.
///
/// Abstract: it has no constructor, because the core builds an overnight index
/// only through a family factory such as Estr. It exists so OISRateHelper and
/// MakeOis name one type and accept any family. The fixing accessor stays on
/// the family facade; lifting it here is deferred.
#[pyclass(name = "OvernightIndex", subclass, unsendable)]
pub struct PyOvernightIndex {
    inner: Shared<OvernightIndex>,
}

impl PyOvernightIndex {
    /// A clone of the inner index for the facades that take an overnight index
    /// and are generic over the family.
    pub(crate) fn inner(&self) -> Shared<OvernightIndex> {
        Shared::clone(&self.inner)
    }
}

/// The Euro Short-Term Rate overnight index.
///
/// A subclass of OvernightIndex, so an ESTR index is accepted wherever the
/// general overnight index is. It retains its own clone of the index the base
/// holds - the same object, not a rebuild - so a facade typed on either half
/// reads exactly the same core index.
#[pyclass(name = "Estr", extends = PyOvernightIndex, unsendable)]
pub struct PyEstr {
    inner: Shared<OvernightIndex>,
}

#[pymethods]
impl PyEstr {
    /// Build an ESTR index forwarding off curve.
    ///
    /// Infallible, unlike the Euribor constructor: the overnight tenor is fixed
    /// to one day by the base rather than taken from the caller.
    ///
    /// Args:
    ///     curve (YieldTermStructure | None): The forwarding curve; None builds
    ///         the index over an empty forwarding handle, the form the OIS
    ///         bootstrap needs.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    #[new]
    #[pyo3(signature = (curve, settings))]
    fn new(
        curve: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyClassInitializer<Self> {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        init_overnight(shared(Estr::new(forwarding, settings.inner())))
    }

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The base/subclass initializer the ESTR constructor builds: one index object
/// feeds both halves, so the base [`PyOvernightIndex`] the OIS facades read and
/// the [`PyEstr`] its own `fixing` reads are the same core index.
fn init_overnight(index: Shared<OvernightIndex>) -> PyClassInitializer<PyEstr> {
    let base = PyOvernightIndex {
        inner: Shared::clone(&index),
    };
    PyClassInitializer::from(base).add_subclass(PyEstr { inner: index })
}

/// How an overnight coupon combines its daily fixings.
///
/// Simple is the arithmetic average; Compound (daily compounding) is the coupon
/// default the OIS conventions use.
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

/// A helper fitting an overnight-indexed swap rate (spot-starting, floating
/// off an overnight index).
///
/// The required knobs come first so settings can sit among them; the four
/// optional knobs trail with defaults. discounting_curve=None discounts off the
/// bootstrapping curve; overnight_spread=None is an empty (zero) spread. The
/// deferred core knobs past averaging_method (telescopic value dates, lookback,
/// lockout, observation shift, custom pillar, per-leg calendars) take benign
/// defaults.
#[pyclass(name = "OISRateHelper", extends = PyRateHelper, unsendable)]
pub struct PyOISRateHelper;

#[pymethods]
impl PyOISRateHelper {
    /// Build the helper over the schedule of a spot-starting OIS.
    ///
    /// Args:
    ///     settlement_days (int): The days after the evaluation date the swap
    ///         starts.
    ///     tenor (Period): The length of the swap.
    ///     quote (SimpleQuote): The OIS rate; the caller keeps it, so a later
    ///         set_value re-drives the bootstrap.
    ///     overnight_index (OvernightIndex): The index the floating leg
    ///         compounds.
    ///     payment_lag (int): The days between accrual end and payment.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         payment dates.
    ///     payment_frequency (Frequency): The payment frequency.
    ///     forward_start (Period): How long after spot the swap starts.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///     discounting_curve (YieldTermStructure | None): The curve the flows
    ///         discount on; None discounts off the bootstrapping curve.
    ///     overnight_spread (SimpleQuote | None): The spread over the index;
    ///         None leaves it empty, so zero. The caller keeps it, and
    ///         mutating it re-drives the bootstrap.
    ///     pillar (Pillar): The date the curve node sits at; defaults to
    ///         LastRelevantDate.
    ///     averaging_method (RateAveraging): How the daily fixings combine;
    ///         defaults to Compound.
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
        overnight_index: &PyOvernightIndex,
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

/// The price convention a bond helper fits.
///
/// Clean is the quoted price with the accrued interest stripped out; Dirty is
/// the full settlement price. The two differ by exactly the bond's accrued
/// amount at settlement, so the choice moves the bootstrapped curve for any
/// bond settling mid-coupon and is a no-op for one settling on a coupon date.
#[pyclass(name = "BondPriceType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyBondPriceType {
    Clean,
    Dirty,
}

impl PyBondPriceType {
    /// The core [`BondPriceType`] this variant stands for.
    pub(crate) fn inner(&self) -> BondPriceType {
        match self {
            PyBondPriceType::Clean => BondPriceType::Clean,
            PyBondPriceType::Dirty => BondPriceType::Dirty,
        }
    }
}

/// A helper fitting the quoted price of a fixed-coupon bond it builds itself.
///
/// Unlike the schedule-derived helpers this one is a fixed-date helper: its
/// bond and its dates are built once and do not shift when the evaluation date
/// moves. The pillar is the bond's last cash-flow date, which rolls past the
/// maturity whenever the final payment is date-adjusted, so read pillar_date()
/// rather than assuming the maturity.
///
/// The constructor is contained: it takes eleven of the core's sixteen
/// arguments and defaults the rest. Those defaults are deferrals, not
/// oversights:
///
/// - The four ex-coupon knobs (period, calendar, convention, end-of-month) take
///   the no-ex-coupon defaults the core oracle passes: no period, a null
///   calendar, Unadjusted, and False. An ex-coupon bond is not constructible
///   from Python yet.
/// - payment_calendar is defaulted to None, so the schedule's own calendar
///   rolls the payment dates, again as the core oracle does. A bond paying on a
///   calendar other than its schedule's is not constructible from Python yet.
/// - The generic BondHelper, over an arbitrary pre-built bond, is not faced at
///   all: it needs a bond-instrument facade, which does not exist.
/// - Schedule takes no end_of_month knob, so an end-of-month bond schedule is
///   not constructible from Python yet.
///
/// issue_date is the one core argument moved out of position: it is optional,
/// so it trails the required price_type and settings.
#[pyclass(name = "FixedRateBondHelper", extends = PyRateHelper, unsendable)]
pub struct PyFixedRateBondHelper;

#[pymethods]
impl PyFixedRateBondHelper {
    /// Build the helper over a fixed-coupon bond assembled from the schedule.
    ///
    /// Args:
    ///     price (SimpleQuote): The bond's quoted price, read as clean or dirty
    ///         per price_type. The caller keeps it, so a later set_value
    ///         re-drives the bootstrap.
    ///     settlement_days (int): The days between the evaluation date and the
    ///         bond's settlement date.
    ///     face_amount (float): The notional the coupons accrue on.
    ///     schedule (Schedule): The coupon schedule; its calendar also rolls
    ///         the payment dates.
    ///     coupons (list[float]): The coupon rates, one per period or a single
    ///         rate applied to every period.
    ///     day_counter (DayCounter): The day count the coupons accrue under.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         payment dates.
    ///     redemption (float): The redemption amount, per 100 of face.
    ///     price_type (BondPriceType): Whether price is a clean or a dirty
    ///         quote.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date.
    ///     issue_date (Date | None): The bond's issue date; None leaves it
    ///         unset.
    ///
    /// Raises:
    ///     ItofinError: On whatever the core rejects about the bond, and when
    ///         the evaluation date is unset, since the helper resolves the
    ///         bond's next cash-flow date off it.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        price,
        settlement_days,
        face_amount,
        schedule,
        coupons,
        day_counter,
        payment_convention,
        redemption,
        price_type,
        settings,
        issue_date = None,
    ))]
    fn new(
        price: &PySimpleQuote,
        settlement_days: Natural,
        face_amount: Real,
        schedule: &PySchedule,
        coupons: Vec<Real>,
        day_counter: &PyDayCounter,
        payment_convention: &PyBusinessDayConvention,
        redemption: Real,
        price_type: &PyBondPriceType,
        settings: &PySettings,
        issue_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let helper = FixedRateBondHelper::new(
            price.handle(),
            settlement_days,
            face_amount,
            schedule.inner(),
            coupons,
            day_counter.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            price_type.inner(),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(PyClassInitializer::from(PyRateHelper {
            inner: helper as Shared<dyn RateHelper>,
        })
        .add_subclass(PyFixedRateBondHelper))
    }
}
