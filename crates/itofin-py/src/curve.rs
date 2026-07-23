//! Facades for the yield term-structure hierarchy: the [`PyYieldTermStructure`]
//! base and the concrete [`PyFlatForward`] curve.

use crate::PyQlError;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::math::interpolations::linear::Linear;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::{DiscountCurve, FlatForward, ForwardCurve, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;

/// Python `YieldTermStructure`: the shared base for every yield curve
/// (`termstructures::yieldtermstructure::YieldTermStructure`).
///
/// Holds the erased `Handle<dyn YieldTermStructure>` and exposes the query
/// surface every concrete curve inherits (discount factors, zero and forward
/// rates) plus the extrapolation toggles. Concrete curves such as
/// [`PyFlatForward`] subclass this and supply only their constructor.
#[pyclass(name = "YieldTermStructure", subclass, unsendable)]
pub struct PyYieldTermStructure {
    inner: Handle<dyn YieldTermStructure>,
}

#[pymethods]
impl PyYieldTermStructure {
    /// The discount factor at year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn discount(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .discount(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The discount factor from `date` to the reference date.
    #[pyo3(signature = (date, extrapolate = false))]
    fn discount_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .discount_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The continuously-compounded zero rate at year-fraction `t`, read back
    /// with the convention the curve was built with.
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

    /// The continuously-compounded forward rate between year-fractions `t1`
    /// and `t2`.
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

    /// The date at which the discount factor is 1.0.
    fn reference_date(&self) -> PyResult<PyDate> {
        let date = self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .reference_date()
            .map_err(PyQlError::from)?;
        Ok(PyDate::from_inner(date))
    }

    /// The latest date for which the curve can return values.
    fn max_date(&self) -> PyResult<PyDate> {
        let date = self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .max_date();
        Ok(PyDate::from_inner(date))
    }

    /// Whether the curve answers dates/times beyond its maximum.
    fn allows_extrapolation(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .allows_extrapolation())
    }

    /// Allows extrapolation past the maximum date/time.
    fn enable_extrapolation(&self) -> PyResult<()> {
        self.inner
            .current_link()
            .map_err(PyQlError::from)?
            .enable_extrapolation();
        Ok(())
    }

    /// Forbids extrapolation past the maximum date/time.
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

/// Python `FlatForward`: a flat continuously-compounded yield curve behind a
/// [`Handle`] (`termstructures::yields::FlatForward`).
///
/// Built with `Compounding::Continuous` and `Frequency::Annual` - the
/// convention every downstream Heston/Hull-White oracle assumes. The query
/// surface is inherited from [`PyYieldTermStructure`]; the `Handle` is
/// assembled internally so it never crosses the PyO3 boundary, and the pricing
/// facades (H1/W1) take a clone of it through the base's crate-internal
/// accessor.
#[pyclass(name = "FlatForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyFlatForward;

#[pymethods]
impl PyFlatForward {
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

/// Python `ZeroCurve`: a yield curve built from (date, continuously-compounded
/// zero-rate) nodes, interpolating linearly in zero-rate space
/// (`termstructures::yields::ZeroCurve = InterpolatedZeroCurve<Linear>`).
///
/// Extends [`PyYieldTermStructure`]; the first date is the reference date and
/// the query surface is inherited. Finite in time: queries past the last node
/// require `enable_extrapolation()` or `extrapolate=True`.
#[pyclass(name = "ZeroCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyZeroCurve;

#[pymethods]
impl PyZeroCurve {
    #[new]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        yields: Vec<f64>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let curve = shared(
            ZeroCurve::new(dates, yields, day_counter.inner(), Linear).map_err(PyQlError::from)?,
        ) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyZeroCurve))
    }
}

/// Python `DiscountCurve`: a yield curve built from (date, discount-factor)
/// nodes, interpolating log-linearly for piecewise-constant forwards
/// (`termstructures::yields::DiscountCurve = InterpolatedDiscountCurve<LogLinear>`).
///
/// Extends [`PyYieldTermStructure`]; the first date is the reference date and
/// its discount must be 1.0. Unlike the other two curves this constructor
/// accepts an optional calendar. Finite in time: queries past the last node
/// require `enable_extrapolation()` or `extrapolate=True`.
#[pyclass(name = "DiscountCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyDiscountCurve;

#[pymethods]
impl PyDiscountCurve {
    #[new]
    #[pyo3(signature = (dates, discounts, day_counter, calendar = None))]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        discounts: Vec<f64>,
        day_counter: &PyDayCounter,
        calendar: Option<&PyCalendar>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|d| d.inner()).collect();
        let curve = shared(
            DiscountCurve::new(
                dates,
                discounts,
                day_counter.inner(),
                calendar.map(PyCalendar::inner),
            )
            .map_err(PyQlError::from)?,
        ) as Shared<dyn YieldTermStructure>;
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyDiscountCurve))
    }
}

/// Python `ForwardCurve`: a yield curve built from (date, instantaneous
/// forward-rate) nodes, interpolating backward-flat
/// (`termstructures::yields::ForwardCurve = InterpolatedForwardCurve<BackwardFlat>`).
///
/// Extends [`PyYieldTermStructure`]; the first date is the reference date and
/// the query surface is inherited. Finite in time: queries past the last node
/// require `enable_extrapolation()` or `extrapolate=True`.
#[pyclass(name = "ForwardCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyForwardCurve;

#[pymethods]
impl PyForwardCurve {
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
