//! Facades for the yield term-structure hierarchy: the [`PyYieldTermStructure`]
//! base and the concrete [`PyFlatForward`] curve.

use crate::helpers::PyRateHelper;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use crate::{ItofinError, PyQlError};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::convexmonotone::ConvexMonotone;
use libitofin::math::interpolations::cubic::Cubic;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::bootstraptraits::{Discount, ForwardRate, ZeroYield};
use libitofin::termstructures::globalbootstrap::GlobalBootstrap;
use libitofin::termstructures::localbootstrap::LocalBootstrap;
use libitofin::termstructures::yields::{
    DiscountCurve, FlatForward, ForwardCurve, InterpolatedDiscountCurve, InterpolatedZeroCurve,
    PiecewiseYieldCurve, ZeroCurve,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;

/// Shared base for every yield curve: discount factors, zero and forward rates.
///
/// Concrete curves subclass this and supply only their constructor; the whole
/// query surface below is inherited.
#[pyclass(name = "YieldTermStructure", subclass, unsendable)]
pub struct PyYieldTermStructure {
    inner: Handle<dyn YieldTermStructure>,
}

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
    /// A clone of the inner curve handle for the process/model ctors (H1/W1).
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn YieldTermStructure> {
        self.inner.clone()
    }
}

/// A flat continuously-compounded yield curve behind a Handle.
///
/// Built at annual frequency with continuous compounding, the convention every
/// downstream Heston and Hull-White oracle assumes.
#[pyclass(name = "FlatForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyFlatForward;

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
        PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyFlatForward)
    }
}

/// A yield curve interpolating continuously-compounded zero rates between nodes.
///
/// The first date is the reference date. Finite in time: queries past the last
/// node require enable_extrapolation() or extrapolate=True.
#[pyclass(name = "ZeroCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyZeroCurve;

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
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyZeroCurve))
    }
}

/// A yield curve interpolating discount factors between nodes.
///
/// The first date is the reference date and its discount must be 1.0. Finite
/// in time: queries past the last node require extrapolation.
#[pyclass(name = "DiscountCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyDiscountCurve;

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
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyDiscountCurve))
    }
}

/// A yield curve interpolating instantaneous forward rates backward-flat.
///
/// The first date is the reference date. Finite in time. Unlike ZeroCurve and
/// DiscountCurve this curve offers no cubic option, QuantLib-SWIG exposing its
/// cubic curve on the zero and discount curves only.
///
/// A query past the last node needs enable_extrapolation() or extrapolate=True.
#[pyclass(name = "ForwardCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyForwardCurve;

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
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyForwardCurve))
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
/// What the global bootstrap adds is additional_helpers: instruments handed to
/// the curve and registered with it that contribute neither a pillar nor a
/// residual. Their quote is inert (reading it takes a penalty term, and
/// penalties, additional dates and additional variables from Python are
/// deferred), so all they do is extend the curve's max_date to their own
/// latest_relevant_date, making dates past the last pillar queryable without
/// extrapolation.
///
/// The bootstrap is lazy: construction only rejects an empty helper list, and
/// the solver runs on the first query, re-running after a helper-quote or
/// evaluation-date change. A bootstrap failure therefore surfaces from the
/// query methods, not from the constructor.
///
/// max_date is the exception: it swallows a bootstrap failure and falls back to
/// the last helper's date.
#[pyclass(name = "PiecewiseYieldCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseYieldCurve;

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
    ///         residual. They only extend the curve's max_date to their
    ///         latest_relevant_date; "iterative" rejects them.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list, on an unknown interpolation
    ///         or bootstrap name, on additional helpers under "iterative",
    ///         and on "Cubic" under "global".
    #[new]
    #[pyo3(signature = (
        reference_date,
        helpers,
        day_counter,
        interpolation = "LogLinear",
        bootstrap = "iterative",
        additional_helpers = None,
    ))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        interpolation: &str,
        bootstrap: &str,
        additional_helpers: Option<Vec<PyRef<PyRateHelper>>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let additional: Vec<Shared<dyn RateHelper>> = additional_helpers
            .map(|extra| extra.iter().map(|helper| helper.inner()).collect())
            .unwrap_or_default();
        let curve: Shared<dyn YieldTermStructure> = match bootstrap {
            "iterative" => {
                if !additional.is_empty() {
                    return Err(ItofinError::new_err(
                        "additional_helpers requires bootstrap=\"global\"",
                    ));
                }
                match interpolation {
                    "LogLinear" => PiecewiseYieldCurve::<Discount, LogLinear>::new(
                        reference_date.inner(),
                        instruments,
                        day_counter.inner(),
                        LogLinear,
                    )
                    .map_err(PyQlError::from)?,
                    "Linear" => PiecewiseYieldCurve::<Discount, Linear>::new(
                        reference_date.inner(),
                        instruments,
                        day_counter.inner(),
                        Linear,
                    )
                    .map_err(PyQlError::from)?,
                    "Cubic" => PiecewiseYieldCurve::<Discount, Cubic>::new(
                        reference_date.inner(),
                        instruments,
                        day_counter.inner(),
                        Cubic,
                    )
                    .map_err(PyQlError::from)?,
                    other => {
                        return Err(ItofinError::new_err(format!(
                            "unknown interpolation {other:?}, expected LogLinear, Linear or Cubic"
                        )));
                    }
                }
            }
            "global" => match interpolation {
                "LogLinear" => {
                    PiecewiseYieldCurve::<Discount, LogLinear, GlobalBootstrap>::with_bootstrap(
                        reference_date.inner(),
                        instruments,
                        day_counter.inner(),
                        LogLinear,
                        GlobalBootstrap::with_penalties(
                            additional,
                            None,
                            None,
                            None,
                            Vec::new(),
                            |_, _| Vec::new(),
                        ),
                    )
                    .map_err(PyQlError::from)?
                }
                "Linear" => {
                    PiecewiseYieldCurve::<Discount, Linear, GlobalBootstrap>::with_bootstrap(
                        reference_date.inner(),
                        instruments,
                        day_counter.inner(),
                        Linear,
                        GlobalBootstrap::with_penalties(
                            additional,
                            None,
                            None,
                            None,
                            Vec::new(),
                            |_, _| Vec::new(),
                        ),
                    )
                    .map_err(PyQlError::from)?
                }
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
            },
            other => {
                return Err(ItofinError::new_err(format!(
                    "unknown bootstrap {other:?}, expected iterative or global"
                )));
            }
        };
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyPiecewiseYieldCurve))
    }
}

/// A curve bootstrapped in discount-factor space with log-linear interpolation.
///
/// The verbatim QuantLib-SWIG name for the blessed (Discount, LogLinear)
/// combination. Unlike the PiecewiseYieldCurve alias, the named class retains
/// the concrete curve so it can expose the node introspection the erased
/// handle discards. data() are discount factors, so data()[0] is the reference
/// node's 1.0.
#[pyclass(name = "PiecewiseLogLinearDiscount", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseLogLinearDiscount {
    concrete: Shared<PiecewiseYieldCurve<Discount, LogLinear>>,
}

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
    #[new]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            LogLinear,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(erased),
        })
        .add_subclass(PyPiecewiseLogLinearDiscount { concrete }))
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
#[pyclass(name = "PiecewiseLinearZero", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseLinearZero {
    concrete: Shared<PiecewiseYieldCurve<ZeroYield, Linear>>,
}

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
    #[new]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ZeroYield, Linear>::new(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            Linear,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(erased),
        })
        .add_subclass(PyPiecewiseLinearZero { concrete }))
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
#[pyclass(name = "PiecewiseCubicZero", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseCubicZero {
    concrete: Shared<PiecewiseYieldCurve<ZeroYield, Cubic>>,
}

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
    #[new]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ZeroYield, Cubic>::new(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            Cubic,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(erased),
        })
        .add_subclass(PyPiecewiseCubicZero { concrete }))
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
#[pyclass(name = "PiecewiseLinearForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseLinearForward {
    concrete: Shared<PiecewiseYieldCurve<ForwardRate, Linear>>,
}

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
    #[new]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ForwardRate, Linear>::new(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            Linear,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(erased),
        })
        .add_subclass(PyPiecewiseLinearForward { concrete }))
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
#[pyclass(name = "PiecewiseConvexMonotoneForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseConvexMonotoneForward {
    concrete: ConvexMonotoneCurve,
}

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
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, bootstrap = "iterative"))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        bootstrap: &str,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = match bootstrap {
            "iterative" => ConvexMonotoneCurve::Iterative(
                PiecewiseYieldCurve::<ForwardRate, ConvexMonotone>::new(
                    reference_date.inner(),
                    instruments,
                    day_counter.inner(),
                    ConvexMonotone::default(),
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
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(erased),
        })
        .add_subclass(PyPiecewiseConvexMonotoneForward { concrete }))
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
#[pyclass(name = "PiecewiseFlatForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseFlatForward {
    concrete: Shared<PiecewiseYieldCurve<ForwardRate, BackwardFlat>>,
}

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
    #[new]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let concrete = PiecewiseYieldCurve::<ForwardRate, BackwardFlat>::new(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            BackwardFlat,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(erased),
        })
        .add_subclass(PyPiecewiseFlatForward { concrete }))
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
