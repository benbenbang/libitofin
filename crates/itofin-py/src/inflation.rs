//! Facades for the inflation slice: the [`PyDiscountingSwapEngine`] every
//! inflation swap prices through, the [`PyCpiInterpolationType`] observation
//! flag, the [`PyZeroInflationIndex`] family and the
//! [`PyZeroInflationTermStructure`] curve hierarchy.
//!
//! The swap engine is generic rather than inflation-specific - it prices any
//! swap - but is homed here because the inflation tickets are the first to need
//! it and are its only consumers today.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::settings::PySettings;
use crate::time::{PyDate, PyDayCounter, PyFrequency};
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::index::Index;
use libitofin::indexes::inflation::{EuHicp, UkHicp, UkRpi};
use libitofin::indexes::inflationindex::ZeroInflationIndex;
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
use libitofin::termstructures::inflation::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve;
use pyo3::prelude::*;

/// Python `DiscountingSwapEngine`: discounts each leg of a swap over a single
/// yield curve (`pricingengines::swap::discountingswapengine`).
///
/// Infallible at construction: it stores the discount handle and the settings
/// and registers as an observer of the curve. Every precondition (an empty
/// handle, an unset evaluation date) is reported when the swap is priced.
///
/// Deferred (visible): the core's `include_settlement_date_flows`,
/// `settlement_date` and `npv_date` overrides
/// (`discountingswapengine.rs:58-63`) are not exposed and are always passed as
/// `None`, so the flow decision follows the settings' own flags and both dates
/// fall back to the curve reference date. That is the shape every ported
/// fixture uses.
///
/// The `settings` passed here must be the same object the swap this engine
/// prices was built with, or the two resolve their dates against different
/// evaluation dates and the NPV is silently wrong.
#[pyclass(name = "DiscountingSwapEngine", unsendable)]
pub struct PyDiscountingSwapEngine {
    inner: SharedMut<DiscountingSwapEngine>,
}

#[pymethods]
impl PyDiscountingSwapEngine {
    /// An engine discounting every leg on `discount`.
    #[new]
    fn new(discount: &PyYieldTermStructure, settings: &PySettings) -> Self {
        PyDiscountingSwapEngine {
            inner: shared_mut(DiscountingSwapEngine::new(
                discount.handle(),
                None,
                None,
                None,
                settings.inner(),
            )),
        }
    }
}

impl PyDiscountingSwapEngine {
    /// The erased engine the instrument facades install via
    /// `set_pricing_engine`.
    #[allow(dead_code)]
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

/// Python `CpiInterpolationType`: how an observation interpolates between the
/// index fixings bracketing it (core `indexes::CpiInterpolationType`).
///
/// A fieldless pyo3 enum exposing `CpiInterpolationType.Flat` /
/// `CpiInterpolationType.Linear`: `Flat` reads the fixing of the lagged period
/// outright, `Linear` advances from it to the next period's fixing by how far
/// the observation date has run into its own period.
///
/// The core's deprecated `AsIndex` variant is not ported and so has no
/// counterpart here.
#[pyclass(name = "CpiInterpolationType", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCpiInterpolationType {
    Flat,
    Linear,
}

/// Python `ZeroInflationIndex`: a price index publishing one level per period,
/// reading back either a stored figure or a forecast off its inflation curve
/// (core `indexes::inflationindex::ZeroInflationIndex`).
///
/// The curve is reached through a [`RelinkableHandle`] the facade owns. The
/// core's `with_term_structure` consumes the index and takes a plain
/// [`Handle`](libitofin::handle::Handle), so an index built against a curve
/// that does not exist yet could never be pointed at one later; linking the
/// index to this facade's own relinkable handle at construction moves that
/// choice to `link_to`, which is how the bootstrapped-curve fixtures need it.
/// The handle starts empty, exactly as the core's own constructor leaves it, so
/// a forecast before any link raises the empty-handle error.
///
/// `link_to`, which points that handle at a bootstrapped curve, lands with
/// `PiecewiseZeroInflationCurve`: there is no curve facade to link to yet.
#[pyclass(name = "ZeroInflationIndex", unsendable)]
pub struct PyZeroInflationIndex {
    inner: Shared<ZeroInflationIndex>,
    curve: RelinkableHandle<dyn ZeroInflationTermStructure>,
}

impl PyZeroInflationIndex {
    /// Wraps `build` over a fresh empty relinkable handle the index observes.
    fn with_curve_handle(
        build: impl FnOnce(&RelinkableHandle<dyn ZeroInflationTermStructure>) -> ZeroInflationIndex,
    ) -> Self {
        let curve = RelinkableHandle::<dyn ZeroInflationTermStructure>::empty();
        let inner = shared(build(&curve));
        PyZeroInflationIndex { inner, curve }
    }
}

#[pymethods]
impl PyZeroInflationIndex {
    /// The UK Retail Price Index: "RPI", monthly, one-month availability lag.
    #[staticmethod]
    fn uk_rpi(settings: &PySettings) -> Self {
        PyZeroInflationIndex::with_curve_handle(|curve| {
            UkRpi::new(settings.inner()).with_term_structure(curve.handle())
        })
    }

    /// The UK harmonised index of consumer prices.
    #[staticmethod]
    fn uk_hicp(settings: &PySettings) -> Self {
        PyZeroInflationIndex::with_curve_handle(|curve| {
            UkHicp::new(settings.inner()).with_term_structure(curve.handle())
        })
    }

    /// The euro-area harmonised index of consumer prices.
    #[staticmethod]
    fn eu_hicp(settings: &PySettings) -> Self {
        PyZeroInflationIndex::with_curve_handle(|curve| {
            EuHicp::new(settings.inner()).with_term_structure(curve.handle())
        })
    }

    /// The index name, e.g. `"UK RPI"`, under which fixings are stored.
    fn name(&self) -> String {
        self.inner.name()
    }

    /// Records a published figure across the whole inflation period it
    /// describes, so a later read on any day inside that period finds it.
    fn add_fixing(&self, fixing_date: &PyDate, value: f64) -> PyResult<()> {
        Ok(self
            .inner
            .add_fixing(fixing_date.inner(), value)
            .map_err(PyQlError::from)?)
    }

    /// The fixing at `fixing_date`, stored or forecast off the linked curve.
    ///
    /// `forecast_todays_fixing` is accepted and ignored, as in the core: the
    /// choice between history and forecast is made by
    /// [`needs_forecast`](Self::needs_forecast) alone. A date the store should
    /// cover but does not is an error rather than a forecast, and a forecast
    /// with no curve linked raises the empty-handle error.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// The first day of the inflation period the latest stored figure
    /// describes. An index with no history is an error.
    fn last_fixing_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.last_fixing_date().map_err(PyQlError::from)?,
        ))
    }

    /// Whether `fixing_date` has to be forecast rather than read from history,
    /// decided against the latest period that could have been published by the
    /// settings' evaluation date.
    fn needs_forecast(&self, fixing_date: &PyDate) -> PyResult<bool> {
        Ok(self
            .inner
            .needs_forecast(fixing_date.inner())
            .map_err(PyQlError::from)?)
    }

    fn __repr__(&self) -> String {
        format!("ZeroInflationIndex({})", self.inner.name())
    }
}

impl PyZeroInflationIndex {
    /// Points the internal handle at `curve`, so the index forecasts off it.
    ///
    /// The seam the `link_to` method calls once a zero inflation curve facade
    /// exists.
    #[allow(dead_code)]
    pub(crate) fn relink(&self, curve: Shared<dyn ZeroInflationTermStructure>) {
        self.curve.link_to(curve);
    }

    /// The wrapped core index, for the coupon and helper facades that take one.
    #[allow(dead_code)]
    pub(crate) fn shared(&self) -> Shared<ZeroInflationIndex> {
        Shared::clone(&self.inner)
    }
}

/// Python `ZeroInflationTermStructure`: the shared base for every zero-coupon
/// inflation curve (`termstructures::inflation::inflationtermstructure`).
///
/// Holds the erased `Handle<dyn ZeroInflationTermStructure>` and exposes the
/// query surface every concrete curve inherits, the shape
/// [`PyDefaultProbabilityTermStructure`](crate::credit) already uses on the
/// credit side. Concrete curves such as [`PyInterpolatedZeroInflationCurve`]
/// subclass this and supply only their constructor and node inspectors.
///
/// The two rate reads are not interchangeable.
/// [`zero_rate_date`](Self::zero_rate_date) snaps its date to the start of the
/// inflation period containing it before the curve sees it, because a fixing
/// applies to a whole period; [`zero_rate`](Self::zero_rate) takes a
/// year-fraction already measured under the curve's own day counter and
/// quantizes nothing. A mid-period date therefore reads that period's rate
/// through the first and an interpolated one through the second.
///
/// Deferred (visible): `base_rate()` is not exposed. A zero curve carries no
/// base rate, so the core accessor is an error on every curve reachable here
/// (`inflationtermstructure.rs:159-166`); it follows with the year-on-year
/// structures that do carry one.
#[pyclass(name = "ZeroInflationTermStructure", subclass, unsendable)]
pub struct PyZeroInflationTermStructure {
    inner: Handle<dyn ZeroInflationTermStructure>,
}

#[pymethods]
impl PyZeroInflationTermStructure {
    /// The zero-coupon inflation rate at year-fraction `t`, on the yearly
    /// compounding ZCIIS quotes assume.
    ///
    /// `t` must be measured with the curve's own day counter, and is negative
    /// for the base period. Nothing here accounts for observation lags or
    /// period interpolation: the caller manages those.
    #[pyo3(signature = (t, extrapolate = false))]
    fn zero_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .zero_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The zero-coupon inflation rate for the inflation period containing
    /// `date`.
    ///
    /// The date is quantized to that period's first day before both the range
    /// check and the time conversion, so every day inside one period reads the
    /// same rate.
    #[pyo3(signature = (date, extrapolate = false))]
    fn zero_rate_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .zero_rate_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The base date: the last date for which the fixing is known. It precedes
    /// the reference date, so its year-fraction is negative.
    fn base_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .base_date(),
        ))
    }

    /// The frequency of the inflation fixings the curve is built on.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .frequency(),
        )
    }
}

impl PyZeroInflationTermStructure {
    /// The base half of a concrete curve's [`PyClassInitializer`] chain.
    pub(crate) fn from_handle(inner: Handle<dyn ZeroInflationTermStructure>) -> Self {
        PyZeroInflationTermStructure { inner }
    }

    /// A clone of the inner curve handle for the coupon and instrument facades
    /// that take an inflation curve.
    #[allow(dead_code)]
    pub(crate) fn handle(&self) -> Handle<dyn ZeroInflationTermStructure> {
        self.inner.clone()
    }
}

/// Python `InterpolatedZeroInflationCurve`: a zero-coupon inflation curve built
/// from (date, zero-rate) nodes, interpolating linearly in zero-rate space
/// (`termstructures::inflation::InterpolatedZeroInflationCurve<Linear>`).
///
/// Extends [`PyZeroInflationTermStructure`], which carries the whole query
/// surface. The first date is the *base* date rather than the reference date,
/// which is passed separately and normally follows it; node times are measured
/// from the reference date, so the first one is negative. That is the
/// divergence from the yield-side [`ZeroCurve`](crate::curve::PyZeroCurve),
/// whose first node is its own reference date at time zero.
///
/// The concrete curve is retained alongside the erased handle so the node
/// inspectors [`times`](Self::times), [`dates`](Self::dates) and
/// [`nodes`](Self::nodes) stay reachable, the shape
/// [`PyInterpolatedHazardRateCurve`](crate::credit) uses.
///
/// Fallible: the core rejects fewer than two dates, a dates/rates count
/// mismatch, a rate at or below -100 % from the second node on, and unsorted
/// dates (`interpolatedzeroinflationcurve.rs:95-106`), each as
/// [`struct@crate::ItofinError`].
///
/// `Linear` is pinned at the boundary: it is the interpolator C++'s
/// `ZeroInflationCurve` typedef fixes (`interpolatedzeroinflationcurve.rs:74`),
/// so no interpolation argument is offered.
///
/// Deferred (visible): the core's `rates()` / `data()` inspectors are omitted,
/// both being the rate half of [`nodes`](Self::nodes); and the curve carries no
/// seasonality, which the core does not port either.
#[pyclass(
    name = "InterpolatedZeroInflationCurve",
    extends = PyZeroInflationTermStructure,
    unsendable
)]
pub struct PyInterpolatedZeroInflationCurve {
    concrete: Shared<InterpolatedZeroInflationCurve<Linear>>,
}

#[pymethods]
impl PyInterpolatedZeroInflationCurve {
    /// A curve through the `rates` quoted at `dates`, with `dates[0]` as the
    /// base date and `reference_date` given separately.
    #[new]
    fn new(
        reference_date: &PyDate,
        dates: Vec<PyRef<PyDate>>,
        rates: Vec<f64>,
        frequency: &PyFrequency,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|date| date.inner()).collect();
        let concrete = shared(
            InterpolatedZeroInflationCurve::new(
                reference_date.inner(),
                dates,
                rates,
                frequency.inner(),
                day_counter.inner(),
                Linear,
            )
            .map_err(PyQlError::from)?,
        );
        let erased = Shared::clone(&concrete) as Shared<dyn ZeroInflationTermStructure>;
        Ok(
            PyClassInitializer::from(PyZeroInflationTermStructure::from_handle(Handle::new(
                erased,
            )))
            .add_subclass(PyInterpolatedZeroInflationCurve { concrete }),
        )
    }

    /// The node times, measured from the reference date; the first is negative
    /// whenever the base date precedes it.
    fn times(&self) -> Vec<f64> {
        self.concrete.times().to_vec()
    }

    /// The node dates, the first of which is the base date.
    fn dates(&self) -> Vec<PyDate> {
        self.concrete
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }

    /// The `(date, zero-rate)` nodes.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.concrete
            .nodes()
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect()
    }
}
