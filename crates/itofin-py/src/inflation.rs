//! Facades for the inflation slice: the [`PyDiscountingSwapEngine`] every
//! inflation swap prices through, the [`PyCpiInterpolationType`] observation
//! flag, the [`PyZeroInflationIndex`] family, the
//! [`PyZeroInflationTermStructure`] curve hierarchy, the
//! [`PyMultiplicativePriceSeasonality`] correction any of its curves can carry,
//! the [`PyZeroInflationHelper`] bootstrap helpers that fit its piecewise member
//! and the [`PyZeroCouponInflationSwap`] the rest of them price together, plus
//! the year-on-year family that mirrors it: the
//! [`PyYoYInflationTermStructure`] curve hierarchy and the
//! [`PyYoYInflationHelper`] helpers that fit its piecewise member, and the
//! optionlet stack written over that curve: the flat
//! [`PyConstantYoYOptionletVolatility`] surface, the
//! [`PyYoYInflationCapFloorEngine`] pricing against it and the
//! [`PyYoYInflationCapFloor`] it prices, which
//! [`PyMakeYoYInflationCapFloor`] is the only way to build, and the quoted
//! [`PyYoYCapFloorTermPriceSurface`] cap/floor price grid the optionlet
//! strippers read.
//!
//! The swap engine is generic rather than inflation-specific - it prices any
//! swap - but is homed here because the inflation tickets are the first to need
//! it and are its only consumers today.

use crate::PyQlError;
use crate::capfloor::PyCapFloorType;
use crate::cashflows::PyYoYInflationCoupon;
use crate::curve::PyYieldTermStructure;
use crate::helpers::PyPillar;
use crate::market::PySimpleQuote;
use crate::results::Results;
use crate::settings::PySettings;
use crate::swap::PySwapType;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod, PySchedule,
};
use libitofin::cashflows::YoYOptionletDistribution;
use libitofin::currency::Currency;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::index::Index;
use libitofin::indexes::inflation::{EuHicp, UkHicp, UkRpi};
use libitofin::indexes::inflationindex::{
    CpiInterpolationType, YoYInflationIndex, ZeroInflationIndex,
};
use libitofin::indexes::region::Region;
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    MakeYoYInflationCapFloor, YearOnYearInflationSwap, YoYInflationCapFloor,
    ZeroCouponInflationSwap,
};
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{DiscountingSwapEngine, YoYInflationCapFloorEngine};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::inflationhelpers::{
    YearOnYearInflationSwapHelper, YoYInflationHelper, ZeroCouponInflationSwapHelper,
    ZeroInflationHelper,
};
use libitofin::termstructures::inflation::inflationtermstructure::{
    YoYInflationTermStructure, ZeroInflationTermStructure,
};
use libitofin::termstructures::inflation::interpolatedyoyinflationcurve::InterpolatedYoYInflationCurve;
use libitofin::termstructures::inflation::interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve;
use libitofin::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve;
use libitofin::termstructures::inflation::piecewisezeroinflationcurve::PiecewiseZeroInflationCurve;
use libitofin::termstructures::inflation::seasonality::{
    MultiplicativePriceSeasonality, Seasonality,
};
use libitofin::termstructures::inflation::yoycapfloortermpricesurface::{
    InterpolatedYoYCapFloorTermPriceSurface, YoYCapFloorTermPriceSurface,
};
use libitofin::termstructures::volatility::{
    ConstantYoYOptionletVolatility, YoYOptionletVolatilitySurface,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::date::Date;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::period::Period;
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

impl PyCpiInterpolationType {
    /// The core [`CpiInterpolationType`] this variant stands for.
    pub(crate) fn inner(&self) -> CpiInterpolationType {
        match self {
            PyCpiInterpolationType::Flat => CpiInterpolationType::Flat,
            PyCpiInterpolationType::Linear => CpiInterpolationType::Linear,
        }
    }

    /// The variant standing for a core [`CpiInterpolationType`] a facade read
    /// back off an object it built.
    pub(crate) fn from_inner(inner: CpiInterpolationType) -> Self {
        match inner {
            CpiInterpolationType::Flat => PyCpiInterpolationType::Flat,
            CpiInterpolationType::Linear => PyCpiInterpolationType::Linear,
        }
    }
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

    /// Points the index at `curve`, so every forecast from here on compounds
    /// off it.
    ///
    /// Takes the [`PyZeroInflationTermStructure`] base, so any subclass links:
    /// the directly-built [`PyInterpolatedZeroInflationCurve`] as much as the
    /// bootstrapped curves that follow. The base's handle is filled at
    /// construction, so it always dereferences here.
    ///
    /// It is the curve *behind* `curve`'s handle at call time that is stored,
    /// not the handle itself: relinking the facade afterwards leaves this index
    /// on the curve it was given, and a later `link_to` is how it moves.
    ///
    /// # Errors
    ///
    /// Reports the empty-handle error if `curve` somehow carries no link.
    fn link_to(&self, curve: &PyZeroInflationTermStructure) -> PyResult<()> {
        self.relink(curve.handle().current_link().map_err(PyQlError::from)?);
        Ok(())
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
    /// The seam [`link_to`](Self::link_to) dereferences a curve facade into.
    pub(crate) fn relink(&self, curve: Shared<dyn ZeroInflationTermStructure>) {
        self.curve.link_to(curve);
    }

    /// The wrapped core index, for the coupon and helper facades that take one.
    pub(crate) fn shared(&self) -> Shared<ZeroInflationIndex> {
        Shared::clone(&self.inner)
    }
}

/// Python `MultiplicativePriceSeasonality`: the seasonal correction a price
/// index carries, whose factors multiply the index level itself
/// (`termstructures::inflation::seasonality::MultiplicativePriceSeasonality`).
///
/// Seasonality fills an inflation curve in between the integer-year maturities
/// the market quotes. The factors are given in whole multiples of the count the
/// frequency dictates - twelve for [`Frequency.Monthly`](crate::time::PyFrequency)
/// - and are reused as long as needed, so twelve of them are stationary and
/// twenty-four repeat every two years. They are not applied raw: the factor at
/// the queried date is normalized against the one at a reference date, which for
/// a zero rate is the curve's own base date, so the correction is the identity
/// there.
///
/// Install it with
/// [`ZeroInflationTermStructure.set_seasonality`](PyZeroInflationTermStructure::set_seasonality).
/// Only the date-taking rate query folds the correction in; the year-fraction
/// one cannot, a time not naming the date the factors are a function of.
///
/// Fallible at construction: the core rejects a frequency outside
/// semiannual-through-daily - `Frequency.Annual` among them - an empty factor
/// set, and a factor count that is not a whole multiple of the frequency, each
/// as [`struct@crate::ItofinError`].
///
/// Deferred (visible): the core's `set`, which replaces the whole
/// specification in place, is not exposed - building a new object and
/// installing it says the same thing without a second mutation path. The core's
/// `is_consistent` is not exposed either: it is what `set_seasonality` runs as
/// its gate, and is reported from there.
#[pyclass(name = "MultiplicativePriceSeasonality", unsendable)]
pub struct PyMultiplicativePriceSeasonality {
    inner: Shared<MultiplicativePriceSeasonality>,
}

#[pymethods]
impl PyMultiplicativePriceSeasonality {
    /// The seasonality whose `seasonality_factors` start at
    /// `seasonality_base_date` and step at `frequency`.
    #[new]
    fn new(
        seasonality_base_date: &PyDate,
        frequency: &PyFrequency,
        seasonality_factors: Vec<f64>,
    ) -> PyResult<Self> {
        Ok(PyMultiplicativePriceSeasonality {
            inner: shared(
                MultiplicativePriceSeasonality::new(
                    seasonality_base_date.inner(),
                    frequency.inner(),
                    seasonality_factors,
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// The date the factor set is anchored on.
    fn seasonality_base_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.seasonality_base_date())
    }

    /// The frequency the factors step at.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(self.inner.frequency())
    }

    /// The factors, in order from the seasonality base date.
    fn seasonality_factors(&self) -> Vec<f64> {
        self.inner.seasonality_factors().to_vec()
    }

    /// The raw factor covering `to`, before any normalization against a
    /// reference date - not the correction the curve applies.
    ///
    /// The offset from the seasonality base date is counted in whole factor
    /// periods and wrapped modulo the factor count, so a set shorter than the
    /// span repeats and dates before the anchor wrap backwards.
    ///
    /// # Errors
    ///
    /// Reports a year-based factor period, which cannot express seasonality.
    fn seasonality_factor(&self, to: &PyDate) -> PyResult<f64> {
        Ok(self
            .inner
            .seasonality_factor(to.inner())
            .map_err(PyQlError::from)?)
    }
}

impl PyMultiplicativePriceSeasonality {
    /// The upcast correction, for the curve facade that installs one.
    pub(crate) fn shared(&self) -> Shared<dyn Seasonality> {
        Shared::clone(&self.inner) as Shared<dyn Seasonality>
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
/// [`set_seasonality`](Self::set_seasonality) lives here rather than on either
/// concrete curve: it is an `InflationTermStructure` method reached through the
/// erased handle, and the handle holds the very object the subclass wraps, so
/// the one method serves both.
///
/// Deferred (visible): `base_rate()` is not exposed. A zero curve carries no
/// base rate, so the core accessor is an error on every curve reachable here
/// (`inflationtermstructure.rs:159-166`); it follows with the year-on-year
/// structures that do carry one. The core's `seasonality()` getter is omitted
/// too: it hands back an erased `Shared<dyn Seasonality>`, and the trait carries
/// no downcast surface to recover the concrete
/// [`PyMultiplicativePriceSeasonality`] from, so the getter could only return
/// something lossy. [`has_seasonality`](Self::has_seasonality) answers the
/// question the fixtures ask, and the caller already holds the object it
/// installed.
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

    /// Installs `seasonality` on the curve, replacing whatever it carried;
    /// `None` clears it.
    ///
    /// A curve that caches anything derived from the correction - every
    /// bootstrapped one does - is invalidated here, so the next read re-solves
    /// against the new correction.
    ///
    /// # Errors
    ///
    /// Reports the consistency gate, which a multi-year factor set fails: the
    /// whole-year comparison deciding those is a documented core deferral
    /// (#807). The store happens *before* the gate runs, as C++'s does, so a
    /// rejected correction is left installed and unannounced - clear it with
    /// `None` before reading the curve again.
    #[pyo3(signature = (seasonality))]
    fn set_seasonality(
        &self,
        seasonality: Option<&PyMultiplicativePriceSeasonality>,
    ) -> PyResult<()> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .set_seasonality(seasonality.map(PyMultiplicativePriceSeasonality::shared))
            .map_err(PyQlError::from)?)
    }

    /// Whether the curve carries a seasonality correction. Reports one left
    /// installed by a [`set_seasonality`](Self::set_seasonality) that raised.
    fn has_seasonality(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .has_seasonality())
    }
}

impl PyZeroInflationTermStructure {
    /// The base half of a concrete curve's [`PyClassInitializer`] chain.
    pub(crate) fn from_handle(inner: Handle<dyn ZeroInflationTermStructure>) -> Self {
        PyZeroInflationTermStructure { inner }
    }

    /// A clone of the inner curve handle for the coupon and instrument facades
    /// that take an inflation curve.
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
/// both being the rate half of [`nodes`](Self::nodes).
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
                None,
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

/// Python `ZeroInflationHelper`: the shared base for every zero-inflation
/// bootstrap helper
/// (`termstructures::inflation::inflationhelpers::ZeroInflationHelper`).
///
/// Holds the erased `Shared<dyn ZeroInflationHelper>` and exposes the two dates
/// the bootstrap places a curve node by, the shape
/// [`PyDefaultProbabilityHelper`](crate::credithelpers::PyDefaultProbabilityHelper)
/// already uses on the credit side. Concrete helpers such as
/// [`PyZeroCouponInflationSwapHelper`] subclass this and supply only their
/// constructor.
///
/// Deferred (visible): the core trait's `earliest_date`, `maturity_date` and
/// `latest_relevant_date` are not exposed. On the one concrete helper reachable
/// here all five dates collapse onto the same fixing-period start
/// (`inflationhelpers.rs:288-290`), so the three would report exactly what
/// [`pillar_date`](Self::pillar_date) reports; they follow with a helper whose
/// dates straddle a period.
#[pyclass(name = "ZeroInflationHelper", subclass, unsendable)]
pub struct PyZeroInflationHelper {
    inner: Shared<dyn ZeroInflationHelper>,
}

#[pymethods]
impl PyZeroInflationHelper {
    /// The pillar date, at which the curve node this helper sets sits.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// The latest date the helper needs curve data at (equal to the pillar
    /// date).
    fn latest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_date())
    }
}

impl PyZeroInflationHelper {
    /// The base half of a concrete helper's [`PyClassInitializer`] chain.
    pub(crate) fn from_shared(inner: Shared<dyn ZeroInflationHelper>) -> Self {
        PyZeroInflationHelper { inner }
    }

    /// A clone of the upcast helper, for the piecewise inflation-curve facade,
    /// which takes a list of helpers and threads each into the bootstrap.
    pub(crate) fn shared(&self) -> Shared<dyn ZeroInflationHelper> {
        Shared::clone(&self.inner)
    }
}

/// Python `ZeroCouponInflationSwapHelper`: the bootstrap helper fitting a
/// zero-coupon inflation swap quoted as a rate
/// (`termstructures::inflation::inflationhelpers::ZeroCouponInflationSwapHelper`).
///
/// The helper prices a unit-notional, zero-strike swap of its own and reports
/// that contract's fair rate; the bootstrap drives the quoted rate less that
/// fair rate to zero. It needs no nominal curve, building itself a flat 0 % one,
/// because both legs pay on the same adjusted maturity and their discount
/// factors cancel out of the fair rate.
///
/// The swap starts at the evaluation date held by `settings` and is rebuilt
/// whenever that date moves, so the evaluation date must be set *before* the
/// constructor runs, not merely before the bootstrap. The helper retains the
/// caller's [`PySimpleQuote`], so a later `set_value` re-drives the bootstrap.
///
/// It prices through a *copy* of `index` linked to a handle of its own, which
/// the bootstrap points at the curve under construction; the caller's index
/// keeps whatever curve it had and need not be linked at all.
///
/// `pillar` picks which of the two nodes an interpolated swap straddles the
/// helper fits; a flat swap reads a single fixing and ignores it.
///
/// Fallible: the core rejects an observation lag the index cannot observe
/// through, the swap being built here too, and under
/// [`CpiInterpolationType.Linear`](PyCpiInterpolationType) one that leaves less
/// than a whole index period over the index's availability lag, interpolation
/// reading the month after the one the lag lands in. Both raise
/// [`struct@crate::ItofinError`].
#[pyclass(
    name = "ZeroCouponInflationSwapHelper",
    extends = PyZeroInflationHelper,
    unsendable
)]
pub struct PyZeroCouponInflationSwapHelper {
    concrete: Shared<ZeroCouponInflationSwapHelper>,
}

#[pymethods]
impl PyZeroCouponInflationSwapHelper {
    /// A helper fitting `quote` on a swap maturing at `maturity`.
    #[new]
    #[pyo3(signature = (
        quote,
        swap_obs_lag,
        maturity,
        calendar,
        payment_convention,
        day_counter,
        index,
        observation_interpolation,
        settings,
        pillar = PyPillar::LastRelevantDate,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: &PySimpleQuote,
        swap_obs_lag: &PyPeriod,
        maturity: &PyDate,
        calendar: &PyCalendar,
        payment_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        index: &PyZeroInflationIndex,
        observation_interpolation: &PyCpiInterpolationType,
        settings: &PySettings,
        pillar: PyPillar,
    ) -> PyResult<PyClassInitializer<Self>> {
        let concrete = ZeroCouponInflationSwapHelper::new(
            quote.handle(),
            swap_obs_lag.inner(),
            maturity.inner(),
            calendar.inner(),
            payment_convention.inner(),
            day_counter.inner(),
            &index.shared(),
            observation_interpolation.inner(),
            pillar.inner(),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn ZeroInflationHelper>;
        Ok(
            PyClassInitializer::from(PyZeroInflationHelper::from_shared(erased))
                .add_subclass(PyZeroCouponInflationSwapHelper { concrete }),
        )
    }

    /// The date the maturity fixing is observed at on the helper's own swap:
    /// `maturity` less the observation lag, unsnapped to its inflation period.
    ///
    /// Read off the cached contract's indexed flow, so it reports the date the
    /// helper actually prices at rather than one recomputed here. It is *not*
    /// [`pillar_date`](PyZeroInflationHelper::pillar_date), which is the first
    /// day of the period containing it - the quantization being the helper's
    /// rounding and not the contract's.
    ///
    /// Fallible: the cached swap carries the error that stopped it being built,
    /// notably an evaluation date that was never set.
    fn inflation_fixing_date(&self) -> PyResult<PyDate> {
        let swap = self.concrete.swap();
        let swap = swap
            .as_ref()
            .map_err(|error| PyQlError::from(error.clone()))?;
        Ok(PyDate::from_inner(swap.inflation_cash_flow().fixing_date()))
    }
}

/// Python `PiecewiseZeroInflationCurve`: a zero-coupon inflation curve
/// bootstrapped from inflation helpers, solving one zero-rate node per helper
/// (`termstructures::inflation::PiecewiseZeroInflationCurve<Linear>`).
///
/// Extends [`PyZeroInflationTermStructure`], which carries the whole query
/// surface. Each helper's observed fixing period marks a segment boundary, and
/// its node is solved so the helper reprices its own quote off the curve.
///
/// Node zero sits on `base_date`, not on `reference_date`: the base date is the
/// last date for which a fixing is known - in practice
/// [`ZeroInflationIndex.last_fixing_date`](PyZeroInflationIndex::last_fixing_date)
/// - and precedes the reference date, so [`times`](Self::times)`[0]` is
/// negative. That is the one structural difference from every other piecewise
/// curve, whose first node is its own reference date at time zero.
///
/// Lazy, like the credit-side [`PyPiecewiseDefaultCurve`](crate::credit): the
/// constructor only registers on the helpers, and the bootstrap runs on the
/// first read - a query, an inspector, or an explicit
/// [`calculate`](Self::calculate). A helper quote moving invalidates the cache,
/// so the next read re-bootstraps. The evaluation date must therefore be in
/// place before that first read as well as before the helpers were built.
///
/// The concrete curve is retained alongside the erased handle so the node
/// inspectors stay reachable, as [`PyInterpolatedZeroInflationCurve`] and
/// [`PyPiecewiseDefaultCurve`](crate::credit) both do.
///
/// Fallible: the core rejects an empty helper set, and every inspector
/// propagates a bootstrap failure as [`struct@crate::ItofinError`].
///
/// `Linear` is pinned at the boundary: it is the only interpolator the core
/// constructs (`piecewisezeroinflationcurve.rs:105-125`), so no interpolation
/// argument is offered.
///
/// A seasonality installed through
/// [`set_seasonality`](PyZeroInflationTermStructure::set_seasonality)
/// invalidates the bootstrap, so the next read re-solves every node against the
/// correction and the quoted swaps still reprice to nothing.
///
/// Deferred (visible): the core's `data()` inspector is omitted, being the rate
/// half of [`nodes`](Self::nodes) - the choice
/// [`PyInterpolatedZeroInflationCurve`] already made on this side, where the
/// credit curve exposes both.
#[pyclass(
    name = "PiecewiseZeroInflationCurve",
    extends = PyZeroInflationTermStructure,
    unsendable
)]
pub struct PyPiecewiseZeroInflationCurve {
    concrete: Shared<PiecewiseZeroInflationCurve<Linear>>,
}

#[pymethods]
impl PyPiecewiseZeroInflationCurve {
    /// A curve over `helpers` with a fixed `reference_date` and a `base_date`
    /// preceding it. `helpers` accepts any
    /// [`ZeroInflationHelper`](PyZeroInflationHelper) subclass.
    #[new]
    fn new(
        reference_date: &PyDate,
        base_date: &PyDate,
        frequency: &PyFrequency,
        day_counter: &PyDayCounter,
        helpers: Vec<PyRef<PyZeroInflationHelper>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn ZeroInflationHelper>> =
            helpers.iter().map(|helper| helper.shared()).collect();
        let concrete = PiecewiseZeroInflationCurve::<Linear>::new(
            reference_date.inner(),
            base_date.inner(),
            frequency.inner(),
            day_counter.inner(),
            instruments,
            None,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn ZeroInflationTermStructure>;
        Ok(
            PyClassInitializer::from(PyZeroInflationTermStructure::from_handle(Handle::new(
                erased,
            )))
            .add_subclass(PyPiecewiseZeroInflationCurve { concrete }),
        )
    }

    /// Runs the bootstrap if the cache is stale, so a solver failure surfaces
    /// here rather than inside a later query.
    fn calculate(&self) -> PyResult<()> {
        Ok(self.concrete.calculate().map_err(PyQlError::from)?)
    }

    /// The node times, measured from the reference date in the curve's own day
    /// count; the first is negative (triggers the bootstrap).
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.times().map_err(PyQlError::from)?)
    }

    /// The node dates, the first of which is the base date (triggers the
    /// bootstrap).
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The solved `(date, zero-rate)` nodes (triggers the bootstrap).
    fn nodes(&self) -> PyResult<Vec<(PyDate, f64)>> {
        Ok(self
            .concrete
            .nodes()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect())
    }
}

/// Python `ZeroCouponInflationSwap`: one fixed flow against one
/// inflation-indexed flow, both exchanged at maturity
/// (`instruments::zerocouponinflationswap`).
///
/// The quoted `fixed_rate` is the `K` that at inception matches the inflation
/// growth. [`SwapType`](crate::swap::PySwapType) names the *inflation* leg, so a
/// `Payer` pays inflation and receives fixed.
///
/// `maturity` is pre-adjustment: each leg's payment date is it rolled on that
/// leg's calendar and convention, while the year fraction behind the fixed
/// amount stays on the raw date. `inflation_calendar` and
/// `inflation_convention` fall back to the fixed-leg ones when `None`, and are
/// stored resolved.
///
/// Fallible at construction: the observation lag must let the index observe
/// fixings that exist, which under
/// [`Linear`](PyCpiInterpolationType::Linear) costs a further publication
/// period (`zerocouponinflationswap.rs:156-176`).
///
/// Pricing needs an engine: call [`set_engine`](Self::set_engine) before
/// [`npv`](Self::npv). [`fair_rate`](Self::fair_rate) is the exception - it
/// reads the indexed flow directly and prices with no engine at all - but it
/// does need the index linked to a curve.
///
/// Deferred (visible): the core omits `adjust_inf_obs_dates` from its own
/// signature, so there is nothing to expose here; the leg and cash-flow
/// accessors are not surfaced either, there being no cash-flow facade, and
/// [`inflation_fixing_date`](Self::inflation_fixing_date) carries the one datum
/// off the indexed flow that the fixtures read.
#[pyclass(name = "ZeroCouponInflationSwap", unsendable)]
pub struct PyZeroCouponInflationSwap {
    inner: SharedMut<ZeroCouponInflationSwap>,
}

#[pymethods]
impl PyZeroCouponInflationSwap {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        swap_type,
        nominal,
        start_date,
        maturity,
        fixed_calendar,
        fixed_convention,
        day_counter,
        fixed_rate,
        inflation_index,
        observation_lag,
        observation_interpolation,
        inflation_calendar,
        inflation_convention,
        settings,
    ))]
    fn new(
        swap_type: &PySwapType,
        nominal: f64,
        start_date: &PyDate,
        maturity: &PyDate,
        fixed_calendar: &PyCalendar,
        fixed_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        fixed_rate: f64,
        inflation_index: &PyZeroInflationIndex,
        observation_lag: &PyPeriod,
        observation_interpolation: &PyCpiInterpolationType,
        inflation_calendar: Option<&PyCalendar>,
        inflation_convention: Option<PyBusinessDayConvention>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(PyZeroCouponInflationSwap {
            inner: shared_mut(
                ZeroCouponInflationSwap::new(
                    swap_type.inner(),
                    nominal,
                    start_date.inner(),
                    maturity.inner(),
                    fixed_calendar.inner(),
                    fixed_convention.inner(),
                    day_counter.inner(),
                    fixed_rate,
                    inflation_index.shared(),
                    observation_lag.inner(),
                    observation_interpolation.inner(),
                    inflation_calendar.map(PyCalendar::inner),
                    inflation_convention.map(|convention| convention.inner()),
                    settings.inner(),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Attaches a [`PyDiscountingSwapEngine`] so the swap prices.
    ///
    /// The engine must resolve its dates against the same `Settings` object
    /// this swap was built with.
    fn set_engine(&mut self, engine: &PyDiscountingSwapEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Forces the valuation, idempotent and fallible as
    /// [`VanillaOption.calculate`](crate::option::PyVanillaOption::calculate).
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Whether the cached results are currently valid.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Attaches `engine` and returns the NPV, the one-shot form of
    /// [`set_engine`](Self::set_engine) followed by [`npv`](Self::npv).
    fn price(&mut self, engine: &PyDiscountingSwapEngine) -> PyResult<f64> {
        self.set_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// A frozen [`Results`] copy of the valuation, calculating first.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// The swap NPV under the attached engine.
    ///
    /// Fallible: with no engine attached the core reports `"null pricing
    /// engine"`, and with no curve linked into the index the indexed flow
    /// cannot be forecast.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// The rate that would price the swap at zero: the index ratio
    /// `I(T)/I(0)` de-compounded over the swap's own year fraction.
    ///
    /// Needs no engine - it reads the indexed flow rather than any priced
    /// result - but does need the index linked, the flow's amount being a
    /// forecast off the inflation curve.
    fn fair_rate(&self) -> PyResult<f64> {
        Ok(self.inner.borrow().fair_rate().map_err(PyQlError::from)?)
    }

    /// The fixed leg's NPV, priced on demand. Fallible as [`npv`](Self::npv).
    fn fixed_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// The inflation leg's NPV, priced on demand. Fallible as
    /// [`npv`](Self::npv).
    fn inflation_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .inflation_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// The fixed leg's sensitivity to a basis point on the quoted rate,
    /// computed in closed form rather than read off the engine, whose own leg
    /// BPS is zero for a non-coupon flow.
    ///
    /// Fallible as [`npv`](Self::npv): it needs the engine's discount factor at
    /// the fixed leg's end date.
    fn fixed_leg_bps(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_leg_bps()
            .map_err(PyQlError::from)?)
    }

    /// The contract maturity, raw and pre-adjustment - not either leg's
    /// payment date.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.borrow().maturity_date())
    }

    /// The date the maturity fixing is observed at, `maturity` less the
    /// observation lag, unsnapped.
    fn obs_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.borrow().obs_date())
    }

    /// The same date as [`obs_date`](Self::obs_date), read off the indexed flow
    /// rather than off the swap: the core's `obs_date()` is that call
    /// (`zerocouponinflationswap.rs:315-317`). Both names are kept because both
    /// exist in the core, and the oracle asserts they coincide.
    fn inflation_fixing_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.borrow().inflation_cash_flow().fixing_date())
    }
}

/// Python `YoYInflationTermStructure`: the shared base for every year-on-year
/// inflation curve (`termstructures::inflation::inflationtermstructure`).
///
/// Holds the erased `Handle<dyn YoYInflationTermStructure>` and exposes the
/// query surface every concrete curve inherits, the shape
/// [`PyZeroInflationTermStructure`] already uses on the zero side. Concrete
/// curves such as [`PyInterpolatedYoYInflationCurve`] subclass this and supply
/// only their constructor and node inspectors.
///
/// The two rate reads are not interchangeable, exactly as on the zero side.
/// [`yoy_rate_date`](Self::yoy_rate_date) snaps its date to the start of the
/// inflation period containing it before the curve sees it, and is the only one
/// that folds in a seasonality correction;
/// [`yoy_rate`](Self::yoy_rate) takes a year-fraction already measured under the
/// curve's own day counter and quantizes nothing.
///
/// [`base_rate`](Self::base_rate) is exposed here where the zero base defers it:
/// a year-on-year curve carries the rate observed over the period ending on its
/// base date, C++ forwarding `baseYoYRate` into the base constructor's
/// `baseRate` slot (`inflationtermstructure.rs:198`), so every curve reachable
/// through this facade answers rather than raising.
///
/// Deferred (visible): the core's `seasonality()` getter, for the reason
/// [`PyZeroInflationTermStructure`] omits it - it hands back an erased
/// `Shared<dyn Seasonality>` there is no downcast surface to recover the
/// concrete correction from.
#[pyclass(name = "YoYInflationTermStructure", subclass, unsendable)]
pub struct PyYoYInflationTermStructure {
    inner: Handle<dyn YoYInflationTermStructure>,
}

#[pymethods]
impl PyYoYInflationTermStructure {
    /// The year-on-year inflation rate at year-fraction `t`.
    ///
    /// `t` must be measured with the curve's own day counter, and is negative
    /// for the base period. This is not the year-on-year swap rate, which comes
    /// from the instrument.
    #[pyo3(signature = (t, extrapolate = false))]
    fn yoy_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .yoy_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The year-on-year inflation rate for the inflation period containing
    /// `date`.
    ///
    /// The date is quantized to that period's first day before both the range
    /// check and the time conversion, so every day inside one period reads the
    /// same rate. Any seasonality correction is folded in last, at the
    /// *original* date rather than the period start, as C++ does on this path.
    #[pyo3(signature = (date, extrapolate = false))]
    fn yoy_rate_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .yoy_rate_date(date.inner(), extrapolate)
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

    /// The year-on-year rate observed over the period ending on the base date,
    /// which node zero is seeded with and keeps.
    fn base_rate(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .base_rate()
            .map_err(PyQlError::from)?)
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

    /// Installs `seasonality` on the curve, replacing whatever it carried;
    /// `None` clears it.
    ///
    /// # Errors
    ///
    /// Reports the consistency gate, and leaves a rejected correction installed
    /// as C++ does - see [`PyZeroInflationTermStructure::set_seasonality`].
    #[pyo3(signature = (seasonality))]
    fn set_seasonality(
        &self,
        seasonality: Option<&PyMultiplicativePriceSeasonality>,
    ) -> PyResult<()> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .set_seasonality(seasonality.map(PyMultiplicativePriceSeasonality::shared))
            .map_err(PyQlError::from)?)
    }

    /// Whether the curve carries a seasonality correction.
    fn has_seasonality(&self) -> PyResult<bool> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .has_seasonality())
    }
}

impl PyYoYInflationTermStructure {
    /// The base half of a concrete curve's [`PyClassInitializer`] chain.
    pub(crate) fn from_handle(inner: Handle<dyn YoYInflationTermStructure>) -> Self {
        PyYoYInflationTermStructure { inner }
    }

    /// A clone of the inner curve handle for the index facade that links one.
    pub(crate) fn handle(&self) -> Handle<dyn YoYInflationTermStructure> {
        self.inner.clone()
    }
}

/// Python `InterpolatedYoYInflationCurve`: a year-on-year inflation curve built
/// from (date, year-on-year-rate) nodes, interpolating linearly in rate space
/// (`termstructures::inflation::InterpolatedYoYInflationCurve<Linear>`).
///
/// Extends [`PyYoYInflationTermStructure`], which carries the whole query
/// surface. The first date is the *base* date rather than the reference date,
/// which is passed separately and normally follows it; the first rate is the
/// base rate the curve publishes, and node times are measured from the
/// reference date, so the first one is negative.
///
/// The concrete curve is retained alongside the erased handle so the node
/// inspectors stay reachable, as [`PyInterpolatedZeroInflationCurve`] does.
///
/// Fallible: the core rejects fewer than two dates, a dates/rates count
/// mismatch and, from the second node on, a rate at or below -100 %, the base
/// rate being left unconstrained
/// (`interpolatedyoyinflationcurve.rs:100-117`).
///
/// `Linear` is pinned at the boundary: it is the interpolator C++'s
/// `YoYInflationCurve` typedef fixes, so no interpolation argument is offered.
#[pyclass(
    name = "InterpolatedYoYInflationCurve",
    extends = PyYoYInflationTermStructure,
    unsendable
)]
pub struct PyInterpolatedYoYInflationCurve {
    concrete: Shared<InterpolatedYoYInflationCurve<Linear>>,
}

#[pymethods]
impl PyInterpolatedYoYInflationCurve {
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
            InterpolatedYoYInflationCurve::new(
                reference_date.inner(),
                dates,
                rates,
                frequency.inner(),
                day_counter.inner(),
                Linear,
                None,
            )
            .map_err(PyQlError::from)?,
        );
        let erased = Shared::clone(&concrete) as Shared<dyn YoYInflationTermStructure>;
        Ok(
            PyClassInitializer::from(PyYoYInflationTermStructure::from_handle(Handle::new(
                erased,
            )))
            .add_subclass(PyInterpolatedYoYInflationCurve { concrete }),
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

    /// The `(date, year-on-year rate)` nodes.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.concrete
            .nodes()
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect()
    }
}

/// Python `YoYInflationHelper`: the shared base for every year-on-year
/// bootstrap helper
/// (`termstructures::inflation::inflationhelpers::YoYInflationHelper`).
///
/// Holds the erased `Shared<dyn YoYInflationHelper>` and exposes the two dates
/// the bootstrap places a curve node by, the shape [`PyZeroInflationHelper`]
/// already uses. Concrete helpers such as [`PyYearOnYearInflationSwapHelper`]
/// subclass this and supply only their constructor.
///
/// Deferred (visible): the core trait's `earliest_date`, `maturity_date` and
/// `latest_relevant_date` are omitted, as on the zero side - on the one
/// concrete helper reachable here they collapse onto the same fixing-period
/// start (`inflationhelpers.rs:730-731`), so all three would report what
/// [`pillar_date`](Self::pillar_date) reports.
#[pyclass(name = "YoYInflationHelper", subclass, unsendable)]
pub struct PyYoYInflationHelper {
    inner: Shared<dyn YoYInflationHelper>,
}

#[pymethods]
impl PyYoYInflationHelper {
    /// The pillar date, at which the curve node this helper sets sits.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// The latest date the helper needs curve data at (equal to the pillar
    /// date).
    fn latest_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.latest_date())
    }
}

impl PyYoYInflationHelper {
    /// The base half of a concrete helper's [`PyClassInitializer`] chain.
    pub(crate) fn from_shared(inner: Shared<dyn YoYInflationHelper>) -> Self {
        PyYoYInflationHelper { inner }
    }

    /// A clone of the upcast helper, for the piecewise year-on-year curve
    /// facade, which threads each helper into the bootstrap.
    pub(crate) fn shared(&self) -> Shared<dyn YoYInflationHelper> {
        Shared::clone(&self.inner)
    }
}

/// Python `YoYInflationIndex`: an index publishing one year-on-year inflation
/// rate per period, read back as a stored figure or forecast off its
/// year-on-year curve (core `indexes::inflationindex::YoYInflationIndex`).
///
/// Two forms, as in the core. A **ratio** index
/// ([`from_underlying`](Self::from_underlying)) derives its rate from two
/// [`PyZeroInflationIndex`] fixings a year apart and owns no history of its
/// own; a **quoted** one (the constructor) is published as a rate in its own
/// right and keeps its own history through [`add_fixing`](Self::add_fixing).
///
/// The curve is reached through a [`RelinkableHandle`] the facade owns and both
/// forms link at construction, for the reason [`PyZeroInflationIndex`]
/// documents: the core's `with_term_structure` takes a plain
/// [`Handle`](libitofin::handle::Handle), so an index that did not take this
/// facade's relinkable handle up front could never be pointed at a curve later
/// and [`link_to`](Self::link_to) would silently do nothing. The handle starts
/// empty, so a forecast before any link raises the empty-handle error.
///
/// The quoted constructor spells its region and currency out as their component
/// fields: neither core type has a Python facade, and inventing a name lookup
/// or defaulting the currency metadata would put made-up values on the index.
#[pyclass(name = "YoYInflationIndex", unsendable)]
pub struct PyYoYInflationIndex {
    inner: Shared<YoYInflationIndex>,
    curve: RelinkableHandle<dyn YoYInflationTermStructure>,
    underlying: Option<Py<PyZeroInflationIndex>>,
}

impl PyYoYInflationIndex {
    /// Wraps `build` over a fresh empty relinkable handle the index observes.
    ///
    /// Both constructors route through here: the core's `from_underlying` and
    /// `new` alike leave the curve handle empty
    /// (`inflationindex.rs:661,684`), and it is
    /// [`with_term_structure`](YoYInflationIndex::with_term_structure) that
    /// registers the index on a handle it will keep seeing.
    fn with_curve_handle(
        underlying: Option<Py<PyZeroInflationIndex>>,
        build: impl FnOnce(&RelinkableHandle<dyn YoYInflationTermStructure>) -> YoYInflationIndex,
    ) -> Self {
        let curve = RelinkableHandle::<dyn YoYInflationTermStructure>::empty();
        let inner = shared(build(&curve));
        PyYoYInflationIndex {
            inner,
            curve,
            underlying,
        }
    }
}

#[pymethods]
impl PyYoYInflationIndex {
    /// A quoted year-on-year index, published as a rate in its own right and
    /// keeping its own fixing history.
    #[new]
    #[pyo3(signature = (
        family_name,
        region_name,
        region_code,
        revised,
        frequency,
        availability_lag,
        currency_name,
        currency_code,
        currency_numeric_code,
        currency_symbol,
        currency_fraction_symbol,
        currency_fractions_per_unit,
        settings,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        family_name: String,
        region_name: String,
        region_code: String,
        revised: bool,
        frequency: &PyFrequency,
        availability_lag: &PyPeriod,
        currency_name: String,
        currency_code: String,
        currency_numeric_code: i32,
        currency_symbol: String,
        currency_fraction_symbol: String,
        currency_fractions_per_unit: i32,
        settings: &PySettings,
    ) -> Self {
        PyYoYInflationIndex::with_curve_handle(None, |curve| {
            YoYInflationIndex::new(
                family_name,
                Region::new(region_name, region_code),
                revised,
                frequency.inner(),
                availability_lag.inner(),
                Currency::new(
                    currency_name,
                    currency_code,
                    currency_numeric_code,
                    currency_symbol,
                    currency_fraction_symbol,
                    currency_fractions_per_unit,
                ),
                settings.inner(),
            )
            .with_term_structure(curve.handle())
        })
    }

    /// A ratio year-on-year index over `underlying`, dividing that index's
    /// figure for a period by its figure a year earlier.
    ///
    /// The metadata is inherited wholesale bar the family name, which is
    /// prefixed `YYR_`, so a `"UK RPI"` underlying yields `"UK YYR_RPI"`. The
    /// index files no fixings of its own and reads the underlying's history
    /// instead, so [`add_fixing`](Self::add_fixing) belongs on the underlying.
    #[staticmethod]
    fn from_underlying(underlying: &Bound<'_, PyZeroInflationIndex>) -> Self {
        let zero = underlying.borrow().shared();
        let handle = underlying.clone().unbind();
        PyYoYInflationIndex::with_curve_handle(Some(handle), |curve| {
            YoYInflationIndex::from_underlying(zero).with_term_structure(curve.handle())
        })
    }

    /// The index name, e.g. `"UK YYR_RPI"`, under which fixings are stored.
    fn name(&self) -> String {
        self.inner.name()
    }

    /// Whether this index is the ratio of two price-index fixings rather than a
    /// quoted rate.
    fn ratio(&self) -> bool {
        self.inner.ratio()
    }

    /// The price index a ratio index divides, `None` on a quoted one.
    ///
    /// This is the very object [`from_underlying`](Self::from_underlying) was
    /// handed, not a fresh facade around the same core index: a rebuilt one
    /// would carry a relinkable handle this index never sees, so linking it
    /// would silently forecast off nothing.
    fn underlying_index(&self, py: Python<'_>) -> Option<Py<PyZeroInflationIndex>> {
        self.underlying
            .as_ref()
            .map(|underlying| underlying.clone_ref(py))
    }

    /// Records a published year-on-year rate across the whole inflation period
    /// it describes. A ratio index reads the underlying's history, so filing
    /// here records a figure it will never consult.
    fn add_fixing(&self, fixing_date: &PyDate, value: f64) -> PyResult<()> {
        Ok(self
            .inner
            .add_fixing(fixing_date.inner(), value)
            .map_err(PyQlError::from)?)
    }

    /// The rate at `fixing_date`, stored or forecast off the linked curve.
    ///
    /// `forecast_todays_fixing` is accepted and ignored, as in the core:
    /// [`needs_forecast`](Self::needs_forecast) alone decides. A forecast with
    /// no curve linked raises the empty-handle error.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// The first day of the inflation period the latest figure on record
    /// describes, read off the underlying on a ratio index. An index with no
    /// history is an error.
    fn last_fixing_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.last_fixing_date().map_err(PyQlError::from)?,
        ))
    }

    /// Points the index at `curve`, so every forecast from here on reads it.
    ///
    /// Takes the [`PyYoYInflationTermStructure`] base, so any subclass links.
    /// It is the curve *behind* `curve`'s handle at call time that is stored,
    /// not the handle itself.
    ///
    /// # Errors
    ///
    /// Reports the empty-handle error if `curve` somehow carries no link.
    fn link_to(&self, curve: &PyYoYInflationTermStructure) -> PyResult<()> {
        self.curve
            .link_to(curve.handle().current_link().map_err(PyQlError::from)?);
        Ok(())
    }

    /// Whether `fixing_date` has to be forecast rather than read from history,
    /// a ratio index deferring the question to its underlying.
    fn needs_forecast(&self, fixing_date: &PyDate) -> PyResult<bool> {
        Ok(self
            .inner
            .needs_forecast(fixing_date.inner())
            .map_err(PyQlError::from)?)
    }

    fn __repr__(&self) -> String {
        format!("YoYInflationIndex({})", self.inner.name())
    }
}

impl PyYoYInflationIndex {
    /// The wrapped core index, for the helper and swap facades that take one.
    pub(crate) fn shared(&self) -> Shared<YoYInflationIndex> {
        Shared::clone(&self.inner)
    }
}

/// Python `YearOnYearInflationSwapHelper`: the bootstrap helper fitting a
/// year-on-year inflation swap quoted as a rate
/// (`termstructures::inflation::inflationhelpers::YearOnYearInflationSwapHelper`).
///
/// The helper prices a unit-notional, zero-strike swap of its own and reports
/// that contract's fair rate; the bootstrap drives the quoted rate less that
/// fair rate to zero. Unlike its zero-coupon twin it does need a nominal curve:
/// the year-on-year legs pay on a schedule of dates rather than one, so their
/// discount factors do not cancel.
///
/// The swap starts at the evaluation date held by `settings` and is rebuilt
/// whenever that date moves, so the evaluation date must be set *before* the
/// constructor runs, not merely before the bootstrap. The helper retains the
/// caller's [`PySimpleQuote`], so a later `set_value` re-drives the bootstrap.
///
/// It prices through a *copy* of `index` linked to a handle of its own, which
/// the bootstrap points at the curve under construction; the caller's index
/// keeps whatever curve it had and need not be linked at all.
///
/// `pillar` is accepted for signature parity but never read: it only ever
/// discriminates on the interpolated path, which is refused below.
///
/// Fallible: [`CpiInterpolationType.Linear`](PyCpiInterpolationType) is refused
/// outright (`inflationhelpers.rs:701-706`), the interpolated branch of the C++
/// constructor being deferred with the rest of `CPI::Linear` (#847); and the
/// swap is built here, so an observation lag its legs cannot be built under
/// fails at construction. Both raise [`struct@crate::ItofinError`].
///
/// Deferred (visible): the zero twin's `inflation_fixing_date` has no
/// counterpart here - a year-on-year contract observes once per coupon rather
/// than once at maturity, and there is no cash-flow facade to enumerate them
/// through (#848).
#[pyclass(
    name = "YearOnYearInflationSwapHelper",
    extends = PyYoYInflationHelper,
    unsendable
)]
pub struct PyYearOnYearInflationSwapHelper;

#[pymethods]
impl PyYearOnYearInflationSwapHelper {
    /// A helper fitting `quote` on a swap maturing at `maturity`, discounted on
    /// `nominal_term_structure`.
    #[new]
    #[pyo3(signature = (
        quote,
        swap_obs_lag,
        maturity,
        calendar,
        payment_convention,
        day_counter,
        index,
        interpolation,
        nominal_term_structure,
        settings,
        pillar = PyPillar::LastRelevantDate,
    ))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: &PySimpleQuote,
        swap_obs_lag: &PyPeriod,
        maturity: &PyDate,
        calendar: &PyCalendar,
        payment_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        index: &PyYoYInflationIndex,
        interpolation: &PyCpiInterpolationType,
        nominal_term_structure: &PyYieldTermStructure,
        settings: &PySettings,
        pillar: PyPillar,
    ) -> PyResult<PyClassInitializer<Self>> {
        let concrete = YearOnYearInflationSwapHelper::new(
            quote.handle(),
            swap_obs_lag.inner(),
            maturity.inner(),
            calendar.inner(),
            payment_convention.inner(),
            day_counter.inner(),
            &index.shared(),
            interpolation.inner(),
            nominal_term_structure.handle(),
            pillar.inner(),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        let erased = concrete as Shared<dyn YoYInflationHelper>;
        Ok(
            PyClassInitializer::from(PyYoYInflationHelper::from_shared(erased))
                .add_subclass(PyYearOnYearInflationSwapHelper),
        )
    }
}

/// Python `PiecewiseYoYInflationCurve`: a year-on-year inflation curve
/// bootstrapped from year-on-year helpers, solving one rate node per helper
/// (`termstructures::inflation::PiecewiseYoYInflationCurve<Linear>`).
///
/// Extends [`PyYoYInflationTermStructure`], which carries the whole query
/// surface. Node zero sits on `base_date` at `base_yoy_rate` and is kept rather
/// than solved: the base date is the last date for which a fixing is known and
/// precedes the reference date, so [`times`](Self::times)`[0]` is negative.
/// Each helper's observed fixing period marks a later segment boundary, and its
/// node is solved so the helper reprices its own quote off the curve.
///
/// Lazy, like [`PyPiecewiseZeroInflationCurve`]: the constructor only registers
/// on the helpers, and the bootstrap runs on the first read - a query, an
/// inspector, or an explicit [`calculate`](Self::calculate). A helper quote
/// moving invalidates the cache.
///
/// Fallible: the core rejects an empty helper set, and every inspector
/// propagates a bootstrap failure as [`struct@crate::ItofinError`]. `Linear` is
/// pinned at the boundary, it being the only interpolator the core constructs.
#[pyclass(
    name = "PiecewiseYoYInflationCurve",
    extends = PyYoYInflationTermStructure,
    unsendable
)]
pub struct PyPiecewiseYoYInflationCurve {
    concrete: Shared<PiecewiseYoYInflationCurve<Linear>>,
}

#[pymethods]
impl PyPiecewiseYoYInflationCurve {
    /// A curve over `helpers` with a fixed `reference_date`, a `base_date`
    /// preceding it and the `base_yoy_rate` observed over the period ending
    /// there. `helpers` accepts any [`YoYInflationHelper`](PyYoYInflationHelper)
    /// subclass.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        reference_date: &PyDate,
        base_date: &PyDate,
        base_yoy_rate: f64,
        frequency: &PyFrequency,
        day_counter: &PyDayCounter,
        helpers: Vec<PyRef<PyYoYInflationHelper>>,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn YoYInflationHelper>> =
            helpers.iter().map(|helper| helper.shared()).collect();
        let concrete = PiecewiseYoYInflationCurve::<Linear>::new(
            reference_date.inner(),
            base_date.inner(),
            base_yoy_rate,
            frequency.inner(),
            day_counter.inner(),
            instruments,
            None,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn YoYInflationTermStructure>;
        Ok(
            PyClassInitializer::from(PyYoYInflationTermStructure::from_handle(Handle::new(
                erased,
            )))
            .add_subclass(PyPiecewiseYoYInflationCurve { concrete }),
        )
    }

    /// Runs the bootstrap if the cache is stale, so a solver failure surfaces
    /// here rather than inside a later query.
    fn calculate(&self) -> PyResult<()> {
        Ok(self.concrete.calculate().map_err(PyQlError::from)?)
    }

    /// The node times, measured from the reference date in the curve's own day
    /// count; the first is negative (triggers the bootstrap).
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.times().map_err(PyQlError::from)?)
    }

    /// The node dates, the first of which is the base date (triggers the
    /// bootstrap).
    fn dates(&self) -> PyResult<Vec<PyDate>> {
        Ok(self
            .concrete
            .dates()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(PyDate::from_inner)
            .collect())
    }

    /// The solved `(date, year-on-year rate)` nodes (triggers the bootstrap).
    fn nodes(&self) -> PyResult<Vec<(PyDate, f64)>> {
        Ok(self
            .concrete
            .nodes()
            .map_err(PyQlError::from)?
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect())
    }
}

/// Python `YearOnYearInflationSwap`: a fixed leg against a leg of year-on-year
/// inflation coupons, both paid over a schedule
/// (`instruments::yearonyearinflationswap`).
///
/// [`SwapType`](crate::swap::PySwapType) names the *fixed* leg, so a `Payer`
/// pays fixed and receives inflation - the opposite reading from
/// [`PyZeroCouponInflationSwap`], where it names the inflation leg.
///
/// The two schedules are independent inputs. The fixed leg takes its payment
/// calendar from its own schedule while the year-on-year leg pays on
/// `payment_calendar`, the inflation index carrying no calendar of its own;
/// both legs adjust with `payment_convention`, which C++ defaults to
/// `ModifiedFollowing` and the port takes explicitly. `spread` is added to
/// every forecast rate on the year-on-year leg.
///
/// Fallible at construction: both leg builds propagate, so a coupon the
/// observation lag leaves unbuildable fails here.
///
/// Pricing needs an engine: call [`set_engine`](Self::set_engine) before
/// anything else. Every accessor below is `&mut self` where the zero-coupon
/// swap's [`fair_rate`](PyZeroCouponInflationSwap::fair_rate) is not: this
/// contract recovers both fair values from a priced result rather than reading
/// a single indexed flow (`yearonyearinflationswap.rs:311,326`), so each one
/// drives the calculation.
///
/// Deferred (visible): the leg and coupon accessors are not surfaced, there
/// being no cash-flow facade to hand them back through (#848).
#[pyclass(name = "YearOnYearInflationSwap", unsendable)]
pub struct PyYearOnYearInflationSwap {
    inner: SharedMut<YearOnYearInflationSwap>,
}

#[pymethods]
impl PyYearOnYearInflationSwap {
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        swap_type,
        nominal,
        fixed_schedule,
        fixed_rate,
        fixed_day_count,
        yoy_schedule,
        yoy_index,
        observation_lag,
        interpolation,
        spread,
        yoy_day_count,
        payment_calendar,
        payment_convention,
        settings,
    ))]
    fn new(
        swap_type: &PySwapType,
        nominal: f64,
        fixed_schedule: &PySchedule,
        fixed_rate: f64,
        fixed_day_count: &PyDayCounter,
        yoy_schedule: &PySchedule,
        yoy_index: &PyYoYInflationIndex,
        observation_lag: &PyPeriod,
        interpolation: &PyCpiInterpolationType,
        spread: f64,
        yoy_day_count: &PyDayCounter,
        payment_calendar: &PyCalendar,
        payment_convention: &PyBusinessDayConvention,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(PyYearOnYearInflationSwap {
            inner: shared_mut(
                YearOnYearInflationSwap::new(
                    swap_type.inner(),
                    nominal,
                    fixed_schedule.inner(),
                    fixed_rate,
                    fixed_day_count.inner(),
                    yoy_schedule.inner(),
                    yoy_index.shared(),
                    observation_lag.inner(),
                    interpolation.inner(),
                    spread,
                    yoy_day_count.inner(),
                    payment_calendar.inner(),
                    payment_convention.inner(),
                    settings.inner(),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Attaches a [`PyDiscountingSwapEngine`] so the swap prices.
    ///
    /// The engine must resolve its dates against the same `Settings` object
    /// this swap was built with.
    fn set_engine(&mut self, engine: &PyDiscountingSwapEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Forces the valuation, idempotent and fallible as
    /// [`VanillaOption.calculate`](crate::option::PyVanillaOption::calculate).
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Whether the cached results are currently valid.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Attaches `engine` and returns the NPV, the one-shot form of
    /// [`set_engine`](Self::set_engine) followed by [`npv`](Self::npv).
    fn price(&mut self, engine: &PyDiscountingSwapEngine) -> PyResult<f64> {
        self.set_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// A frozen [`Results`] copy of the valuation, calculating first.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// The swap NPV under the attached engine.
    ///
    /// Fallible: with no engine attached the core reports `"null pricing
    /// engine"`, and with no curve linked into the index the coupons cannot be
    /// forecast.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// The fixed rate that would price the swap at zero, recovered from the NPV
    /// and the fixed leg's BPS. Prices on demand, so it needs an engine.
    fn fair_rate(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_rate()
            .map_err(PyQlError::from)?)
    }

    /// The spread over the index that would price the swap at zero, recovered
    /// off the year-on-year leg. Fallible as [`fair_rate`](Self::fair_rate).
    fn fair_spread(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_spread()
            .map_err(PyQlError::from)?)
    }

    /// The fixed leg's NPV, priced on demand. Fallible as [`npv`](Self::npv).
    fn fixed_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// The year-on-year leg's NPV, priced on demand. Fallible as
    /// [`npv`](Self::npv).
    fn yoy_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .yoy_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// The quoted fixed rate the swap was struck at.
    fn fixed_rate(&self) -> f64 {
        self.inner.borrow().fixed_rate()
    }

    /// The spread the year-on-year coupons carry over the index.
    fn spread(&self) -> f64 {
        self.inner.borrow().spread()
    }
}

/// Python `ConstantYoYOptionletVolatility`: one volatility for every strike and
/// every date (`termstructures::volatility::ConstantYoYOptionletVolatility`).
///
/// The reference date moves with the evaluation date carried by `settings`,
/// `settlement_days` business days on from it, so that date must be set before
/// anything is priced off the surface.
///
/// `min_strike` and `max_strike` bound the strike domain a query is answered
/// over; C++ defaults them to `-1.0` and `100.0` and the port carries no default
/// arguments, so both are passed here too.
///
/// Both constructors are bound: the flat one taking a value, and
/// [`with_quote`](Self::with_quote) taking a live quote whose changes notify the
/// surface's observers.
///
/// Deferred (visible): the whole stripped/interpolated surface hierarchy, which
/// has no port at all, tracked as #874.
#[pyclass(name = "ConstantYoYOptionletVolatility", unsendable)]
pub struct PyConstantYoYOptionletVolatility {
    inner: Shared<ConstantYoYOptionletVolatility>,
}

#[pymethods]
impl PyConstantYoYOptionletVolatility {
    /// A flat surface at `volatility`, observing inflation `observation_lag`
    /// back on an index publishing at `frequency`.
    ///
    /// Infallible, unlike most curve constructors: nothing is resolved here.
    /// The reference date, the base date and the strike range are all read at
    /// query time, so an unset evaluation date surfaces then.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        volatility: f64,
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        observation_lag: &PyPeriod,
        frequency: &PyFrequency,
        index_is_interpolated: bool,
        min_strike: f64,
        max_strike: f64,
        settings: &PySettings,
    ) -> Self {
        PyConstantYoYOptionletVolatility {
            inner: shared(ConstantYoYOptionletVolatility::new(
                volatility,
                settlement_days,
                calendar.inner(),
                business_day_convention.inner(),
                day_counter.inner(),
                observation_lag.inner(),
                frequency.inner(),
                index_is_interpolated,
                min_strike,
                max_strike,
                settings.inner(),
            )),
        }
    }

    /// A flat surface quoted by `volatility`: the quote is retained rather than
    /// read once, so a later `set_value` on it notifies the surface's observers
    /// and anything priced off the surface reprices at the new level.
    ///
    /// Infallible, and otherwise as [`new`](Self::new), which the arguments
    /// after the first mirror exactly.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    fn with_quote(
        volatility: &PySimpleQuote,
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        observation_lag: &PyPeriod,
        frequency: &PyFrequency,
        index_is_interpolated: bool,
        min_strike: f64,
        max_strike: f64,
        settings: &PySettings,
    ) -> Self {
        PyConstantYoYOptionletVolatility {
            inner: shared(ConstantYoYOptionletVolatility::with_quote(
                volatility.handle(),
                settlement_days,
                calendar.inner(),
                business_day_convention.inner(),
                day_counter.inner(),
                observation_lag.inner(),
                frequency.inner(),
                index_is_interpolated,
                min_strike,
                max_strike,
                settings.inner(),
            )),
        }
    }

    /// The lag the surface itself observes inflation with.
    fn observation_lag(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.observation_lag())
    }

    /// How often the observed index publishes.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(self.inner.frequency())
    }

    /// Whether the observed index interpolates between publications.
    fn index_is_interpolated(&self) -> bool {
        self.inner.index_is_interpolated()
    }

    /// The date the surface measures its variance from: the reference date
    /// pulled back by the surface's own observation lag, snapped to the start of
    /// the publication period unless the index is interpolated.
    ///
    /// # Errors
    ///
    /// Reports an unset evaluation date, and a frequency admitting no
    /// publication period.
    fn base_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.base_date().map_err(PyQlError::from)?,
        ))
    }

    /// The volatility for an exercise on `date` struck at `strike`, observing
    /// inflation `obs_lag` back.
    ///
    /// The lag is explicit rather than defaulted: C++ substitutes the surface's
    /// own for a sentinel period, and the port has no sentinel to carry, so a
    /// caller wanting it passes [`observation_lag`](Self::observation_lag).
    ///
    /// # Errors
    ///
    /// Reports an observed date before [`base_date`](Self::base_date), and a
    /// `strike` outside the surface's strike domain.
    fn volatility(&self, date: &PyDate, strike: f64, obs_lag: &PyPeriod) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility(date.inner(), strike, obs_lag.inner())
            .map_err(PyQlError::from)?)
    }

    /// The total integrated variance for an exercise on `date` struck at
    /// `strike`: the figure that scales time out of the optionlet formulae
    /// without committing to the distribution reading it.
    ///
    /// # Errors
    ///
    /// As [`volatility`](Self::volatility).
    fn total_variance(&self, date: &PyDate, strike: f64, obs_lag: &PyPeriod) -> PyResult<f64> {
        Ok(self
            .inner
            .total_variance(date.inner(), strike, obs_lag.inner())
            .map_err(PyQlError::from)?)
    }
}

impl PyConstantYoYOptionletVolatility {
    /// The erased surface handle the cap/floor engine constructors take.
    pub(crate) fn handle(&self) -> Handle<dyn YoYOptionletVolatilitySurface> {
        Handle::new(Shared::clone(&self.inner) as Shared<dyn YoYOptionletVolatilitySurface>)
    }
}

/// Python `YoYInflationCapFloorEngine`: prices a year-on-year cap or floor
/// optionlet by optionlet (`pricingengines::YoYInflationCapFloorEngine`).
///
/// The distribution is chosen by the constructor rather than passed as an
/// argument, mirroring the core's three constructors and C++'s three engine
/// classes: [`black`](Self::black) is lognormal, `unit_displaced` lognormal in
/// `1 + rate` and `bachelier` normal. The core
/// `YoYOptionletDistribution` enum is not bound - nothing takes one by value -
/// so [`distribution`](Self::distribution) reads back as a string.
///
/// The `settings` behind the volatility surface and behind the cap/floor this
/// engine prices must be the same object, or the two resolve their dates
/// against different evaluation dates and the NPV is silently wrong.
///
/// An engine carries the arguments and results of the contract it last priced,
/// so a cap and a floor priced together want one engine each.
#[pyclass(name = "YoYInflationCapFloorEngine", unsendable)]
pub struct PyYoYInflationCapFloorEngine {
    inner: SharedMut<YoYInflationCapFloorEngine>,
}

#[pymethods]
impl PyYoYInflationCapFloorEngine {
    /// Optionlets under the lognormal model (C++
    /// `YoYInflationBlackCapFloorEngine`), forwards read off `index`,
    /// volatilities off `volatility` and discounting on `nominal_ts`.
    #[staticmethod]
    fn black(
        index: &PyYoYInflationIndex,
        volatility: &PyConstantYoYOptionletVolatility,
        nominal_ts: &PyYieldTermStructure,
    ) -> Self {
        PyYoYInflationCapFloorEngine {
            inner: shared_mut(YoYInflationCapFloorEngine::black(
                index.shared(),
                volatility.handle(),
                nominal_ts.handle(),
            )),
        }
    }

    /// Optionlets under the unit-displaced lognormal model (C++
    /// `YoYInflationUnitDisplacedBlackCapFloorEngine`), the usual quoting
    /// convention for a rate that may be negative. See [`black`](Self::black).
    #[staticmethod]
    fn unit_displaced(
        index: &PyYoYInflationIndex,
        volatility: &PyConstantYoYOptionletVolatility,
        nominal_ts: &PyYieldTermStructure,
    ) -> Self {
        PyYoYInflationCapFloorEngine {
            inner: shared_mut(YoYInflationCapFloorEngine::unit_displaced(
                index.shared(),
                volatility.handle(),
                nominal_ts.handle(),
            )),
        }
    }

    /// Optionlets under the normal model (C++
    /// `YoYInflationBachelierCapFloorEngine`). See [`black`](Self::black).
    #[staticmethod]
    fn bachelier(
        index: &PyYoYInflationIndex,
        volatility: &PyConstantYoYOptionletVolatility,
        nominal_ts: &PyYieldTermStructure,
    ) -> Self {
        PyYoYInflationCapFloorEngine {
            inner: shared_mut(YoYInflationCapFloorEngine::bachelier(
                index.shared(),
                volatility.handle(),
                nominal_ts.handle(),
            )),
        }
    }

    /// The distribution optionlets are valued under, as `"black"`,
    /// `"unit_displaced"` or `"bachelier"`.
    fn distribution(&self) -> String {
        match self.inner.borrow().distribution() {
            YoYOptionletDistribution::Black => "black",
            YoYOptionletDistribution::UnitDisplaced => "unit_displaced",
            YoYOptionletDistribution::Bachelier => "bachelier",
        }
        .to_string()
    }
}

impl PyYoYInflationCapFloorEngine {
    /// The erased engine the cap/floor facade installs via `set_pricing_engine`.
    pub(crate) fn engine(&self) -> SharedMut<dyn PricingEngine> {
        SharedMut::clone(&self.inner) as SharedMut<dyn PricingEngine>
    }
}

/// Python `MakeYoYInflationCapFloor`: the standard market builder for a
/// year-on-year inflation cap or floor
/// (`instruments::makeyoyinflationcapfloor`).
///
/// It derives an annual year-on-year leg from a length in years, trims that leg
/// to the optionlets asked for, and strikes it either at an explicit `strike` or
/// at the money off `atm_strike`. Exactly one of the two is required: the core
/// refuses both together and neither at all, at build time rather than at the
/// setters, so both surface from [`build`](Self::build).
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the whole configuration up front and
/// assembles the chain inside [`build`](Self::build), as
/// [`MakeVanillaSwap`](crate::swap::PyMakeVanillaSwap) does. An unset optional
/// leaves the core default in place: a 1,000,000 nominal, a `ModifiedFollowing`
/// payment roll, a 30/360 bond-basis day counter, no fixing days, every
/// optionlet kept and no forward start.
///
/// Trimming happens *before* the at-the-money fill, so `as_optionlet` and
/// `first_caplet_excluded` change what an unset strike resolves to: the rate
/// that reprices whatever survives, not the whole leg's.
///
/// `CapFloorType.Collar` has no path through the builder - it carries a single
/// strike, and a collar needs two strike vectors - so a collar is built through
/// [`YoYInflationCapFloor::collar`](PyYoYInflationCapFloor::collar) over a leg
/// of its own instead.
#[pyclass(name = "MakeYoYInflationCapFloor", unsendable)]
pub struct PyMakeYoYInflationCapFloor {
    cap_floor_type: PyCapFloorType,
    index: Shared<YoYInflationIndex>,
    length: u64,
    calendar: Calendar,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    settings: Shared<Settings<Date>>,
    nominal: Option<f64>,
    effective_date: Option<Date>,
    payment_day_counter: Option<DayCounter>,
    payment_adjustment: Option<BusinessDayConvention>,
    fixing_days: Option<u32>,
    engine: Option<SharedMut<dyn PricingEngine>>,
    as_optionlet: bool,
    forward_start: Option<Period>,
    first_caplet_excluded: bool,
    strike: Option<f64>,
    atm_strike: Option<Handle<dyn YieldTermStructure>>,
}

#[pymethods]
impl PyMakeYoYInflationCapFloor {
    /// A builder for a `cap_floor_type` cap/floor of `length` years on `index`,
    /// observed `observation_lag` back under `interpolation` and paying on
    /// `calendar`.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        cap_floor_type,
        index,
        length,
        calendar,
        observation_lag,
        interpolation,
        settings,
        nominal = None,
        effective_date = None,
        payment_day_counter = None,
        payment_adjustment = None,
        fixing_days = None,
        engine = None,
        as_optionlet = false,
        forward_start = None,
        first_caplet_excluded = false,
        strike = None,
        atm_strike = None,
    ))]
    fn new(
        cap_floor_type: PyCapFloorType,
        index: &PyYoYInflationIndex,
        length: u64,
        calendar: &PyCalendar,
        observation_lag: &PyPeriod,
        interpolation: PyCpiInterpolationType,
        settings: &PySettings,
        nominal: Option<f64>,
        effective_date: Option<&PyDate>,
        payment_day_counter: Option<&PyDayCounter>,
        payment_adjustment: Option<&PyBusinessDayConvention>,
        fixing_days: Option<u32>,
        engine: Option<&PyYoYInflationCapFloorEngine>,
        as_optionlet: bool,
        forward_start: Option<&PyPeriod>,
        first_caplet_excluded: bool,
        strike: Option<f64>,
        atm_strike: Option<&PyYieldTermStructure>,
    ) -> Self {
        PyMakeYoYInflationCapFloor {
            cap_floor_type,
            index: index.shared(),
            length,
            calendar: calendar.inner(),
            observation_lag: observation_lag.inner(),
            interpolation: interpolation.inner(),
            settings: settings.inner(),
            nominal,
            effective_date: effective_date.map(PyDate::inner),
            payment_day_counter: payment_day_counter.map(PyDayCounter::inner),
            payment_adjustment: payment_adjustment.map(PyBusinessDayConvention::inner),
            fixing_days,
            engine: engine.map(PyYoYInflationCapFloorEngine::engine),
            as_optionlet,
            forward_start: forward_start.map(PyPeriod::inner),
            first_caplet_excluded,
            strike,
            atm_strike: atm_strike.map(PyYieldTermStructure::handle),
        }
    }

    /// Builds the cap/floor, which already carries its engine when one was
    /// given.
    ///
    /// # Errors
    ///
    /// Reports both an explicit `strike` and an `atm_strike` given together and
    /// neither given at all; an unset evaluation date when the start date has to
    /// be derived; and whatever the leg construction and the at-the-money fill
    /// report.
    fn build(&self) -> PyResult<PyYoYInflationCapFloor> {
        let mut maker = MakeYoYInflationCapFloor::new(
            self.cap_floor_type.inner(),
            Shared::clone(&self.index),
            self.length as usize,
            self.calendar.clone(),
            self.observation_lag,
            self.interpolation,
            Shared::clone(&self.settings),
        );
        if let Some(nominal) = self.nominal {
            maker = maker.with_nominal(nominal);
        }
        if let Some(effective_date) = self.effective_date {
            maker = maker.with_effective_date(effective_date);
        }
        if let Some(day_counter) = &self.payment_day_counter {
            maker = maker.with_payment_day_counter(day_counter.clone());
        }
        if let Some(convention) = self.payment_adjustment {
            maker = maker.with_payment_adjustment(convention);
        }
        if let Some(fixing_days) = self.fixing_days {
            maker = maker.with_fixing_days(fixing_days);
        }
        if let Some(engine) = &self.engine {
            maker = maker.with_pricing_engine(SharedMut::clone(engine));
        }
        if self.as_optionlet {
            maker = maker.as_optionlet(true);
        }
        if let Some(forward_start) = self.forward_start {
            maker = maker.with_forward_start(forward_start);
        }
        if self.first_caplet_excluded {
            maker = maker.with_first_caplet_excluded();
        }
        if let Some(strike) = self.strike {
            maker = maker.with_strike(strike);
        }
        if let Some(atm_strike) = &self.atm_strike {
            maker = maker.with_atm_strike(atm_strike.clone());
        }
        Ok(PyYoYInflationCapFloor::from_inner(shared_mut(
            maker.build().map_err(PyQlError::from)?,
        )))
    }
}

/// Python `YoYInflationCapFloor`: a cap or floor over a year-on-year inflation
/// leg (`instruments::inflationcapfloor::YoYInflationCapFloor`).
///
/// Built either through [`MakeYoYInflationCapFloor`](PyMakeYoYInflationCapFloor),
/// the standard market builder, or through the raw constructors
/// [`new`](Self::new), [`cap`](Self::cap), [`floor`](Self::floor),
/// [`collar`](Self::collar) and [`with_strikes`](Self::with_strikes), which take
/// the coupon vector [`YoYInflationLeg`](crate::cashflows::PyYoYInflationLeg)
/// hands back (#848). The raw route is the only one that reaches a collar: the
/// builder carries a single strike.
///
/// Unlike a nominal cap/floor this instrument keeps its first optionlet, so the
/// strip spans its leg exactly and cap - floor is the year-on-year swap.
///
/// Pricing needs an engine: call [`set_engine`](Self::set_engine) before
/// [`npv`](Self::npv).
#[pyclass(name = "YoYInflationCapFloor", unsendable)]
pub struct PyYoYInflationCapFloor {
    inner: SharedMut<YoYInflationCapFloor>,
}

#[pymethods]
impl PyYoYInflationCapFloor {
    /// A `cap_floor_type` instrument over `coupons`, struck at `cap_rates` and
    /// `floor_rates` (`inflationcapfloor.rs:141`).
    ///
    /// Each strike vector is padded to the leg length by repeating its last
    /// entry, so a single strike stands for every optionlet.
    ///
    /// # Errors
    ///
    /// Reports an empty leg, and a strike vector the type needs and did not
    /// get: a cap or a collar needs cap rates, a floor or a collar floor rates.
    #[staticmethod]
    fn new(
        cap_floor_type: PyCapFloorType,
        coupons: Vec<PyRef<PyYoYInflationCoupon>>,
        cap_rates: Vec<f64>,
        floor_rates: Vec<f64>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self::from_inner(shared_mut(
            YoYInflationCapFloor::new(
                cap_floor_type.inner(),
                coupons.iter().map(|coupon| coupon.shared()).collect(),
                cap_rates,
                floor_rates,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        )))
    }

    /// A cap over `coupons` struck at `strikes`. Fallible as
    /// [`new`](Self::new).
    #[staticmethod]
    fn cap(
        coupons: Vec<PyRef<PyYoYInflationCoupon>>,
        strikes: Vec<f64>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self::from_inner(shared_mut(
            YoYInflationCapFloor::cap(
                coupons.iter().map(|coupon| coupon.shared()).collect(),
                strikes,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        )))
    }

    /// A floor over `coupons` struck at `strikes`. Fallible as
    /// [`new`](Self::new).
    #[staticmethod]
    fn floor(
        coupons: Vec<PyRef<PyYoYInflationCoupon>>,
        strikes: Vec<f64>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self::from_inner(shared_mut(
            YoYInflationCapFloor::floor(
                coupons.iter().map(|coupon| coupon.shared()).collect(),
                strikes,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        )))
    }

    /// A collar over `coupons`, long the cap at `cap_rates` and short the floor
    /// at `floor_rates`. Fallible as [`new`](Self::new).
    #[staticmethod]
    fn collar(
        coupons: Vec<PyRef<PyYoYInflationCoupon>>,
        cap_rates: Vec<f64>,
        floor_rates: Vec<f64>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self::from_inner(shared_mut(
            YoYInflationCapFloor::collar(
                coupons.iter().map(|coupon| coupon.shared()).collect(),
                cap_rates,
                floor_rates,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        )))
    }

    /// The single-vector constructor: `strikes` are cap rates for a
    /// `CapFloorType.Cap` and floor rates for a `CapFloorType.Floor`.
    ///
    /// # Errors
    ///
    /// Reports an empty `strikes`, a `CapFloorType.Collar` - which needs two
    /// vectors, so [`collar`](Self::collar) is its constructor - and as
    /// [`new`](Self::new).
    #[staticmethod]
    fn with_strikes(
        cap_floor_type: PyCapFloorType,
        coupons: Vec<PyRef<PyYoYInflationCoupon>>,
        strikes: Vec<f64>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self::from_inner(shared_mut(
            YoYInflationCapFloor::with_strikes(
                cap_floor_type.inner(),
                coupons.iter().map(|coupon| coupon.shared()).collect(),
                strikes,
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        )))
    }

    /// The cap strikes, one per coupon; empty on a floor.
    fn cap_rates(&self) -> Vec<f64> {
        self.inner.borrow().cap_rates().to_vec()
    }

    /// The floor strikes, one per coupon; empty on a cap.
    fn floor_rates(&self) -> Vec<f64> {
        self.inner.borrow().floor_rates().to_vec()
    }

    /// The number of optionlets, one per year-on-year coupon.
    fn coupon_count(&self) -> usize {
        self.inner.borrow().yoy_leg().len()
    }

    /// The leg's earliest accrual start.
    ///
    /// # Errors
    ///
    /// Reports an empty leg, which the constructors already refuse.
    fn start_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.borrow().start_date().map_err(PyQlError::from)?,
        ))
    }

    /// The leg's latest accrual end. Fallible as
    /// [`start_date`](Self::start_date).
    fn maturity_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .borrow()
                .maturity_date()
                .map_err(PyQlError::from)?,
        ))
    }

    /// The at-the-money rate: the strike at which the leg reprices on
    /// `discount_curve`.
    ///
    /// The core takes the curve itself rather than a handle, so the link is
    /// resolved here and held for the call.
    ///
    /// # Errors
    ///
    /// Reports an unlinked `discount_curve`, a curve with no reference date and
    /// a leg with no basis-point sensitivity to solve over.
    fn atm_rate(&self, discount_curve: &PyYieldTermStructure) -> PyResult<f64> {
        let handle = discount_curve.handle();
        let link = handle.current_link().map_err(PyQlError::from)?;
        Ok(self
            .inner
            .borrow()
            .atm_rate(link.as_ref())
            .map_err(PyQlError::from)?)
    }

    /// Attaches a [`PyYoYInflationCapFloorEngine`], replacing whatever engine
    /// the factory installed.
    ///
    /// The engine must resolve its dates against the same `Settings` object this
    /// cap/floor was built with: two different ones would price the leg and the
    /// optionlets on different dates with no error raised.
    fn set_engine(&mut self, engine: &PyYoYInflationCapFloorEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Forces the valuation, idempotent and fallible as
    /// [`VanillaOption.calculate`](crate::option::PyVanillaOption::calculate).
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Whether the cached results are currently valid.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Attaches `engine`, replacing whatever the factory installed, and returns
    /// the NPV: the one-shot form of [`set_engine`](Self::set_engine) followed
    /// by [`npv`](Self::npv).
    fn price(&mut self, engine: &PyYoYInflationCapFloorEngine) -> PyResult<f64> {
        self.set_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// A frozen [`Results`] copy of the valuation, calculating first.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// The cap/floor NPV under the attached engine.
    ///
    /// # Errors
    ///
    /// Reports `"null pricing engine"` with no engine attached, and whatever the
    /// engine reports.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }
}

impl PyYoYInflationCapFloor {
    /// Wraps the instrument the factory built.
    pub(crate) fn from_inner(inner: SharedMut<YoYInflationCapFloor>) -> Self {
        PyYoYInflationCapFloor { inner }
    }
}

/// Python `YoYCapFloorTermPriceSurface`: the quoted year-on-year cap/floor
/// price grid, bicubic-interpolated over strike and maturity with a cubic ATM
/// swap rate curve through the cap/floor intersections (core
/// `termstructures::inflation::yoycapfloortermpricesurface`, C++'s
/// `InterpolatedYoYCapFloorTermPriceSurface<Bicubic, Cubic>`).
///
/// The facade holds the concrete interpolated surface; the K-interpolated
/// optionlet stripping pipeline (#910) reads it back erased through a
/// crate-internal upcast accessor, the same seam the engine facades use.
///
/// Construction only validates and stores the quotes. The calculations - the
/// cap/floor intersection and the year-on-year bootstrap over its ATM swap
/// rates - run on the first read and are cached, so a bad grid surfaces at the
/// constructor and a failed solve at the first price or rate query. The
/// reference date moves with the evaluation date carried by `settings`, which
/// must be set before construction: the maturity checks resolve against it.
///
/// Deferred (visible): the remaining trait surface - `price` and the by-tenor
/// query forms, `atm_yoy_rate`, the `atm_yoy_swap_time_rates`/`date_rates`
/// vectors, the cap/floor strike splits, `yoy_ts` and `base_date` - is not
/// exposed; nothing downstream of #909/#910 reads it from Python yet.
#[pyclass(name = "YoYCapFloorTermPriceSurface", unsendable)]
pub struct PyYoYCapFloorTermPriceSurface {
    inner: Shared<InterpolatedYoYCapFloorTermPriceSurface>,
}

#[pymethods]
impl PyYoYCapFloorTermPriceSurface {
    /// A surface over quoted cap and floor prices by strike (rows) and
    /// maturity (columns).
    ///
    /// # Errors
    ///
    /// Reports an empty or ragged price grid, and the core's data-consistency
    /// gate: mismatched strike/maturity/grid dimensions, a non-increasing
    /// axis, or a cap/floor strike union that overlaps the wrong way round.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        fixing_days: u32,
        yy_lag: &PyPeriod,
        yoy_index: &PyYoYInflationIndex,
        interpolation: &PyCpiInterpolationType,
        nominal_term_structure: &PyYieldTermStructure,
        day_counter: &PyDayCounter,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        c_strikes: Vec<f64>,
        f_strikes: Vec<f64>,
        cf_maturities: Vec<PyRef<'_, PyPeriod>>,
        c_price: Vec<Vec<f64>>,
        f_price: Vec<Vec<f64>>,
        settings: &PySettings,
    ) -> PyResult<Self> {
        let inner = InterpolatedYoYCapFloorTermPriceSurface::new(
            fixing_days,
            yy_lag.inner(),
            yoy_index.shared(),
            interpolation.inner(),
            nominal_term_structure.handle(),
            day_counter.inner(),
            calendar.inner(),
            business_day_convention.inner(),
            c_strikes,
            f_strikes,
            cf_maturities.iter().map(|period| period.inner()).collect(),
            crate::capfloortermvol::matrix_from_rows(&c_price)?,
            crate::capfloortermvol::matrix_from_rows(&f_price)?,
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(PyYoYCapFloorTermPriceSurface {
            inner: shared(inner),
        })
    }

    /// The interpolated cap price at `date` struck at `strike`, a pure surface
    /// lookup with the spline's extrapolation enabled as in C++.
    ///
    /// # Errors
    ///
    /// Reports an unset evaluation date, and whatever stops the first-read
    /// calculations: a failed intersection solve, an intersection outside its
    /// arbitrage bounds past the extrapolation horizon, or a bootstrap that
    /// cannot reprice its helpers.
    fn cap_price(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .cap_price(date.inner(), strike)
            .map_err(PyQlError::from)?)
    }

    /// The interpolated floor price at `date` struck at `strike`. See
    /// [`cap_price`](Self::cap_price).
    fn floor_price(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .floor_price(date.inner(), strike)
            .map_err(PyQlError::from)?)
    }

    /// The ATM year-on-year swap rate at `date`, read off the cubic curve
    /// through the cap/floor intersections. `extrapolate` defaults `True` as
    /// in C++; passed `False`, a date outside the quoted maturities is an
    /// error instead.
    ///
    /// # Errors
    ///
    /// As [`cap_price`](Self::cap_price), plus the range check when
    /// `extrapolate` is `False`.
    #[pyo3(signature = (date, extrapolate = true))]
    fn atm_yoy_swap_rate(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .atm_yoy_swap_rate(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The cap/floor strike union: every floor strike, then the cap strikes
    /// above them.
    fn strikes(&self) -> Vec<f64> {
        self.inner.strikes().to_vec()
    }

    /// The quoted maturities, one per grid column.
    fn maturities(&self) -> Vec<PyPeriod> {
        self.inner
            .maturities()
            .iter()
            .map(|&period| PyPeriod::from_inner(period))
            .collect()
    }
}

impl PyYoYCapFloorTermPriceSurface {
    /// The erased surface the optionlet stripper facades (#910) take: the
    /// concrete surface is held so the constructor's type survives, and the
    /// upcast happens here (the newtype-loses-shared_ptr-upcast pattern).
    #[allow(dead_code)]
    pub(crate) fn shared(&self) -> Shared<dyn YoYCapFloorTermPriceSurface> {
        Shared::clone(&self.inner) as Shared<dyn YoYCapFloorTermPriceSurface>
    }
}
