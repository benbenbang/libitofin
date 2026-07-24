//! Facades for the yield term-structure hierarchy: the [`PyYieldTermStructure`]
//! base and the concrete [`PyFlatForward`] curve.

use crate::helpers::PyRateHelper;
use crate::time::{PyCalendar, PyDate, PyDayCounter};
use crate::{ItofinError, PyQlError};
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::cubic::Cubic;
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::interpolations::loglinear::LogLinear;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::RateHelper;
use libitofin::termstructures::bootstraptraits::{Discount, ForwardRate, ZeroYield};
use libitofin::termstructures::yields::{
    DiscountCurve, FlatForward, ForwardCurve, InterpolatedDiscountCurve, InterpolatedZeroCurve,
    PiecewiseYieldCurve, ZeroCurve,
};
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
/// the query surface is inherited. `interpolation` selects the zero-rate
/// interpolator: `"Linear"` (default, the shipped behaviour) or `"Cubic"` (the
/// Kruger cubic factory, non-monotonic). Finite in time: queries past the last
/// node require `enable_extrapolation()` or `extrapolate=True`.
#[pyclass(name = "ZeroCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyZeroCurve;

#[pymethods]
impl PyZeroCurve {
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

/// Python `DiscountCurve`: a yield curve built from (date, discount-factor)
/// nodes, interpolating log-linearly for piecewise-constant forwards
/// (`termstructures::yields::DiscountCurve = InterpolatedDiscountCurve<LogLinear>`).
///
/// Extends [`PyYieldTermStructure`]; the first date is the reference date and
/// its discount must be 1.0. Unlike the other two curves this constructor
/// accepts an optional calendar. `interpolation` selects the discount-factor
/// interpolator: `"LogLinear"` (default, the shipped behaviour) or `"Cubic"`
/// (the Kruger cubic factory, non-monotonic). Finite in time: queries past the
/// last node require `enable_extrapolation()` or `extrapolate=True`.
#[pyclass(name = "DiscountCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyDiscountCurve;

#[pymethods]
impl PyDiscountCurve {
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

/// Python `ForwardCurve`: a yield curve built from (date, instantaneous
/// forward-rate) nodes, interpolating backward-flat
/// (`termstructures::yields::ForwardCurve = InterpolatedForwardCurve<BackwardFlat>`).
///
/// Extends [`PyYieldTermStructure`]; the first date is the reference date and
/// the query surface is inherited. Finite in time: queries past the last node
/// require `enable_extrapolation()` or `extrapolate=True`.
///
/// Unlike [`PyZeroCurve`] and [`PyDiscountCurve`], this curve offers no `Cubic`
/// interpolation option: QuantLib-SWIG exposes its cubic curve on the zero and
/// discount curves only, so the forward curve is intentionally left alone.
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

/// Python `PiecewiseYieldCurve`: a yield curve bootstrapped from a strip of
/// rate helpers, one curve node per helper maturity
/// (`termstructures::yields::PiecewiseYieldCurve<Discount, I>`).
///
/// Extends [`PyYieldTermStructure`]; every helper is solved so it reprices its
/// own market quote off the curve. This ergonomic string-dispatch alias covers
/// the `Discount` convention over the `LogLinear` (default) or `Linear`
/// interpolator selected by the `interpolation` string; both erase to the same
/// `Handle<dyn YieldTermStructure>`, so no discriminant is stored. The other
/// bootstrap conventions (`ZeroYield`, `ForwardRate`) are reached through the
/// named [`PyPiecewiseLinearZero`], [`PyPiecewiseLinearForward`] and
/// [`PyPiecewiseFlatForward`] classes, which also expose node introspection.
/// `(Discount, Linear)` deliberately gets no named class - QuantLib-SWIG has no
/// equivalent - so it stays reachable only through this alias's `"Linear"` arm.
///
/// The bootstrap is lazy: construction only rejects an empty helper list, and
/// the solver runs on the first query (a `discount`/`zero_rate`), re-running
/// after a helper-quote or evaluation-date change. A bootstrap failure surfaces
/// from those query methods (the inherited base maps them), not the
/// constructor; `max_date` swallows it and falls back to the reference date.
#[pyclass(name = "PiecewiseYieldCurve", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseYieldCurve;

#[pymethods]
impl PyPiecewiseYieldCurve {
    /// A curve over `helpers` with a fixed `reference_date` (typically the
    /// settlement date the caller computed via `Calendar.advance`). `helpers`
    /// accepts any [`RateHelper`](PyRateHelper) subclass; `interpolation` is
    /// `"LogLinear"` or `"Linear"`. Fallible: an empty helper list is rejected
    /// here, an unknown interpolation name too.
    #[new]
    #[pyo3(signature = (reference_date, helpers, day_counter, interpolation = "LogLinear"))]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyRateHelper>>,
        day_counter: &PyDayCounter,
        interpolation: &str,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn RateHelper>> =
            helpers.iter().map(|helper| helper.inner()).collect();
        let curve: Shared<dyn YieldTermStructure> = match interpolation {
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
            "Cubic" => {
                return Err(ItofinError::new_err(
                    "Cubic is a global interpolator (every node depends on all others), \
                     which the single-pass IterativeBootstrap cannot converge; the global \
                     convergence loop is unported (#543) and the core-side guard that would \
                     reject it is tracked separately (#552). Cubic is available on the \
                     standalone ZeroCurve and DiscountCurve instead.",
                ));
            }
            other => {
                return Err(ItofinError::new_err(format!(
                    "unknown interpolation {other:?}, expected LogLinear or Linear"
                )));
            }
        };
        Ok(PyClassInitializer::from(PyYieldTermStructure {
            inner: Handle::new(curve),
        })
        .add_subclass(PyPiecewiseYieldCurve))
    }
}

/// Python `PiecewiseLogLinearDiscount`: a curve bootstrapped in discount-factor
/// space with log-linear interpolation
/// (`PiecewiseYieldCurve<Discount, LogLinear>`).
///
/// The verbatim QuantLib-SWIG name for the blessed `(Discount, LogLinear)`
/// combination. Unlike the string-dispatch [`PyPiecewiseYieldCurve`] alias, the
/// named class retains the concrete curve so it can expose the bootstrapped
/// node introspection (`dates`, `data`) the erased handle discards. Its stored
/// `data()` are discount factors, so `data()[0]` is the reference node's `1.0`.
#[pyclass(name = "PiecewiseLogLinearDiscount", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseLogLinearDiscount {
    concrete: Shared<PiecewiseYieldCurve<Discount, LogLinear>>,
}

#[pymethods]
impl PyPiecewiseLogLinearDiscount {
    /// A curve over `helpers` with a fixed `reference_date`. `helpers` accepts
    /// any [`RateHelper`](PyRateHelper) subclass. Fallible: an empty helper list
    /// is rejected here.
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

    /// The bootstrapped node dates (triggers the lazy bootstrap).
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The bootstrapped node values, discount factors here (triggers the lazy
    /// bootstrap).
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

/// Python `PiecewiseLinearZero`: a curve bootstrapped in zero-rate space with
/// linear interpolation (`PiecewiseYieldCurve<ZeroYield, Linear>`).
///
/// The verbatim QuantLib-SWIG name for the blessed `(ZeroYield, Linear)`
/// combination. Its stored `data()` are continuously-compounded zero rates, so
/// `data()[0]` mirrors the first solved pillar's rate rather than a `1.0`
/// discount.
#[pyclass(name = "PiecewiseLinearZero", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseLinearZero {
    concrete: Shared<PiecewiseYieldCurve<ZeroYield, Linear>>,
}

#[pymethods]
impl PyPiecewiseLinearZero {
    /// A curve over `helpers` with a fixed `reference_date`. Fallible: an empty
    /// helper list is rejected here.
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

    /// The bootstrapped node dates (triggers the lazy bootstrap).
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The bootstrapped node values, zero rates here (triggers the lazy
    /// bootstrap).
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

/// Python `PiecewiseLinearForward`: a curve bootstrapped in instantaneous
/// forward-rate space with linear interpolation
/// (`PiecewiseYieldCurve<ForwardRate, Linear>`).
///
/// The verbatim QuantLib-SWIG name for the blessed `(ForwardRate, Linear)`
/// combination. Its stored `data()` are instantaneous forward rates.
#[pyclass(name = "PiecewiseLinearForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseLinearForward {
    concrete: Shared<PiecewiseYieldCurve<ForwardRate, Linear>>,
}

#[pymethods]
impl PyPiecewiseLinearForward {
    /// A curve over `helpers` with a fixed `reference_date`. Fallible: an empty
    /// helper list is rejected here.
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

    /// The bootstrapped node dates (triggers the lazy bootstrap).
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The bootstrapped node values, forward rates here (triggers the lazy
    /// bootstrap).
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}

/// Python `PiecewiseFlatForward`: a curve bootstrapped in instantaneous
/// forward-rate space with backward-flat interpolation
/// (`PiecewiseYieldCurve<ForwardRate, BackwardFlat>`).
///
/// The verbatim QuantLib-SWIG name for the blessed `(ForwardRate, BackwardFlat)`
/// combination. Piecewise-constant instantaneous forwards produce a curve
/// numerically identical to [`PyPiecewiseLogLinearDiscount`] under every
/// discount/zero/forward query (log-linear in discount space *is* piecewise
/// constant forwards); only the stored `data()` (forward rates vs discount
/// factors) tell the two apart.
#[pyclass(name = "PiecewiseFlatForward", extends = PyYieldTermStructure, unsendable)]
pub struct PyPiecewiseFlatForward {
    concrete: Shared<PiecewiseYieldCurve<ForwardRate, BackwardFlat>>,
}

#[pymethods]
impl PyPiecewiseFlatForward {
    /// A curve over `helpers` with a fixed `reference_date`. Fallible: an empty
    /// helper list is rejected here.
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

    /// The bootstrapped node dates (triggers the lazy bootstrap).
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The bootstrapped node values, forward rates here (triggers the lazy
    /// bootstrap).
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }
}
