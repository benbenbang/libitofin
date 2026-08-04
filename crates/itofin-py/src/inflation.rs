//! Facades for the inflation slice: the [`PyDiscountingSwapEngine`] every
//! inflation swap prices through, the [`PyCpiInterpolationType`] observation
//! flag and the [`PyZeroInflationIndex`] family.
//!
//! The swap engine is generic rather than inflation-specific - it prices any
//! swap - but is homed here because the inflation tickets are the first to need
//! it and are its only consumers today.

use crate::PyQlError;
use crate::curve::PyYieldTermStructure;
use crate::settings::PySettings;
use crate::time::PyDate;
use libitofin::handle::RelinkableHandle;
use libitofin::indexes::index::Index;
use libitofin::indexes::inflation::{EuHicp, UkHicp, UkRpi};
use libitofin::indexes::inflationindex::ZeroInflationIndex;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
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
