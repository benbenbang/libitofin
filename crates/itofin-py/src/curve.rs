//! Facades for the yield term-structure hierarchy: the YieldTermStructure base
//! and the concrete FlatForward curve.

use crate::bootstrap::{BootstrapCallbacks, PySimpleQuoteVariables, global_bootstrap};
use crate::helpers::PyRateHelper;
use crate::iterativebootstrap::{PyIterativeBootstrapOptions, strategy};
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use crate::{ItofinError, PyQlError};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::Interpolator;
use libitofin::math::interpolations::convexmonotone::ConvexMonotone;
use libitofin::math::interpolations::cubic::Cubic;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::bootstraptraits::{
    Discount, ForwardRate, YieldBootstrapTraits, ZeroYield,
};
use libitofin::termstructures::iterativebootstrap::Bootstrap;
use libitofin::termstructures::localbootstrap::LocalBootstrap;
use libitofin::termstructures::yields::{
    DiscountCurve, FlatForward, ForwardCurve, InterpolatedDiscountCurve, InterpolatedZeroCurve,
    PiecewiseYieldCurve, ZeroCurve,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use libitofin::time::{calendar::Calendar, date::Date, daycounter::DayCounter};
use libitofin::types::Natural;
use pyo3::PyTraverseError;
use pyo3::gc::PyVisit;
use pyo3::prelude::*;
#[allow(unused_imports)]
use pyo3_stub_gen::derive::{
    gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pyfunction, gen_stub_pymethods,
};

/// Shared base for every yield curve: discount factors, zero and forward rates.
///
/// Concrete curves subclass this and supply only their constructor; the whole
/// query surface below is inherited.
#[gen_stub_pyclass]
#[pyclass(
    name = "YieldTermStructure",
    subclass,
    unsendable,
    module = "itofin.termstructures"
)]
pub struct PyYieldTermStructure {
    inner: Handle<dyn YieldTermStructure>,
    fresh_links: bool,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyYieldTermStructure {
    /// Return the discount factor at year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, in the curve's own day count.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The discount factor.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and neither extrapolate
    ///         nor the curve's own extrapolation flag allows it.
    #[pyo3(signature = (t, extrapolate = false))]
    fn discount(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .discount(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the discount factor from date back to the reference date.
    ///
    /// Args:
    ///     date (Date): The date discounted from.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The discount factor.
    ///
    /// Raises:
    ///     ItofinError: If date is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn discount_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .discount_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the continuously-compounded zero rate at year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, in the curve's own day count.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The zero rate, continuously compounded at annual frequency.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (t, extrapolate = false))]
    fn zero_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .zero_rate(t, Compounding::Continuous, Frequency::Annual, extrapolate)
            .map_err(PyQlError::from)?
            .rate())
    }

    /// Return the continuously-compounded forward rate between t1 and t2.
    ///
    /// Args:
    ///     t1 (float): The start year fraction.
    ///     t2 (float): The end year fraction.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The forward rate, continuously compounded at annual
    ///         frequency.
    ///
    /// Raises:
    ///     ItofinError: If either time is past the curve's range and
    ///         extrapolation is not allowed.
    #[pyo3(signature = (t1, t2, extrapolate = false))]
    fn forward_rate(&self, t1: f64, t2: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .forward_rate(
                t1,
                t2,
                Compounding::Continuous,
                Frequency::Annual,
                extrapolate,
            )
            .map_err(PyQlError::from)?
            .rate())
    }

    /// Return the date at which the discount factor is 1.0.
    ///
    /// Returns:
    ///     Date: The curve's reference date.
    ///
    /// Raises:
    ///     ItofinError: On a curve whose reference date moves with an
    ///         evaluation date that is not set.
    fn reference_date(&self) -> PyResult<PyDate> {
        let date = self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .reference_date()
            .map_err(PyQlError::from)?;
        Ok(PyDate::from_inner(date))
    }

    /// Return the latest date for which the curve can return values.
    ///
    /// Returns:
    ///     Date: The curve's maximum date.
    fn max_date(&self) -> PyResult<PyDate> {
        let date = self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .max_date();
        Ok(PyDate::from_inner(date))
    }

    /// Return whether the curve answers dates and times beyond its maximum.
    ///
    /// Returns:
    ///     bool: True when extrapolation is enabled on the curve itself.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allow extrapolation past the maximum date and time.
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbid extrapolation past the maximum date and time.
    fn disable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .disable_extrapolation();
        Ok(())
    }
}

impl PyYieldTermStructure {
    pub(crate) fn from_inner(inner: Handle<dyn YieldTermStructure>) -> Self {
        Self {
            inner,
            fresh_links: false,
        }
    }

    /// Wrap a curve built here, so each consumer may get its own link to it.
    pub(crate) fn from_local(curve: Shared<dyn YieldTermStructure>) -> Self {
        Self {
            inner: Handle::new(curve),
            fresh_links: true,
        }
    }

    /// A handle for the process/model ctors (H1/W1).
    ///
    /// Ownership contract: a curve built by this crate (`from_local`) hands
    /// each consumer its own link to the same curve, so the curve's strong
    /// count records every native owner. A handle wrapped by `from_inner` may
    /// carry an owner (a joint-curve member retains its whole assembly) that
    /// `Handle::new` would drop and `current_link` does not carry, so every
    /// consumer shares a clone of that handle instead.
    ///
    /// `PyPiecewiseYieldCurve::__traverse__` relies on this: a new native
    /// holder of a local curve must take its link from here so it raises the
    /// strong count. A holder that does not would let the GC clear callbacks
    /// it still calls.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn YieldTermStructure> {
        match self.inner.current_link() {
            Ok(curve) if self.fresh_links => Handle::new(curve),
            _ => self.inner.clone(),
        }
    }
}

/// A flat continuously-compounded yield curve behind a Handle.
///
/// Built at annual frequency with continuous compounding, the convention every
/// downstream Heston and Hull-White oracle assumes.
#[gen_stub_pyclass]
#[pyclass(name = "FlatForward", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyFlatForward;

#[gen_stub_pymethods]
#[pymethods]
impl PyFlatForward {
    /// Build the flat curve.
    ///
    /// Args:
    ///     reference_date (Date): The date at which the discount factor is
    ///         1.0.
    ///     rate (float): The flat rate, continuously compounded at annual
    ///         frequency.
    ///     day_counter (DayCounter): The day count times are measured in.
    #[gen_stub(override_return_type(type_repr = "FlatForward"))]
    #[new]
    fn new(
        reference_date: &PyDate,
        rate: f64,
        day_counter: &PyDayCounter,
    ) -> PyClassInitializer<Self> {
        let curve = shared(FlatForward::with_rate(
            reference_date.inner(),
            rate,
            day_counter.inner(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>;
        PyClassInitializer::from(PyYieldTermStructure::from_local(curve))
            .add_subclass(PyFlatForward)
    }

    /// Build a flat curve backed by a retained, observable quote.
    #[staticmethod]
    fn from_quote(
        py: Python<'_>,
        reference_date: &PyDate,
        quote: &PySimpleQuote,
        day_counter: &PyDayCounter,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatForward::new(
            reference_date.inner(),
            quote.handle(),
            day_counter.inner(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyYieldTermStructure::from_local(curve)).add_subclass(Self),
        )
    }
}

/// A yield curve interpolating continuously-compounded zero rates between nodes.
///
/// The first date is the reference date. Finite in time: queries past the last
/// node require enable_extrapolation() or extrapolate=True.
#[gen_stub_pyclass]
#[pyclass(name = "ZeroCurve", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyZeroCurve;

#[gen_stub_pymethods]
#[pymethods]
impl PyZeroCurve {
    /// Build the curve over its (date, zero-rate) nodes.
    ///
    /// Args:
    ///     dates (list[Date]): The node dates, the first being the reference
    ///         date.
    ///     yields (list[float]): The continuously-compounded zero rate at each
    ///         node.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     interpolation (str): "Linear", the shipped behaviour, or "Cubic",
    ///         the Kruger cubic factory, which is non-monotonic.
    ///
    /// Raises:
    ///     ItofinError: On an unknown interpolation name, and on whatever the
    ///         core rejects about the nodes.
    #[gen_stub(override_return_type(type_repr = "ZeroCurve"))]
    #[new]
    #[pyo3(signature = (dates, yields, day_counter, interpolation = "Linear"))]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        yields: Vec<f64>,
        day_counter: &PyDayCounter,
        interpolation: &str,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let curve: Shared<dyn YieldTermStructure> = match interpolation {
            "Linear" => shared(
                ZeroCurve::new(dates, yields, day_counter.inner(), Linear)
                    .map_err(PyQlError::from)?,
            ),
            "Cubic" => shared(
                InterpolatedZeroCurve::<Cubic>::new(dates, yields, day_counter.inner(), Cubic)
                    .map_err(PyQlError::from)?,
            ),
            other => {
                return Err(ItofinError::new_err(format!(
                    "unknown interpolation {other:?}, expected Linear or Cubic"
                )));
            }
        };
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(curve))
                .add_subclass(PyZeroCurve),
        )
    }
}

/// A yield curve interpolating discount factors between nodes.
///
/// The first date is the reference date and its discount must be 1.0. Finite
/// in time: queries past the last node require extrapolation.
#[gen_stub_pyclass]
#[pyclass(name = "DiscountCurve", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyDiscountCurve;

#[gen_stub_pymethods]
#[pymethods]
impl PyDiscountCurve {
    /// Build the curve over its (date, discount-factor) nodes.
    ///
    /// Args:
    ///     dates (list[Date]): The node dates, the first being the reference
    ///         date.
    ///     discounts (list[float]): The discount factor at each node; the
    ///         first must be 1.0.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     calendar (Calendar | None): The curve's calendar; unlike the other
    ///         two node curves this constructor accepts one.
    ///     interpolation (str): "LogLinear", the shipped behaviour, giving
    ///         piecewise-constant forwards, or "Cubic", which is
    ///         non-monotonic.
    ///
    /// Raises:
    ///     ItofinError: On an unknown interpolation name, and on whatever the
    ///         core rejects about the nodes.
    #[gen_stub(override_return_type(type_repr = "DiscountCurve"))]
    #[new]
    #[pyo3(signature = (dates, discounts, day_counter, calendar = None, interpolation = "LogLinear"))]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        discounts: Vec<f64>,
        day_counter: &PyDayCounter,
        calendar: Option<&PyCalendar>,
        interpolation: &str,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let calendar = calendar.map(PyCalendar::inner);
        let curve: Shared<dyn YieldTermStructure> = match interpolation {
            "LogLinear" => shared(
                DiscountCurve::new(dates, discounts, day_counter.inner(), calendar)
                    .map_err(PyQlError::from)?,
            ),
            "Cubic" => shared(
                InterpolatedDiscountCurve::<Cubic>::new(
                    dates,
                    discounts,
                    day_counter.inner(),
                    calendar,
                )
                .map_err(PyQlError::from)?,
            ),
            other => {
                return Err(ItofinError::new_err(format!(
                    "unknown interpolation {other:?}, expected LogLinear or Cubic"
                )));
            }
        };
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(curve))
                .add_subclass(PyDiscountCurve),
        )
    }
}

/// A yield curve interpolating instantaneous forward rates backward-flat.
///
/// The first date is the reference date. Finite in time. Unlike ZeroCurve and
/// DiscountCurve this curve offers no cubic option, QuantLib-SWIG exposing its
/// cubic curve on the zero and discount curves only.
///
/// A query past the last node needs enable_extrapolation() or extrapolate=True.
#[gen_stub_pyclass]
#[pyclass(name = "ForwardCurve", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyForwardCurve;

#[gen_stub_pymethods]
#[pymethods]
impl PyForwardCurve {
    /// Build the curve over its (date, forward-rate) nodes.
    ///
    /// Args:
    ///     dates (list[Date]): The node dates, the first being the reference
    ///         date.
    ///     forwards (list[float]): The instantaneous forward rate at each
    ///         node.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On whatever the core rejects about the nodes.
    #[gen_stub(override_return_type(type_repr = "ForwardCurve"))]
    #[new]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        forwards: Vec<f64>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let curve = shared(
            ForwardCurve::new(dates, forwards, day_counter.inner(), BackwardFlat)
                .map_err(PyQlError::from)?,
        ) as Shared<dyn YieldTermStructure>;
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(curve))
                .add_subclass(PyForwardCurve),
        )
    }
}

/// A yield curve bootstrapped from a strip of rate helpers, one node per maturity.
///
/// Every helper is solved so it reprices its own market quote off the curve.
/// This string-dispatch alias covers the Discount convention; the other
/// bootstrap conventions are reached through the named Piecewise* classes,
/// which also expose node introspection.
///
/// bootstrap selects the algorithm: "iterative" (the default) solves one node
/// at a time, "global" solves every node at once through a
/// Levenberg-Marquardt fit of all helper residuals. The two are exactly
/// determined on a plain strip and agree at every pillar to about 1e-13, so
/// "global" is a faithful superset rather than a divergent algorithm. It is
/// offered for "LogLinear" and "Linear" only.
///
/// Global-only restrictions include additional helpers, callable additional
/// node dates, penalties(times, data) returning extra residuals, and external
/// SimpleQuoteVariables. Callbacks run synchronously on the owning thread and
/// are retained by native consumers even after this Python wrapper is dropped.
/// Fallible pricing queries report callback exceptions as ItofinError with their
/// original type and message.
///
/// A callback that strongly captures this curve is collected once the cycle is
/// unreachable and no native consumer retains the curve. A strong capture of a
/// consumer that retains the curve still cannot be garbage-collected: capture a
/// weakref.ref instead. Capturing additional helpers is supported.
/// Keep the residual count constant during each calculation; values must be finite.
/// Quote mutation from a callback is rejected; read helpers and trial nodes instead.
/// The supplied time and data lists include the reference node and are copies.
///
/// The bootstrap is lazy: construction only rejects an empty helper list, and
/// the solver runs on the first query, re-running after a helper-quote or
/// evaluation-date change. A bootstrap failure therefore surfaces from the
/// query methods, not from the constructor.
///
/// max_date is the exception: it swallows a bootstrap failure and reports the
/// current grid bound, or the reference date before a grid has been installed.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseYieldCurve", extends = PyYieldTermStructure, unsendable, weakref, module = "itofin.termstructures")]
pub struct PyPiecewiseYieldCurve {
    callbacks: Option<(Shared<dyn YieldTermStructure>, Shared<BootstrapCallbacks>)>,
}

impl PyPiecewiseYieldCurve {
    /// The callbacks, exposed to the cycle GC only while this wrapper is the
    /// sole native owner of the curve.
    ///
    /// Exactly two strong holders are expected then: the wrapper's own link
    /// (built by `from_local`) and the `(curve, cell)` pair kept here. Helpers
    /// link to the curve weakly and observers are weak, so neither counts.
    /// Every consumer gets its own link from `handle()`, so any consumer, any
    /// other clone, and any transient `current_link()` clone (a running query)
    /// raises the count and hides the edge. Hiding is fail-safe: the cycle
    /// leaks, but a live consumer's callbacks are never cleared. A foreign-thread
    /// GC pass skips this unsendable traversal for the same reason. Joint curves
    /// build `GlobalBootstrap::new` internally and never carry Python callbacks.
    ///
    /// Rule: a `__traverse__` on an object shared with native code must
    /// under-report. A leak is safe; over-reporting lets the GC clear a
    /// callback a native consumer still calls, a use-after-free.
    fn sole_owner_callbacks(&self) -> Option<&BootstrapCallbacks> {
        self.callbacks
            .as_ref()
            .filter(|(curve, _)| Shared::strong_count(curve) == 2)
            .map(|(_, callbacks)| &**callbacks)
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseYieldCurve {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date, typically the
    ///         settlement date the caller computed.
    ///     helpers (list[RateHelper]): The bootstrap instruments; any
    ///         RateHelper subclass is accepted.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     interpolation (str): "LogLinear", "Linear" or "Cubic". Cubic is a
    ///         global interpolator, so its bootstrap runs the multi-pass
    ///         convergence loop instead of a single pass.
    ///     bootstrap (str): "iterative" (the default) or "global". "global"
    ///         supports "LogLinear" and "Linear" only.
    ///     additional_helpers (list[RateHelper] | None): Instruments the
    ///         global bootstrap registers without giving them a pillar or a
    ///         residual. Penalty callbacks may read their quote_error().
    ///     additional_penalties (Callable | None): Called with (times, data)
    ///         to return finite additional least-squares residuals.
    ///     additional_dates (Callable | None): Returns Date objects, read again
    ///         on each recalculation. Additional nodes need matching residuals.
    ///     additional_variables (SimpleQuoteVariables | None): External quotes
    ///         solved jointly with nodes. All additional options require global.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list, on an unknown interpolation
    ///         or bootstrap name, on additional helpers under "iterative",
    ///         and on "Cubic" under "global".
    #[gen_stub(override_return_type(type_repr = "PiecewiseYieldCurve"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        reference_date,
        helpers,
        day_counter,
        interpolation = "LogLinear",
        bootstrap = "iterative",
        additional_helpers = None,
        *,
        additional_penalties = None,
        additional_dates = None,
        additional_variables = None,
        iterative_options = None,
    ))]
    fn new(
        py: Python<'_>,
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        interpolation: &str,
        bootstrap: &str,
        additional_helpers: Option<Vec<PyRef<PyRateHelper>>>,
        #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[list[float], list[float]], list[float]]]", imports = ("typing")))]
        additional_penalties: Option<Py<PyAny>>,
        #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[], list[time.Date]]]", imports = ("typing", "itofin.time")))]
        additional_dates: Option<Py<PyAny>>,
        additional_variables: Option<PyRef<PySimpleQuoteVariables>>,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let (curve, cell) = piecewise_curve(
            py,
            Anchor::Fixed(reference_date.inner()),
            helpers,
            day_counter,
            interpolation,
            bootstrap,
            additional_helpers,
            additional_penalties,
            additional_dates,
            additional_variables,
            iterative_options,
        )?;
        let callbacks = cell.map(|cell| (Shared::clone(&curve), cell));
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(curve))
                .add_subclass(PyPiecewiseYieldCurve { callbacks }),
        )
    }

    #[gen_stub(skip)]
    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        match self.sole_owner_callbacks() {
            Some(callbacks) => callbacks.visit(&visit),
            None => Ok(()),
        }
    }

    #[gen_stub(skip)]
    fn __clear__(&self) {
        if let Some(callbacks) = self.sole_owner_callbacks() {
            callbacks.clear();
        }
    }
    /// Build the curve on a reference date that moves with the evaluation date.
    ///
    /// The reference date sits settlement_days business days on calendar past
    /// the evaluation date of settings; a roll re-bootstraps the curve. A
    /// query made before settings carries an evaluation date raises rather
    /// than falling back to a system clock.
    ///
    /// settings must be the same Settings instance the helpers' indexes carry:
    /// a different instance moves the curve's reference date while the
    /// helpers' dates stay put.
    ///
    /// Args:
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar those days are counted on.
    ///     helpers (list[RateHelper]): The bootstrap instruments.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     settings (Settings): The settings supplying the evaluation date.
    ///     interpolation (str): As in the constructor.
    ///     bootstrap (str): As in the constructor.
    ///     additional_helpers (list[RateHelper] | None): As in the constructor.
    ///     additional_penalties (Callable | None): As in the constructor.
    ///     additional_dates (Callable | None): As in the constructor.
    ///     additional_variables (SimpleQuoteVariables | None): As in the
    ///         constructor.
    ///
    /// Returns:
    ///     PiecewiseYieldCurve: The moving curve.
    ///
    /// Raises:
    ///     ItofinError: On the constructor's argument errors.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        settlement_days,
        calendar,
        helpers,
        day_counter,
        settings,
        interpolation = "LogLinear",
        bootstrap = "iterative",
        additional_helpers = None,
        *,
        additional_penalties = None,
        additional_dates = None,
        additional_variables = None,
        iterative_options = None,
    ))]
    fn moving(
        py: Python<'_>,
        settlement_days: Natural,
        calendar: &PyCalendar,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        settings: &PySettings,
        interpolation: &str,
        bootstrap: &str,
        additional_helpers: Option<Vec<PyRef<PyRateHelper>>>,
        #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[list[float], list[float]], list[float]]]", imports = ("typing")))]
        additional_penalties: Option<Py<PyAny>>,
        #[gen_stub(override_type(type_repr = "typing.Optional[typing.Callable[[], list[time.Date]]]", imports = ("typing", "itofin.time")))]
        additional_dates: Option<Py<PyAny>>,
        additional_variables: Option<PyRef<PySimpleQuoteVariables>>,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<Py<Self>> {
        let anchor = Anchor::Moving(settlement_days, calendar.inner(), settings.inner());
        let (curve, cell) = piecewise_curve(
            py,
            anchor,
            helpers,
            day_counter,
            interpolation,
            bootstrap,
            additional_helpers,
            additional_penalties,
            additional_dates,
            additional_variables,
            iterative_options,
        )?;
        let callbacks = cell.map(|cell| (Shared::clone(&curve), cell));
        Py::new(
            py,
            PyClassInitializer::from(PyYieldTermStructure::from_local(curve))
                .add_subclass(PyPiecewiseYieldCurve { callbacks }),
        )
    }
}

/// The reference date of a piecewise curve: fixed, or moving with settings.
enum Anchor {
    Fixed(Date),
    Moving(Natural, Calendar, Shared<Settings<Date>>),
}

fn anchored<T, I, B>(
    anchor: Anchor,
    instruments: Vec<Shared<dyn RateHelper>>,
    day_counter: DayCounter,
    interpolator: I,
    bootstrap: B,
) -> PyResult<Shared<dyn YieldTermStructure>>
where
    T: YieldBootstrapTraits + 'static,
    I: Interpolator + 'static,
    B: Bootstrap<PiecewiseYieldCurve<T, I, B>> + 'static,
{
    Ok(match anchor {
        Anchor::Fixed(date) => PiecewiseYieldCurve::<T, I, B>::with_bootstrap(
            date,
            instruments,
            day_counter,
            interpolator,
            bootstrap,
        ),
        Anchor::Moving(days, calendar, settings) => PiecewiseYieldCurve::moving_with_bootstrap(
            days,
            calendar,
            instruments,
            day_counter,
            interpolator,
            settings,
            bootstrap,
        ),
    }
    .map_err(PyQlError::from)?)
}

/// A built piecewise curve and, for a global bootstrap, its Python callback cell.
type BuiltCurve = (
    Shared<dyn YieldTermStructure>,
    Option<Shared<BootstrapCallbacks>>,
);

#[allow(clippy::too_many_arguments)]
fn piecewise_curve(
    py: Python<'_>,
    anchor: Anchor,
    helpers: Vec<PyRef<PyRateHelper>>,
    day_counter: &PyDayCounter,
    interpolation: &str,
    bootstrap: &str,
    additional_helpers: Option<Vec<PyRef<PyRateHelper>>>,
    additional_penalties: Option<Py<PyAny>>,
    additional_dates: Option<Py<PyAny>>,
    additional_variables: Option<PyRef<PySimpleQuoteVariables>>,
    iterative_options: Option<&PyIterativeBootstrapOptions>,
) -> PyResult<BuiltCurve> {
    if bootstrap != "iterative" && iterative_options.is_some() {
        return Err(ItofinError::new_err(
            "iterative_options require bootstrap=\"iterative\"",
        ));
    }
    let instruments: Vec<Shared<dyn RateHelper>> =
        helpers.iter().map(|helper| helper.inner()).collect();
    let additional: Vec<Shared<dyn RateHelper>> = additional_helpers
        .map(|extra| extra.iter().map(|helper| helper.inner()).collect())
        .unwrap_or_default();
    let mut callbacks = None;
    let curve = match bootstrap {
        "iterative" => {
            if !additional.is_empty()
                || additional_penalties.is_some()
                || additional_dates.is_some()
                || additional_variables.is_some()
            {
                return Err(ItofinError::new_err(
                    "additional_helpers, additional_penalties, additional_dates and additional_variables require bootstrap=\"global\"",
                ));
            }
            match interpolation {
                "LogLinear" => anchored::<Discount, _, _>(
                    anchor,
                    instruments,
                    day_counter.inner(),
                    LogLinear,
                    strategy(iterative_options)?,
                )?,
                "Linear" => anchored::<Discount, _, _>(
                    anchor,
                    instruments,
                    day_counter.inner(),
                    Linear,
                    strategy(iterative_options)?,
                )?,
                "Cubic" => anchored::<Discount, _, _>(
                    anchor,
                    instruments,
                    day_counter.inner(),
                    Cubic,
                    strategy(iterative_options)?,
                )?,
                other => {
                    return Err(ItofinError::new_err(format!(
                        "unknown interpolation {other:?}, expected LogLinear, Linear or Cubic"
                    )));
                }
            }
        }
        "global" => {
            let (strategy, cell) = global_bootstrap(
                py,
                additional,
                additional_penalties,
                additional_dates,
                additional_variables.as_deref(),
            )?;
            callbacks = Some(cell);
            match interpolation {
                "LogLinear" => anchored::<Discount, _, _>(
                    anchor,
                    instruments,
                    day_counter.inner(),
                    LogLinear,
                    strategy,
                )?,
                "Linear" => anchored::<Discount, _, _>(
                    anchor,
                    instruments,
                    day_counter.inner(),
                    Linear,
                    strategy,
                )?,
                "Cubic" => {
                    return Err(ItofinError::new_err(
                        "bootstrap=\"global\" supports LogLinear or Linear",
                    ));
                }
                other => {
                    return Err(ItofinError::new_err(format!(
                        "unknown interpolation {other:?}, expected LogLinear, Linear or Cubic"
                    )));
                }
            }
        }
        other => {
            return Err(ItofinError::new_err(format!(
                "unknown bootstrap {other:?}, expected iterative or global"
            )));
        }
    };
    Ok((curve, callbacks))
}

/// A curve bootstrapped in discount-factor space with log-linear interpolation.
///
/// The verbatim QuantLib-SWIG name for the blessed (Discount, LogLinear)
/// combination. Unlike the PiecewiseYieldCurve alias, the named class retains
/// the concrete curve so it can expose the node introspection the erased
/// handle discards. data() are discount factors, so data()[0] is the reference
/// node's 1.0.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseLogLinearDiscount", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseLogLinearDiscount {
    concrete: Shared<PiecewiseYieldCurve<Discount, LogLinear>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseLogLinearDiscount {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[RateHelper]): The bootstrap instruments; any
    ///         RateHelper subclass is accepted.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
    #[gen_stub(override_return_type(type_repr = "PiecewiseLogLinearDiscount"))]
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, iterative_options=None))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<Discount, LogLinear>::with_bootstrap(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            LogLinear,
            strategy(iterative_options)?,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(erased))
                .add_subclass(PyPiecewiseLogLinearDiscount { concrete }),
        )
    }

    /// Return the bootstrapped node dates, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[Date]: One date per helper maturity, plus the reference node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// Return the bootstrapped node values, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[float]: The discount factors, the first being 1.0.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

/// A curve bootstrapped in zero-rate space with linear interpolation.
///
/// The verbatim QuantLib-SWIG name for the blessed (ZeroYield, Linear)
/// combination. data() are continuously-compounded zero rates, so data()[0]
/// mirrors the first solved pillar's rate rather than a 1.0 discount.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseLinearZero", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseLinearZero {
    concrete: Shared<PiecewiseYieldCurve<ZeroYield, Linear>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseLinearZero {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[RateHelper]): The bootstrap instruments.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
    #[gen_stub(override_return_type(type_repr = "PiecewiseLinearZero"))]
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, iterative_options=None))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ZeroYield, Linear>::with_bootstrap(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            Linear,
            strategy(iterative_options)?,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(erased))
                .add_subclass(PyPiecewiseLinearZero { concrete }),
        )
    }

    /// Return the bootstrapped node dates, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[Date]: One date per helper maturity, plus the reference node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// Return the bootstrapped node values, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[float]: The zero rates at the nodes.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

/// A curve bootstrapped in zero-rate space with Kruger cubic interpolation.
///
/// The QuantLib-SWIG name for the (ZeroYield, Cubic) combination. Cubic is a
/// global interpolator, so the bootstrap runs the multi-pass convergence loop
/// instead of a single pass. data() are continuously-compounded zero rates, so
/// data()[0] mirrors the first solved pillar's rate rather than a 1.0 discount.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseCubicZero", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseCubicZero {
    concrete: Shared<PiecewiseYieldCurve<ZeroYield, Cubic>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseCubicZero {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[RateHelper]): The bootstrap instruments.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
    #[gen_stub(override_return_type(type_repr = "PiecewiseCubicZero"))]
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, iterative_options=None))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ZeroYield, Cubic>::with_bootstrap(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            Cubic,
            strategy(iterative_options)?,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(erased))
                .add_subclass(PyPiecewiseCubicZero { concrete }),
        )
    }

    /// Return the bootstrapped node dates, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[Date]: One date per helper maturity, plus the reference node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// Return the bootstrapped node values, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[float]: The zero rates at the nodes.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

/// A curve bootstrapped in instantaneous forward-rate space, interpolating linearly.
///
/// The verbatim QuantLib-SWIG name for the blessed (ForwardRate, Linear)
/// combination. data() are instantaneous forward rates.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseLinearForward", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseLinearForward {
    concrete: Shared<PiecewiseYieldCurve<ForwardRate, Linear>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseLinearForward {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[RateHelper]): The bootstrap instruments.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
    #[gen_stub(override_return_type(type_repr = "PiecewiseLinearForward"))]
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, iterative_options=None))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ForwardRate, Linear>::with_bootstrap(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            Linear,
            strategy(iterative_options)?,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(erased))
                .add_subclass(PyPiecewiseLinearForward { concrete }),
        )
    }

    /// Return the bootstrapped node dates, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[Date]: One date per helper maturity, plus the reference node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// Return the bootstrapped node values, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[float]: The instantaneous forward rates at the nodes.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

enum ConvexMonotoneCurve {
    Iterative(Shared<PiecewiseYieldCurve<ForwardRate, ConvexMonotone>>),
    Local(Shared<PiecewiseYieldCurve<ForwardRate, ConvexMonotone, LocalBootstrap>>),
}

/// A curve bootstrapped in forward-rate space with convex-monotone interpolation.
///
/// The QuantLib-SWIG name for the (ForwardRate, ConvexMonotone) combination,
/// built with QuantLib's defaults (quadraticity 0.3, monotonicity 0.7, forced
/// positive). ConvexMonotone is a global interpolator that reads the solved
/// nodes as discrete forwards, so the bootstrap runs the multi-pass
/// convergence loop. data() are instantaneous forward rates; the interpolation
/// ignores node [0], which only mirrors the first solved pillar.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseConvexMonotoneForward", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseConvexMonotoneForward {
    concrete: ConvexMonotoneCurve,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseConvexMonotoneForward {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[RateHelper]): The bootstrap instruments.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     bootstrap (str): "iterative", the shipped behaviour, solving one
    ///         node at a time, or "local", least-squares-fitting a trailing
    ///         window of nodes at each step so the non-local interpolation
    ///         keeps a localised risk profile. The two reprice every pillar
    ///         (the local solve to its own tolerance, about 1e-7 on the
    ///         oracle strip) and diverge between them.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list, and on an unknown bootstrap
    ///         name.
    #[gen_stub(override_return_type(type_repr = "PiecewiseConvexMonotoneForward"))]
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, bootstrap = "iterative", iterative_options=None))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        bootstrap: &str,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        if bootstrap != "iterative" && iterative_options.is_some() {
            return Err(ItofinError::new_err(
                "iterative_options require bootstrap=\"iterative\"",
            ));
        }
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = match bootstrap {
            "iterative" => ConvexMonotoneCurve::Iterative(
                PiecewiseYieldCurve::<ForwardRate, ConvexMonotone>::with_bootstrap(
                    reference_date.inner(),
                    instruments,
                    day_counter.inner(),
                    ConvexMonotone::default(),
                    strategy(iterative_options)?,
                )
                .map_err(PyQlError::from)?,
            ),
            "local" => ConvexMonotoneCurve::Local(
                PiecewiseYieldCurve::<ForwardRate, ConvexMonotone, LocalBootstrap>::new(
                    reference_date.inner(),
                    instruments,
                    day_counter.inner(),
                    ConvexMonotone::default(),
                )
                .map_err(PyQlError::from)?,
            ),
            other => {
                return Err(ItofinError::new_err(format!(
                    "unknown bootstrap {other:?}, expected iterative or local"
                )));
            }
        };
        let erased: Shared<dyn YieldTermStructure> = match &concrete {
            ConvexMonotoneCurve::Iterative(curve) => {
                Shared::clone(curve) as Shared<dyn YieldTermStructure>
            }
            ConvexMonotoneCurve::Local(curve) => {
                Shared::clone(curve) as Shared<dyn YieldTermStructure>
            }
        };
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(erased))
                .add_subclass(PyPiecewiseConvexMonotoneForward { concrete }),
        )
    }

    /// Return the bootstrapped node dates, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[Date]: One date per helper maturity, plus the reference node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        let dates = match &self.concrete {
            ConvexMonotoneCurve::Iterative(curve) => curve.dates(),
            ConvexMonotoneCurve::Local(curve) => curve.dates(),
        };
        Ok(dates
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// Return the bootstrapped node values, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[float]: The instantaneous forward rates at the nodes.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        let data = match &self.concrete {
            ConvexMonotoneCurve::Iterative(curve) => curve.data(),
            ConvexMonotoneCurve::Local(curve) => curve.data(),
        };
        Ok(data.map_err(PyQlError::from)?)
    }
}

/// A curve bootstrapped in forward-rate space, interpolating backward-flat.
///
/// The verbatim QuantLib-SWIG name for the blessed (ForwardRate, BackwardFlat)
/// combination. Piecewise-constant instantaneous forwards make it numerically
/// identical to PiecewiseLogLinearDiscount under every query; only data(),
/// forward rates against discount factors, tells the two apart.
#[gen_stub_pyclass]
#[pyclass(name = "PiecewiseFlatForward", extends = PyYieldTermStructure, unsendable, module = "itofin.termstructures")]
pub struct PyPiecewiseFlatForward {
    concrete: Shared<PiecewiseYieldCurve<ForwardRate, BackwardFlat>>,
}

#[gen_stub_pymethods]
#[pymethods]
impl PyPiecewiseFlatForward {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[RateHelper]): The bootstrap instruments.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
    #[gen_stub(override_return_type(type_repr = "PiecewiseFlatForward"))]
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, iterative_options=None))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        iterative_options: Option<&PyIterativeBootstrapOptions>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ForwardRate, BackwardFlat>::with_bootstrap(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            BackwardFlat,
            strategy(iterative_options)?,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(
            PyClassInitializer::from(PyYieldTermStructure::from_local(erased))
                .add_subclass(PyPiecewiseFlatForward { concrete }),
        )
    }

    /// Return the bootstrapped node dates, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[Date]: One date per helper maturity, plus the reference node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// Return the bootstrapped node values, triggering the lazy bootstrap.
    ///
    /// Returns:
    ///     list[float]: The instantaneous forward rates at the nodes.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use libitofin::time::date::{Date, Month};
    use libitofin::time::daycounters::actual360::Actual360;

    fn flat() -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            Date::new(1, Month::January, 2026),
            0.025,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ))
    }

    #[test]
    fn local_curve_gives_each_consumer_its_own_link() {
        let curve = flat();
        let wrapper = PyYieldTermStructure::from_local(Shared::clone(&curve));
        let (first, second) = (wrapper.handle(), wrapper.handle());
        assert!(!first.points_to_same_link(&second));
        assert!(!first.points_to_same_link(&wrapper.inner));
        assert_eq!(Shared::strong_count(&curve), 4);
    }

    #[test]
    fn callbacks_are_hidden_while_a_consumer_link_lives() {
        let curve = flat();
        let wrapper = PyYieldTermStructure::from_local(Shared::clone(&curve));
        let piecewise = PyPiecewiseYieldCurve {
            callbacks: Some((curve, shared(BootstrapCallbacks::empty()))),
        };
        assert!(piecewise.sole_owner_callbacks().is_some());
        let consumer = wrapper.handle();
        assert!(piecewise.sole_owner_callbacks().is_none());
        drop(consumer);
        assert!(piecewise.sole_owner_callbacks().is_some());
    }

    #[test]
    fn wrapped_handle_is_shared_by_every_consumer() {
        let wrapper = PyYieldTermStructure::from_inner(Handle::new(flat()));
        assert!(wrapper.handle().points_to_same_link(&wrapper.inner));
        assert!(wrapper.handle().points_to_same_link(&wrapper.handle()));
    }
}
