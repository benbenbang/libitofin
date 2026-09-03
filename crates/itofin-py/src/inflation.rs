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
//! [`PyMakeYoYInflationCapFloor`] is the only way to build, the quoted
//! [`PyYoYCapFloorTermPriceSurface`] cap/floor price grid the optionlet
//! strippers read, and the [`PyKInterpolatedYoYOptionletVolatilitySurface`]
//! stripping that grid into an optionlet volatility surface.
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
use libitofin::termstructures::TermStructure;
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
    ConstantYoYOptionletVolatility, InterpolatedYoYOptionletStripper,
    KInterpolatedYoYOptionletVolatilitySurface, VolatilityTermStructure, YoYOptionletStripper,
    YoYOptionletVolatilitySurface,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::date::Date;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::period::Period;
use pyo3::prelude::*;

/// Discounts every leg of a swap over a single yield curve.
///
/// Infallible at construction - every precondition (an empty curve handle, an
/// unset evaluation date) is reported when the swap is priced. The core's
/// include_settlement_date_flows, settlement_date and npv_date overrides are
/// not exposed and are always None, so the flow decision follows the settings'
/// own flags and both dates fall back to the curve reference date. The swap
/// this engine prices must carry the same Settings object.
#[pyclass(name = "DiscountingSwapEngine", unsendable)]
pub struct PyDiscountingSwapEngine {
    inner: SharedMut<DiscountingSwapEngine>,
}

#[pymethods]
impl PyDiscountingSwapEngine {
    /// Build an engine discounting every leg on discount.
    ///
    /// Args:
    ///     discount (YieldTermStructure): The curve every leg is discounted on;
    ///         the engine registers as an observer of it.
    ///     settings (Settings): The explicit settings; must be the same object
    ///         the swap this engine prices was built with, or the two resolve
    ///         their dates against different evaluation dates and the NPV is
    ///         silently wrong.
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

/// How a CPI observation interpolates between the index fixings bracketing it.
///
/// Flat reads the fixing of the lagged period outright; Linear advances from it
/// to the next period's fixing by how far the observation date has run into its
/// own period. The core's deprecated AsIndex variant is not ported and so has
/// no counterpart here.
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

/// A price index publishing one level per period, reading back either a
/// stored figure or a forecast off its inflation curve.
///
/// The curve is reached through a relinkable handle the index owns, so an
/// index can be built before the curve it forecasts off exists. The handle
/// starts empty and a forecast before any link raises ItofinError; link_to
/// fills it.
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
    /// Return the UK Retail Price Index: monthly, one-month availability lag.
    ///
    /// Args:
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Returns:
    ///     ZeroInflationIndex: The "UK RPI" index, over an empty curve handle.
    #[staticmethod]
    fn uk_rpi(settings: &PySettings) -> Self {
        PyZeroInflationIndex::with_curve_handle(|curve| {
            UkRpi::new(settings.inner()).with_term_structure(curve.handle())
        })
    }

    /// Return the UK harmonised index of consumer prices.
    ///
    /// Args:
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Returns:
    ///     ZeroInflationIndex: The UK HICP index, over an empty curve handle.
    #[staticmethod]
    fn uk_hicp(settings: &PySettings) -> Self {
        PyZeroInflationIndex::with_curve_handle(|curve| {
            UkHicp::new(settings.inner()).with_term_structure(curve.handle())
        })
    }

    /// Return the euro-area harmonised index of consumer prices.
    ///
    /// Args:
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Returns:
    ///     ZeroInflationIndex: The EU HICP index, over an empty curve handle.
    #[staticmethod]
    fn eu_hicp(settings: &PySettings) -> Self {
        PyZeroInflationIndex::with_curve_handle(|curve| {
            EuHicp::new(settings.inner()).with_term_structure(curve.handle())
        })
    }

    /// Return the index name, under which fixings are stored.
    ///
    /// Returns:
    ///     str: The name, e.g. "UK RPI".
    fn name(&self) -> String {
        self.inner.name()
    }

    /// Record a published figure across the whole inflation period.
    ///
    /// The figure is stored on every date of the period fixing_date falls in,
    /// so a later read on any day inside that period finds it.
    ///
    /// Args:
    ///     fixing_date (Date): Any date inside the inflation period the figure
    ///         describes.
    ///     value (float): The published index level.
    ///
    /// Raises:
    ///     ItofinError: If the index frequency has no expressible inflation
    ///         period, or a different figure is already stored on a date in
    ///         that period.
    fn add_fixing(&self, fixing_date: &PyDate, value: f64) -> PyResult<()> {
        Ok(self
            .inner
            .add_fixing(fixing_date.inner(), value)
            .map_err(PyQlError::from)?)
    }

    /// Return the fixing at fixing_date, stored or forecast off the linked curve.
    ///
    /// Args:
    ///     fixing_date (Date): The date the level is read or forecast for.
    ///     forecast_todays_fixing (bool): Accepted and ignored, as in the core:
    ///         needs_forecast alone decides between history and forecast.
    ///
    /// Returns:
    ///     float: The index level.
    ///
    /// Raises:
    ///     ItofinError: If a date the store should cover has no figure, or a
    ///         forecast is asked for with no curve linked.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// Return the first day of the period the latest stored figure describes.
    ///
    /// Returns:
    ///     Date: The start of that inflation period.
    ///
    /// Raises:
    ///     ItofinError: If the index has no fixing history.
    fn last_fixing_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.last_fixing_date().map_err(PyQlError::from)?,
        ))
    }

    /// Point the index at curve, so every forecast from here on compounds off it.
    ///
    /// Takes the ZeroInflationTermStructure base, so any subclass links. It is
    /// the curve behind that facade's handle at call time that is stored, not
    /// the handle itself: relinking the facade afterwards leaves this index on
    /// the curve it was given, and a later link_to is how it moves.
    ///
    /// Args:
    ///     curve (ZeroInflationTermStructure): The curve forecasts compound
    ///         off.
    ///
    /// Raises:
    ///     ItofinError: If curve somehow carries no link.
    fn link_to(&self, curve: &PyZeroInflationTermStructure) -> PyResult<()> {
        self.relink(curve.handle().current_link().map_err(PyQlError::from)?);
        Ok(())
    }

    /// Return whether fixing_date has to be forecast rather than read from history.
    ///
    /// Decided against the latest period that could have been published by the
    /// settings' evaluation date.
    ///
    /// Args:
    ///     fixing_date (Date): The date in question.
    ///
    /// Returns:
    ///     bool: True if the date has to be forecast off the curve.
    ///
    /// Raises:
    ///     ItofinError: If the evaluation date is unset, or the index frequency
    ///         has no expressible inflation period.
    fn needs_forecast(&self, fixing_date: &PyDate) -> PyResult<bool> {
        Ok(self
            .inner
            .needs_forecast(fixing_date.inner())
            .map_err(PyQlError::from)?)
    }

    /// Return the printable representation.
    ///
    /// Returns:
    ///     str: A string of the form ZeroInflationIndex(UK RPI).
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

/// The seasonal correction a price index carries, whose factors multiply
/// the index level itself.
///
/// The factors are given in whole multiples of the count the frequency
/// dictates - twelve for Frequency.Monthly - and are reused as long as needed,
/// so twelve of them are stationary and twenty-four repeat every two years.
/// They are not applied raw: the factor at the queried date is normalized
/// against the one at a reference date, which for a zero rate is the curve's
/// own base date, so the correction is the identity there.
///
/// Install it with ZeroInflationTermStructure.set_seasonality. Only the
/// date-taking rate query folds the correction in; the year-fraction one
/// cannot, a time not naming the date the factors are a function of.
#[pyclass(name = "MultiplicativePriceSeasonality", unsendable)]
pub struct PyMultiplicativePriceSeasonality {
    inner: Shared<MultiplicativePriceSeasonality>,
}

#[pymethods]
impl PyMultiplicativePriceSeasonality {
    /// Build the correction from a factor set anchored on a base date.
    ///
    /// Args:
    ///     seasonality_base_date (Date): The date the factor set is anchored
    ///         on.
    ///     frequency (Frequency): The frequency the factors step at.
    ///     seasonality_factors (list[float]): The factors, in order from the
    ///         base date.
    ///
    /// Raises:
    ///     ItofinError: On a frequency outside semiannual-through-daily,
    ///         Frequency.Annual among them; on an empty factor set; and on a
    ///         factor count that is not a whole multiple of the frequency.
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

    /// Return the date the factor set is anchored on.
    ///
    /// Returns:
    ///     Date: The seasonality base date.
    fn seasonality_base_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.seasonality_base_date())
    }

    /// Return the frequency the factors step at.
    ///
    /// Returns:
    ///     Frequency: The stepping frequency.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(self.inner.frequency())
    }

    /// Return the factors, in order from the seasonality base date.
    ///
    /// Returns:
    ///     list[float]: The factor set as given.
    fn seasonality_factors(&self) -> Vec<f64> {
        self.inner.seasonality_factors().to_vec()
    }

    /// Return the raw factor covering to, before any normalization.
    ///
    /// This is not the correction the curve applies, which is normalized
    /// against a reference date. The offset from the seasonality base date is
    /// counted in whole factor periods and wrapped modulo the factor count, so
    /// a set shorter than the span repeats and dates before the anchor wrap
    /// backwards.
    ///
    /// Args:
    ///     to (Date): The date the factor is read at.
    ///
    /// Returns:
    ///     float: The raw seasonality factor.
    ///
    /// Raises:
    ///     ItofinError: On a year-based factor period, which cannot express
    ///         seasonality.
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

/// Shared base for every zero-coupon inflation curve: the zero-coupon
/// inflation rate in a year-fraction and a date form, the base date, the
/// fixing frequency and the seasonality correction the curve carries.
///
/// The two rate reads are not interchangeable. zero_rate_date snaps its date
/// to the start of the inflation period containing it, because a fixing
/// applies to a whole period; zero_rate takes a year-fraction already measured
/// under the curve's own day counter and quantizes nothing. Only the first
/// folds in any seasonality.
#[pyclass(name = "ZeroInflationTermStructure", subclass, unsendable)]
pub struct PyZeroInflationTermStructure {
    inner: Handle<dyn ZeroInflationTermStructure>,
}

#[pymethods]
impl PyZeroInflationTermStructure {
    /// Return the zero-coupon inflation rate at year-fraction t.
    ///
    /// Quoted on the yearly compounding zero-coupon swaps assume. Nothing here
    /// accounts for observation lags or period interpolation: the caller
    /// manages those.
    ///
    /// Args:
    ///     t (float): The year fraction, measured with the curve's own day
    ///         counter; it is negative for the base period.
    ///     extrapolate (bool): Whether to answer past the curve's range.
    ///
    /// Returns:
    ///     float: The zero-coupon inflation rate.
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
            .zero_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the zero-coupon inflation rate for the period containing date.
    ///
    /// The date is quantized to that period's first day before both the range
    /// check and the time conversion, so every day inside one period reads the
    /// same rate. This is the only form that folds in seasonality.
    ///
    /// Args:
    ///     date (Date): The date the rate is read at.
    ///     extrapolate (bool): Whether to answer past the curve's range.
    ///
    /// Returns:
    ///     float: The zero-coupon inflation rate for that period.
    ///
    /// Raises:
    ///     ItofinError: If the period is past the curve's range and
    ///         extrapolation is not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn zero_rate_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .zero_rate_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the base date, the last date for which the fixing is known.
    ///
    /// Returns:
    ///     Date: The base date; it precedes the reference date, so its year
    ///         fraction is negative.
    fn base_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .base_date(),
        ))
    }

    /// Return the frequency of the inflation fixings the curve is built on.
    ///
    /// Returns:
    ///     Frequency: The fixing frequency.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .frequency(),
        )
    }

    /// Install seasonality on the curve, replacing whatever it carried.
    ///
    /// A curve that caches anything derived from the correction - every
    /// bootstrapped one does - is invalidated here, so the next read re-solves
    /// against the new correction.
    ///
    /// Args:
    ///     seasonality (MultiplicativePriceSeasonality | None): The correction
    ///         to install; None clears it.
    ///
    /// Raises:
    ///     ItofinError: From the consistency gate, which a multi-year factor
    ///         set fails, that comparison being a documented core deferral
    ///         (#807). The store happens before the gate runs, as C++'s does,
    ///         so a rejected correction is left installed and unannounced:
    ///         clear it with None before reading the curve again.
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

    /// Return whether the curve carries a seasonality correction.
    ///
    /// Returns:
    ///     bool: True also for a correction left installed by a
    ///         set_seasonality that raised.
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

/// A zero-coupon inflation curve built from (date, zero-rate) nodes,
/// interpolating linearly in zero-rate space.
///
/// The first date is the base date rather than the reference date, which is
/// passed separately and normally follows it; node times are measured from the
/// reference date, so the first one is negative.
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
    /// Build the curve through the rates quoted at dates.
    ///
    /// Linear is pinned at the boundary: it is the interpolator the C++ zero
    /// inflation curve typedef fixes, so no interpolation argument is offered.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date, given separately
    ///         and normally following the base date.
    ///     dates (list[Date]): The node dates, the first being the base date.
    ///     rates (list[float]): The zero rate at each node.
    ///     frequency (Frequency): The frequency of the inflation fixings.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On fewer than two dates, a dates and rates count
    ///         mismatch, a rate at or below -100 per cent from the second node
    ///         on, or unsorted dates.
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

    /// Return the node times, measured from the reference date.
    ///
    /// Returns:
    ///     list[float]: The node times; the first is negative whenever the
    ///         base date precedes the reference date.
    fn times(&self) -> Vec<f64> {
        self.concrete.times().to_vec()
    }

    /// Return the node dates.
    ///
    /// Returns:
    ///     list[Date]: The nodes, the first of which is the base date.
    fn dates(&self) -> Vec<PyDate> {
        self.concrete
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }

    /// Return the curve's nodes as pairs.
    ///
    /// Returns:
    ///     list[tuple[Date, float]]: One (date, zero rate) pair per node.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.concrete
            .nodes()
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect()
    }
}

/// Shared base for every zero-inflation bootstrap helper: the two dates the
/// bootstrap places a curve node by.
///
/// Concrete helpers such as ZeroCouponInflationSwapHelper subclass this and
/// supply only their constructor.
#[pyclass(name = "ZeroInflationHelper", subclass, unsendable)]
pub struct PyZeroInflationHelper {
    inner: Shared<dyn ZeroInflationHelper>,
}

#[pymethods]
impl PyZeroInflationHelper {
    /// Return the date the curve node this helper sets sits at.
    ///
    /// Returns:
    ///     Date: The pillar date.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// Return the latest date the helper needs curve data at.
    ///
    /// Returns:
    ///     Date: The latest date, equal to the pillar date.
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

/// The bootstrap helper fitting a zero-coupon inflation swap quoted as a
/// rate.
///
/// The helper prices a unit-notional, zero-strike swap of its own and reports
/// that contract's fair rate; the bootstrap drives the quoted rate less that
/// fair rate to zero. The swap starts at the evaluation date, so that date must
/// be set before this constructor runs, not merely before the bootstrap.
///
/// It prices through a copy of index linked to a handle of its own, so the
/// caller's index need not be linked to any curve.
///
/// pillar picks which of the two nodes an interpolated swap straddles the helper
/// fits; a flat swap reads a single fixing and ignores it.
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
    /// Build the helper on a swap maturing at maturity.
    ///
    /// It needs no nominal curve, building itself a flat zero-rate one,
    /// because both legs pay on the same adjusted maturity and their discount
    /// factors cancel out of the fair rate.
    ///
    /// Args:
    ///     quote (SimpleQuote): The quoted swap rate; the caller keeps it, so
    ///         a later set_value re-drives the bootstrap.
    ///     swap_obs_lag (Period): How far back the maturity fixing is
    ///         observed.
    ///     maturity (Date): The swap's maturity.
    ///     calendar (Calendar): The calendar the payment rolls on.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         payment date.
    ///     day_counter (DayCounter): The day count the fixed amount accrues
    ///         on.
    ///     index (ZeroInflationIndex): The index observed; the helper prices
    ///         through a copy linked to a handle of its own, so the caller's
    ///         index need not be linked to any curve.
    ///     observation_interpolation (CpiInterpolationType): How the observed
    ///         fixing is interpolated.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the swap starts at, which must be set before this
    ///         constructor runs.
    ///     pillar (Pillar): Which of the two nodes an interpolated swap
    ///         straddles the helper fits; a flat swap reads a single fixing
    ///         and ignores it.
    ///
    /// Raises:
    ///     ItofinError: On an observation lag the index cannot observe
    ///         through, and under Linear interpolation on one that leaves less
    ///         than a whole index period over the index's availability lag.
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

    /// Return the maturity observation date on the helper's own swap.
    ///
    /// Read off the cached contract's indexed flow, so it reports the date the
    /// helper actually prices at rather than one recomputed here. It is not
    /// pillar_date, which is the first day of the period containing it, that
    /// quantization being the helper's rounding and not the contract's.
    ///
    /// Returns:
    ///     Date: The maturity less the observation lag, unsnapped.
    ///
    /// Raises:
    ///     ItofinError: If the cached swap carries the error that stopped it
    ///         being built, notably an evaluation date that was never set.
    fn inflation_fixing_date(&self) -> PyResult<PyDate> {
        let swap = self.concrete.swap();
        let swap = swap
            .as_ref()
            .map_err(|error| PyQlError::from(error.clone()))?;
        Ok(PyDate::from_inner(swap.inflation_cash_flow().fixing_date()))
    }
}

/// A zero-coupon inflation curve bootstrapped from inflation helpers,
/// solving one zero-rate node per helper fixing period.
///
/// Node zero sits on base_date, not on reference_date, so times()[0] is
/// negative - the one structural difference from every other piecewise curve.
///
/// Lazy: the bootstrap runs on the first read, so the evaluation date must be
/// in place before that read as well as before the helpers were built. A helper
/// quote moving invalidates the cache.
///
/// A seasonality installed later through set_seasonality() invalidates the
/// bootstrap, so the next read re-solves every node against the correction.
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
    /// Build the curve over helpers, registering on them without solving.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     base_date (Date): The last date for which a fixing is known, where
    ///         node zero sits.
    ///     frequency (Frequency): The frequency of the inflation fixings.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     helpers (list[ZeroInflationHelper]): The bootstrap instruments.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
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

    /// Run the bootstrap if the cache is stale.
    ///
    /// Calling it explicitly makes a solver failure surface here rather than
    /// inside a later query.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn calculate(&self) -> PyResult<()> {
        Ok(self.concrete.calculate().map_err(PyQlError::from)?)
    }

    /// Return the node times, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[float]: The nodes measured from the reference date; the first
    ///         is negative, node zero sitting on the base date.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.times().map_err(PyQlError::from)?)
    }

    /// Return the node dates, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[Date]: The nodes, the first of which is the base date.
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

    /// Return the solved nodes as pairs, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[tuple[Date, float]]: One (date, zero rate) pair per node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
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

/// One fixed flow against one inflation-indexed flow, both exchanged at maturity.
///
/// fixed_rate is the K that at inception matches the inflation growth.
/// SwapType names the inflation leg, so a Payer pays inflation and receives
/// fixed.
///
/// maturity is pre-adjustment: each leg's payment date is it rolled on that
/// leg's calendar and convention, while the year fraction behind the fixed
/// amount stays on the raw date. inflation_calendar and inflation_convention
/// fall back to the fixed-leg ones when None.
///
/// The core omits adjust_inf_obs_dates from its own signature, so there is
/// nothing to expose here; the leg and cash-flow accessors are not surfaced
/// either, there being no cash-flow facade.
///
/// Pricing needs an engine: call set_engine() before npv(). fair_rate() is the
/// exception, reading the indexed flow directly and pricing with no engine at
/// all, though it does need the index linked to a curve.
#[pyclass(name = "ZeroCouponInflationSwap", unsendable)]
pub struct PyZeroCouponInflationSwap {
    inner: SharedMut<ZeroCouponInflationSwap>,
}

#[pymethods]
impl PyZeroCouponInflationSwap {
    /// Build the swap from its two exchanged flows.
    ///
    /// Args:
    ///     swap_type (SwapType): Which side the inflation leg is seen from; a
    ///         Payer pays inflation and receives fixed.
    ///     nominal (float): The notional both flows are quoted on.
    ///     start_date (Date): The inception the index ratio is measured from.
    ///     maturity (Date): The raw, pre-adjustment maturity.
    ///     fixed_calendar (Calendar): The calendar the fixed payment rolls on.
    ///     fixed_convention (BusinessDayConvention): The roll applied to the
    ///         fixed payment date.
    ///     day_counter (DayCounter): The day count behind the fixed amount,
    ///         which stays on the raw maturity.
    ///     fixed_rate (float): The K that at inception matches the inflation
    ///         growth.
    ///     inflation_index (ZeroInflationIndex): The index the indexed flow
    ///         observes.
    ///     observation_lag (Period): How far back the maturity fixing is
    ///         observed.
    ///     observation_interpolation (CpiInterpolationType): How the observed
    ///         fixing is interpolated.
    ///     inflation_calendar (Calendar | None): The calendar the inflation
    ///         payment rolls on; None falls back to fixed_calendar.
    ///     inflation_convention (BusinessDayConvention | None): The roll
    ///         applied to the inflation payment date; None falls back to
    ///         fixed_convention.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If the observation lag is too short for the index to
    ///         observe fixings that exist, which under Linear interpolation
    ///         costs a further publication period.
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

    /// Attach a discounting engine so the swap prices.
    ///
    /// Args:
    ///     engine (DiscountingSwapEngine): The engine, which must resolve its
    ///         dates against the same Settings object as this swap.
    fn set_engine(&mut self, engine: &PyDiscountingSwapEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the engine refuses the swap.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Attach engine and return the NPV.
    ///
    /// Args:
    ///     engine (DiscountingSwapEngine): The engine to install and price on.
    ///
    /// Returns:
    ///     float: The swap value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, engine: &PyDiscountingSwapEngine) -> PyResult<f64> {
        self.set_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Return a frozen snapshot of the valuation, calculating first.
    ///
    /// Returns:
    ///     Results: A copy of the valuation results.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// Return the swap NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, and if no curve is linked
    ///         into the index, which leaves the indexed flow unforecastable.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// Return the index ratio de-compounded over the swap's own year fraction.
    ///
    /// Needs no engine - it reads the indexed flow rather than any priced
    /// result.
    ///
    /// Returns:
    ///     float: The rate that would price the swap at zero.
    ///
    /// Raises:
    ///     ItofinError: If no curve is linked into the index, the flow's
    ///         amount being a forecast off the inflation curve.
    fn fair_rate(&self) -> PyResult<f64> {
        Ok(self.inner.borrow().fair_rate().map_err(PyQlError::from)?)
    }

    /// Return the fixed leg's NPV, priced on demand.
    ///
    /// Returns:
    ///     float: The present value of the fixed flow.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions npv reports.
    fn fixed_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// Return the inflation leg's NPV, priced on demand.
    ///
    /// Returns:
    ///     float: The present value of the indexed flow.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions npv reports.
    fn inflation_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .inflation_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// Return the fixed leg's sensitivity to a basis point on the quoted rate.
    ///
    /// Computed in closed form rather than read off the engine, whose own leg
    /// BPS is zero for a non-coupon flow.
    ///
    /// Returns:
    ///     float: The basis-point value of the fixed flow.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions npv reports, the calculation
    ///         needing the engine's discount factor at the fixed leg's end
    ///         date.
    fn fixed_leg_bps(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_leg_bps()
            .map_err(PyQlError::from)?)
    }

    /// Return the contract maturity, raw and pre-adjustment.
    ///
    /// Returns:
    ///     Date: The maturity, which is not either leg's payment date.
    fn maturity_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.borrow().maturity_date())
    }

    /// Return the date the maturity fixing is observed at.
    ///
    /// Returns:
    ///     Date: The maturity less the observation lag, unsnapped.
    fn obs_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.borrow().obs_date())
    }

    /// Return the observation date, read off the indexed flow.
    ///
    /// Both names are kept because both exist in the core, and the oracle
    /// asserts they coincide.
    ///
    /// Returns:
    ///     Date: The same date as obs_date.
    fn inflation_fixing_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.borrow().inflation_cash_flow().fixing_date())
    }
}

/// Shared base for every year-on-year inflation curve: the year-on-year
/// rate in a year-fraction and a date form, the base date, the base rate, the
/// fixing frequency and the seasonality correction the curve carries.
///
/// The two rate reads are not interchangeable. yoy_rate_date snaps its date to
/// the start of the inflation period containing it and is the only one that
/// folds in any seasonality; yoy_rate takes a year-fraction already measured
/// under the curve's own day counter and quantizes nothing. Neither is the
/// year-on-year swap rate, which comes from the instrument.
///
/// base_rate is answered here where the zero base defers it: a year-on-year
/// curve carries the rate observed over the period ending on its base date.
#[pyclass(name = "YoYInflationTermStructure", subclass, unsendable)]
pub struct PyYoYInflationTermStructure {
    inner: Handle<dyn YoYInflationTermStructure>,
}

#[pymethods]
impl PyYoYInflationTermStructure {
    /// Return the year-on-year inflation rate at year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, measured with the curve's own day
    ///         counter; it is negative for the base period.
    ///     extrapolate (bool): Whether to answer past the curve's range.
    ///
    /// Returns:
    ///     float: The year-on-year rate, which is not the year-on-year swap
    ///         rate: that comes from the instrument.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (t, extrapolate = false))]
    fn yoy_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .yoy_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the year-on-year rate for the inflation period containing date.
    ///
    /// The date is quantized to that period's first day before both the range
    /// check and the time conversion. Any seasonality correction is folded in
    /// last, at the original date rather than the period start, as C++ does on
    /// this path.
    ///
    /// Args:
    ///     date (Date): The date the rate is read at.
    ///     extrapolate (bool): Whether to answer past the curve's range.
    ///
    /// Returns:
    ///     float: The year-on-year rate for that period.
    ///
    /// Raises:
    ///     ItofinError: If the period is past the curve's range and
    ///         extrapolation is not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn yoy_rate_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .yoy_rate_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the base date, the last date for which the fixing is known.
    ///
    /// Returns:
    ///     Date: The base date; it precedes the reference date, so its year
    ///         fraction is negative.
    fn base_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .base_date(),
        ))
    }

    /// Return the rate observed over the period ending on the base date.
    ///
    /// Returns:
    ///     float: The base rate, which node zero is seeded with and keeps.
    ///
    /// Raises:
    ///     ItofinError: On a curve that carries no base rate.
    fn base_rate(&self) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .base_rate()
            .map_err(PyQlError::from)?)
    }

    /// Return the frequency of the inflation fixings the curve is built on.
    ///
    /// Returns:
    ///     Frequency: The fixing frequency.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(
            self.inner
                .current_link()
                .map_err(PyQlError::from)?
                .frequency(),
        )
    }

    /// Install seasonality on the curve, replacing whatever it carried.
    ///
    /// Args:
    ///     seasonality (MultiplicativePriceSeasonality | None): The correction
    ///         to install; None clears it.
    ///
    /// Raises:
    ///     ItofinError: From the consistency gate, which leaves a rejected
    ///         correction installed as C++ does; see the zero-curve base for
    ///         the full account.
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

    /// Return whether the curve carries a seasonality correction.
    ///
    /// Returns:
    ///     bool: True also for a correction left installed by a
    ///         set_seasonality that raised.
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

/// A year-on-year inflation curve built from (date, year-on-year rate)
/// nodes, interpolating linearly in rate space.
///
/// The first date is the base date rather than the reference date, which is
/// passed separately and normally follows it; the first rate is the base rate
/// the curve publishes, and node times are measured from the reference date, so
/// the first one is negative.
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
    /// Build the curve through the rates quoted at dates.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date, given separately
    ///         and normally following the base date.
    ///     dates (list[Date]): The node dates, the first being the base date.
    ///     rates (list[float]): The year-on-year rate at each node; the first
    ///         is the base rate the curve publishes.
    ///     frequency (Frequency): The frequency of the inflation fixings.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On fewer than two dates, a dates and rates count
    ///         mismatch, or a rate at or below -100 per cent from the second
    ///         node on; the base rate is left unconstrained.
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

    /// Return the node times, measured from the reference date.
    ///
    /// Returns:
    ///     list[float]: The node times; the first is negative whenever the
    ///         base date precedes the reference date.
    fn times(&self) -> Vec<f64> {
        self.concrete.times().to_vec()
    }

    /// Return the node dates.
    ///
    /// Returns:
    ///     list[Date]: The nodes, the first of which is the base date.
    fn dates(&self) -> Vec<PyDate> {
        self.concrete
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }

    /// Return the curve's nodes as pairs.
    ///
    /// Returns:
    ///     list[tuple[Date, float]]: One (date, year-on-year rate) pair per
    ///         node.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.concrete
            .nodes()
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect()
    }
}

/// Shared base for every year-on-year bootstrap helper: the two dates the
/// bootstrap places a curve node by.
///
/// Concrete helpers such as YearOnYearInflationSwapHelper subclass this and
/// supply only their constructor.
#[pyclass(name = "YoYInflationHelper", subclass, unsendable)]
pub struct PyYoYInflationHelper {
    inner: Shared<dyn YoYInflationHelper>,
}

#[pymethods]
impl PyYoYInflationHelper {
    /// Return the date the curve node this helper sets sits at.
    ///
    /// Returns:
    ///     Date: The pillar date.
    fn pillar_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.pillar_date())
    }

    /// Return the latest date the helper needs curve data at.
    ///
    /// Returns:
    ///     Date: The latest date, equal to the pillar date.
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

/// An index publishing one year-on-year inflation rate per period, read
/// back as a stored figure or forecast off its year-on-year curve.
///
/// Two forms. A ratio index (from_underlying) derives its rate from two
/// ZeroInflationIndex fixings a year apart and owns no history of its own; a
/// quoted one (the constructor) is published as a rate in its own right and
/// keeps its own history through add_fixing.
///
/// Both forms link to a relinkable handle the index owns, so an index can be
/// built before the curve it forecasts off exists. The handle starts empty and
/// a forecast before any link raises ItofinError; link_to fills it.
///
/// The quoted constructor spells its region and currency out as their component
/// fields: neither core type has a Python facade, and defaulting the currency
/// metadata would put made-up values on the index.
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
    /// Build a quoted year-on-year index, keeping its own fixing history.
    ///
    /// The rate is published in its own right rather than derived from a price
    /// index, so fixings are filed here through add_fixing.
    ///
    /// Args:
    ///     family_name (str): The index family the fixings are stored under.
    ///     region_name (str): The name of the region the index measures.
    ///     region_code (str): The region's code.
    ///     revised (bool): Whether the published figures are subject to
    ///         revision.
    ///     frequency (Frequency): How often the index publishes.
    ///     availability_lag (Period): How long after a period ends its figure
    ///         is published.
    ///     currency_name (str): The currency's name.
    ///     currency_code (str): The currency's ISO 4217 three-letter code.
    ///     currency_numeric_code (int): The currency's ISO 4217 numeric code.
    ///     currency_symbol (str): The currency's symbol.
    ///     currency_fraction_symbol (str): The symbol of the currency's
    ///         fractional unit.
    ///     currency_fractions_per_unit (int): How many fractional units make
    ///         one currency unit.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
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

    /// Build a ratio index dividing a price index's figure by its figure a year earlier.
    ///
    /// The metadata is inherited bar the family name, which is prefixed YYR_,
    /// so a "UK RPI" underlying yields "UK YYR_RPI"; fixings belong on the
    /// underlying.
    ///
    /// Args:
    ///     underlying (ZeroInflationIndex): The price index whose consecutive
    ///         figures the rate is derived from.
    ///
    /// Returns:
    ///     YoYInflationIndex: The ratio index, over an empty curve handle.
    #[staticmethod]
    fn from_underlying(underlying: &Bound<'_, PyZeroInflationIndex>) -> Self {
        let zero = underlying.borrow().shared();
        let handle = underlying.clone().unbind();
        PyYoYInflationIndex::with_curve_handle(Some(handle), |curve| {
            YoYInflationIndex::from_underlying(zero).with_term_structure(curve.handle())
        })
    }

    /// Return the index name, under which fixings are stored.
    ///
    /// Returns:
    ///     str: The name, e.g. "UK YYR_RPI".
    fn name(&self) -> String {
        self.inner.name()
    }

    /// Return whether this index is the ratio of two price-index fixings.
    ///
    /// Returns:
    ///     bool: True for a ratio index, False for a quoted rate.
    fn ratio(&self) -> bool {
        self.inner.ratio()
    }

    /// Return the price index a ratio index divides, None on a quoted one.
    ///
    /// This is the very object from_underlying was handed, not a fresh facade
    /// around the same core index: a rebuilt one would carry a relinkable
    /// handle this index never sees, so linking it would silently forecast off
    /// nothing.
    ///
    /// Returns:
    ///     ZeroInflationIndex | None: The underlying price index, or None.
    fn underlying_index(&self, py: Python<'_>) -> Option<Py<PyZeroInflationIndex>> {
        self.underlying
            .as_ref()
            .map(|underlying| underlying.clone_ref(py))
    }

    /// Record a published year-on-year rate across the whole inflation period.
    ///
    /// A ratio index reads the underlying's history, so filing here records a
    /// figure it will never consult.
    ///
    /// Args:
    ///     fixing_date (Date): Any date inside the inflation period the rate
    ///         describes.
    ///     value (float): The published year-on-year rate.
    ///
    /// Raises:
    ///     ItofinError: If the index frequency has no expressible inflation
    ///         period, or a different figure is already stored on a date in
    ///         that period.
    fn add_fixing(&self, fixing_date: &PyDate, value: f64) -> PyResult<()> {
        Ok(self
            .inner
            .add_fixing(fixing_date.inner(), value)
            .map_err(PyQlError::from)?)
    }

    /// Return the rate at fixing_date, stored or forecast off the linked curve.
    ///
    /// Args:
    ///     fixing_date (Date): The date the rate is read or forecast for.
    ///     forecast_todays_fixing (bool): Accepted and ignored, as in the core:
    ///         needs_forecast alone decides between history and forecast.
    ///
    /// Returns:
    ///     float: The year-on-year inflation rate.
    ///
    /// Raises:
    ///     ItofinError: If a forecast is asked for with no curve linked.
    #[pyo3(signature = (fixing_date, forecast_todays_fixing = false))]
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// Return the first day of the period the latest figure on record describes.
    ///
    /// Read off the underlying on a ratio index.
    ///
    /// Returns:
    ///     Date: The start of that inflation period.
    ///
    /// Raises:
    ///     ItofinError: If the index has no fixing history.
    fn last_fixing_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.last_fixing_date().map_err(PyQlError::from)?,
        ))
    }

    /// Point the index at curve, so every forecast from here on reads it.
    ///
    /// Takes the YoYInflationTermStructure base, so any subclass links. It is
    /// the curve behind that facade's handle at call time that is stored, not
    /// the handle itself.
    ///
    /// Args:
    ///     curve (YoYInflationTermStructure): The curve forecasts are read off.
    ///
    /// Raises:
    ///     ItofinError: If curve somehow carries no link.
    fn link_to(&self, curve: &PyYoYInflationTermStructure) -> PyResult<()> {
        self.curve
            .link_to(curve.handle().current_link().map_err(PyQlError::from)?);
        Ok(())
    }

    /// Return whether fixing_date has to be forecast rather than read from history.
    ///
    /// A ratio index defers the question to its underlying.
    ///
    /// Args:
    ///     fixing_date (Date): The date in question.
    ///
    /// Returns:
    ///     bool: True if the date has to be forecast off the curve.
    ///
    /// Raises:
    ///     ItofinError: If the evaluation date is unset, or the index frequency
    ///         has no expressible inflation period.
    fn needs_forecast(&self, fixing_date: &PyDate) -> PyResult<bool> {
        Ok(self
            .inner
            .needs_forecast(fixing_date.inner())
            .map_err(PyQlError::from)?)
    }

    /// Return the printable representation.
    ///
    /// Returns:
    ///     str: A string of the form YoYInflationIndex(UK YYR_RPI).
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

/// The bootstrap helper fitting a year-on-year inflation swap quoted as a
/// rate.
///
/// The helper prices a unit-notional, zero-strike swap of its own and reports
/// that contract's fair rate; the bootstrap drives the quoted rate less that
/// fair rate to zero. Unlike its zero-coupon twin it does need a nominal curve:
/// the year-on-year legs pay on a schedule of dates rather than one, so their
/// discount factors do not cancel.
///
/// The swap starts at the evaluation date, so that date must be set before this
/// constructor runs, not merely before the bootstrap. It prices through a copy
/// of index linked to a handle of its own, so the caller's index need not be
/// linked to any curve.
///
/// pillar is accepted for signature parity but never read: it only ever
/// discriminates on the interpolated path, which is refused.
///
/// Fallible: CpiInterpolationType.Linear is refused outright, and the swap is
/// built here, so an observation lag its legs cannot be built under fails at
/// construction.
#[pyclass(
    name = "YearOnYearInflationSwapHelper",
    extends = PyYoYInflationHelper,
    unsendable
)]
pub struct PyYearOnYearInflationSwapHelper;

#[pymethods]
impl PyYearOnYearInflationSwapHelper {
    /// Build the helper on a swap maturing at maturity.
    ///
    /// Args:
    ///     quote (SimpleQuote): The quoted swap rate; the caller keeps it, so
    ///         a later set_value re-drives the bootstrap.
    ///     swap_obs_lag (Period): How far back each coupon's fixings are
    ///         observed.
    ///     maturity (Date): The swap's maturity.
    ///     calendar (Calendar): The calendar the payments roll on.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         payment dates.
    ///     day_counter (DayCounter): The day count the legs accrue on.
    ///     index (YoYInflationIndex): The index observed; the helper prices
    ///         through a copy linked to a handle of its own, so the caller's
    ///         index need not be linked to any curve.
    ///     interpolation (CpiInterpolationType): How the observed fixings are
    ///         interpolated.
    ///     nominal_term_structure (YieldTermStructure): The discount curve,
    ///         which this helper does need: its legs pay on a schedule of
    ///         dates rather than one, so their discount factors do not cancel.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the swap starts at, which must be set before this
    ///         constructor runs.
    ///     pillar (Pillar): Accepted for signature parity but never read; it
    ///         only ever discriminates on the interpolated path.
    ///
    /// Raises:
    ///     ItofinError: On Linear interpolation, which the core refuses
    ///         outright pending the interpolated branch (#847), and on an
    ///         observation lag the helper's own swap legs cannot be built
    ///         under.
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

/// A year-on-year inflation curve bootstrapped from year-on-year helpers,
/// solving one rate node per helper fixing period.
///
/// Node zero sits on base_date at base_yoy_rate and is kept rather than solved,
/// so times()[0] is negative. Each helper's observed fixing period marks a
/// later segment boundary.
///
/// Lazy: the bootstrap runs on the first read, so the evaluation date must be
/// in place before that read as well as before the helpers were built. A helper
/// quote moving invalidates the cache.
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
    /// Build the curve over helpers, registering on them without solving.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     base_date (Date): The last date for which a fixing is known, where
    ///         node zero sits.
    ///     base_yoy_rate (float): The rate node zero is seeded with and keeps,
    ///         rather than solved for.
    ///     frequency (Frequency): The frequency of the inflation fixings.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     helpers (list[YoYInflationHelper]): The bootstrap instruments.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
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

    /// Run the bootstrap if the cache is stale.
    ///
    /// Calling it explicitly makes a solver failure surface here rather than
    /// inside a later query.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn calculate(&self) -> PyResult<()> {
        Ok(self.concrete.calculate().map_err(PyQlError::from)?)
    }

    /// Return the node times, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[float]: The nodes measured from the reference date; the first
    ///         is negative, node zero sitting on the base date.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.times().map_err(PyQlError::from)?)
    }

    /// Return the node dates, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[Date]: The nodes, the first of which is the base date.
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

    /// Return the solved nodes as pairs, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[tuple[Date, float]]: One (date, year-on-year rate) pair per
    ///         node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
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

/// A fixed leg against a leg of year-on-year inflation coupons, both paid over a schedule.
///
/// SwapType names the fixed leg, so a Payer pays fixed and receives inflation -
/// the opposite reading from ZeroCouponInflationSwap, where it names the
/// inflation leg.
///
/// The two schedules are independent inputs. The fixed leg takes its payment
/// calendar from its own schedule while the year-on-year leg pays on
/// payment_calendar; both adjust with payment_convention. spread is added to
/// every forecast rate on the year-on-year leg.
///
/// Pricing needs an engine: call set_engine first. Every priced accessor drives
/// the calculation, so all of them mutate.
#[pyclass(name = "YearOnYearInflationSwap", unsendable)]
pub struct PyYearOnYearInflationSwap {
    inner: SharedMut<YearOnYearInflationSwap>,
}

#[pymethods]
impl PyYearOnYearInflationSwap {
    /// Build the swap from its two schedules.
    ///
    /// Args:
    ///     swap_type (SwapType): Which side the fixed leg is seen from; a
    ///         Payer pays fixed and receives inflation.
    ///     nominal (float): The notional both legs accrue on.
    ///     fixed_schedule (Schedule): The fixed leg's payment schedule, which
    ///         also supplies its payment calendar.
    ///     fixed_rate (float): The rate the fixed leg accrues at.
    ///     fixed_day_count (DayCounter): The day count of the fixed leg.
    ///     yoy_schedule (Schedule): The year-on-year leg's schedule.
    ///     yoy_index (YoYInflationIndex): The index the coupons fix off.
    ///     observation_lag (Period): How far back each coupon's fixings are
    ///         observed.
    ///     interpolation (CpiInterpolationType): How the observed fixings are
    ///         interpolated.
    ///     spread (float): Added to every forecast rate on the year-on-year
    ///         leg.
    ///     yoy_day_count (DayCounter): The day count of the year-on-year leg.
    ///     payment_calendar (Calendar): The calendar the year-on-year leg pays
    ///         on.
    ///     payment_convention (BusinessDayConvention): The roll both legs
    ///         adjust their payment dates with.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If either leg cannot be built, notably from an
    ///         observation lag that leaves a coupon unbuildable.
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

    /// Attach a discounting engine so the swap prices.
    ///
    /// Args:
    ///     engine (DiscountingSwapEngine): The engine, which must resolve its
    ///         dates against the same Settings object this swap was built
    ///         with.
    fn set_engine(&mut self, engine: &PyDiscountingSwapEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the engine refuses the swap.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Attach engine and return the NPV.
    ///
    /// Args:
    ///     engine (DiscountingSwapEngine): The engine to install and price on.
    ///
    /// Returns:
    ///     float: The swap value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, engine: &PyDiscountingSwapEngine) -> PyResult<f64> {
        self.set_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Return a frozen snapshot of the valuation, calculating first.
    ///
    /// Returns:
    ///     Results: A copy of the valuation results.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// Return the swap NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, and if no curve is linked
    ///         into the index, which leaves the coupons unforecastable.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// Return the fixed rate that would price the swap at zero.
    ///
    /// Recovered from the NPV and the fixed leg's BPS, so it prices on demand
    /// and needs an engine.
    ///
    /// Returns:
    ///     float: The fair fixed rate.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions npv reports.
    fn fair_rate(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_rate()
            .map_err(PyQlError::from)?)
    }

    /// Return the spread over the index that would price the swap at zero.
    ///
    /// Recovered off the year-on-year leg.
    ///
    /// Returns:
    ///     float: The fair spread.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions fair_rate reports.
    fn fair_spread(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_spread()
            .map_err(PyQlError::from)?)
    }

    /// Return the fixed leg's NPV, priced on demand.
    ///
    /// Returns:
    ///     float: The present value of the fixed leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions npv reports.
    fn fixed_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fixed_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// Return the year-on-year leg's NPV, priced on demand.
    ///
    /// Returns:
    ///     float: The present value of the inflation leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions npv reports.
    fn yoy_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .yoy_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// Return the quoted fixed rate the swap was struck at.
    ///
    /// Returns:
    ///     float: The fixed-leg rate.
    fn fixed_rate(&self) -> f64 {
        self.inner.borrow().fixed_rate()
    }

    /// Return the spread the year-on-year coupons carry over the index.
    ///
    /// Returns:
    ///     float: The quoted spread.
    fn spread(&self) -> f64 {
        self.inner.borrow().spread()
    }
}

/// One year-on-year optionlet volatility for every strike and every date.
///
/// The reference date moves with the evaluation date carried by settings,
/// settlement_days business days on from it, so that date must be set before
/// anything is priced off the surface.
///
/// min_strike and max_strike bound the strike domain a query is answered over;
/// C++ defaults them to -1.0 and 100.0 and the port carries no default
/// arguments, so both are passed here too.
///
/// Both constructors are bound: __init__ takes a value, with_quote a live
/// quote. The whole stripped/interpolated hierarchy is deferred (#874).
#[pyclass(name = "ConstantYoYOptionletVolatility", unsendable)]
pub struct PyConstantYoYOptionletVolatility {
    inner: Shared<ConstantYoYOptionletVolatility>,
}

#[pymethods]
impl PyConstantYoYOptionletVolatility {
    /// Build a flat surface at a fixed volatility.
    ///
    /// Nothing is resolved here: the reference date, the base date and the
    /// strike range are all read at query time, so an unset evaluation date
    /// surfaces then rather than now.
    ///
    /// Args:
    ///     volatility (float): The single volatility answered everywhere.
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar those days are counted on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a date.
    ///     day_counter (DayCounter): The day count times are measured in.
    ///     observation_lag (Period): The lag the surface itself observes
    ///         inflation with.
    ///     frequency (Frequency): How often the observed index publishes.
    ///     index_is_interpolated (bool): Whether the observed index
    ///         interpolates between publications.
    ///     min_strike (float): The lower bound of the strike domain; C++
    ///         defaults it to -1.0 and the port carries no default arguments.
    ///     max_strike (float): The upper bound of the strike domain; C++
    ///         defaults it to 100.0.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the reference date moves with.
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

    /// Build a flat surface reading its volatility from a live quote.
    ///
    /// The quote is retained rather than read once, so a later set_value on it
    /// notifies the surface's observers and anything priced off the surface
    /// reprices at the new level. Otherwise as __init__, which the arguments
    /// after the first mirror exactly.
    ///
    /// Args:
    ///     volatility (SimpleQuote): The volatility answered everywhere.
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar those days are counted on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a date.
    ///     day_counter (DayCounter): The day count times are measured in.
    ///     observation_lag (Period): The lag the surface itself observes
    ///         inflation with.
    ///     frequency (Frequency): How often the observed index publishes.
    ///     index_is_interpolated (bool): Whether the observed index
    ///         interpolates between publications.
    ///     min_strike (float): The lower bound of the strike domain.
    ///     max_strike (float): The upper bound of the strike domain.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the reference date moves with.
    ///
    /// Returns:
    ///     ConstantYoYOptionletVolatility: The surface over that quote.
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

    /// Return the lag the surface itself observes inflation with.
    ///
    /// Returns:
    ///     Period: The observation lag, which is what to pass as obs_lag for
    ///         the surface's own.
    fn observation_lag(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.observation_lag())
    }

    /// Return how often the observed index publishes.
    ///
    /// Returns:
    ///     Frequency: The publication frequency.
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(self.inner.frequency())
    }

    /// Return whether the observed index interpolates between publications.
    ///
    /// Returns:
    ///     bool: True when the index interpolates.
    fn index_is_interpolated(&self) -> bool {
        self.inner.index_is_interpolated()
    }

    /// Return the date the surface measures its variance from.
    ///
    /// The reference date pulled back by the surface's own observation lag,
    /// snapped to the start of the publication period unless the index is
    /// interpolated.
    ///
    /// Returns:
    ///     Date: The base date.
    ///
    /// Raises:
    ///     ItofinError: On an unset evaluation date, and on a frequency
    ///         admitting no publication period.
    fn base_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.base_date().map_err(PyQlError::from)?,
        ))
    }

    /// Return the volatility for an exercise on date struck at strike.
    ///
    /// Args:
    ///     date (Date): The exercise date.
    ///     strike (float): The strike the volatility is read at.
    ///     obs_lag (Period): How far back inflation is observed; the lag is
    ///         explicit rather than defaulted, because C++ substitutes the
    ///         surface's own for a sentinel period and the port has no
    ///         sentinel to carry. Pass observation_lag() for that behaviour.
    ///
    /// Returns:
    ///     float: The optionlet volatility.
    ///
    /// Raises:
    ///     ItofinError: On an observed date before base_date(), and on a
    ///         strike outside the surface's strike domain.
    fn volatility(&self, date: &PyDate, strike: f64, obs_lag: &PyPeriod) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility(date.inner(), strike, obs_lag.inner())
            .map_err(PyQlError::from)?)
    }

    /// Return the total integrated variance for an exercise on date.
    ///
    /// The figure that scales time out of the optionlet formulae without
    /// committing to the distribution reading it.
    ///
    /// Args:
    ///     date (Date): The exercise date.
    ///     strike (float): The strike the variance is read at.
    ///     obs_lag (Period): How far back inflation is observed.
    ///
    /// Returns:
    ///     float: The total integrated variance.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions volatility() reports.
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

/// Prices a year-on-year inflation cap or floor optionlet by optionlet.
///
/// The distribution is chosen by the constructor rather than passed as an
/// argument, mirroring C++'s three engine classes: black is lognormal,
/// unit_displaced lognormal in 1 + rate and bachelier normal. The core
/// YoYOptionletDistribution enum is not bound, so distribution() reads back as
/// a string.
///
/// The settings behind the volatility surface and behind the cap/floor this
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
    /// Build an engine valuing optionlets under the lognormal model.
    ///
    /// Args:
    ///     index (YoYInflationIndex): The index forwards are read off.
    ///     volatility (ConstantYoYOptionletVolatility): The surface optionlet
    ///         volatilities are read off.
    ///     nominal_ts (YieldTermStructure): The nominal curve optionlets are
    ///         discounted on.
    ///
    /// Returns:
    ///     YoYInflationCapFloorEngine: The lognormal engine.
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

    /// Build an engine valuing optionlets under the unit-displaced lognormal model.
    ///
    /// Lognormal in 1 + rate, the usual quoting convention for a rate that may
    /// be negative.
    ///
    /// Args:
    ///     index (YoYInflationIndex): The index forwards are read off.
    ///     volatility (ConstantYoYOptionletVolatility): The surface optionlet
    ///         volatilities are read off.
    ///     nominal_ts (YieldTermStructure): The nominal curve optionlets are
    ///         discounted on.
    ///
    /// Returns:
    ///     YoYInflationCapFloorEngine: The unit-displaced lognormal engine.
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

    /// Build an engine valuing optionlets under the normal model.
    ///
    /// Args:
    ///     index (YoYInflationIndex): The index forwards are read off.
    ///     volatility (ConstantYoYOptionletVolatility): The surface optionlet
    ///         volatilities are read off.
    ///     nominal_ts (YieldTermStructure): The nominal curve optionlets are
    ///         discounted on.
    ///
    /// Returns:
    ///     YoYInflationCapFloorEngine: The normal engine.
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

    /// Return the distribution optionlets are valued under.
    ///
    /// Returns:
    ///     str: "black", "unit_displaced" or "bachelier".
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

/// The standard market builder for a year-on-year inflation cap or floor.
///
/// It derives an annual year-on-year leg from a length in years, trims that leg
/// to the optionlets asked for, and strikes it either at an explicit strike or
/// at the money off atm_strike. Exactly one of the two is required: the core
/// refuses both together and neither at all, at build time rather than at the
/// setters, so both surface from build().
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the whole configuration up front and
/// assembles the chain inside build(), as MakeVanillaSwap does. An unset
/// optional leaves the core default in place: a 1,000,000 nominal, a
/// ModifiedFollowing payment roll, a 30/360 bond-basis day counter, no fixing
/// days, every optionlet kept and no forward start.
///
/// Trimming happens before the at-the-money fill, so as_optionlet and
/// first_caplet_excluded change what an unset strike resolves to: the rate that
/// reprices whatever survives, not the whole leg's.
///
/// CapFloorType.Collar has no path here - the builder carries a single strike,
/// and a collar needs two strike vectors - so a collar is built through
/// YoYInflationCapFloor.collar over a leg of its own instead.
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
    /// Store the configuration the chain is assembled from in build().
    ///
    /// Args:
    ///     cap_floor_type (CapFloorType): Cap or Floor; Collar has no path
    ///         here.
    ///     index (YoYInflationIndex): The index the optionlets fix off.
    ///     length (int): The length of the derived annual leg, in years.
    ///     calendar (Calendar): The calendar the payments roll on.
    ///     observation_lag (Period): How far back each coupon's fixings are
    ///         observed.
    ///     interpolation (CpiInterpolationType): How the observed fixings are
    ///         interpolated.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///     nominal (float | None): The notional; None keeps the core default
    ///         of 1,000,000.
    ///     effective_date (Date | None): The start date; None derives it from
    ///         the evaluation date.
    ///     payment_day_counter (DayCounter | None): The day count; None keeps
    ///         the core default of 30/360 bond basis.
    ///     payment_adjustment (BusinessDayConvention | None): The payment
    ///         roll; None keeps the core default of ModifiedFollowing.
    ///     fixing_days (int | None): The fixing days of the coupons; None
    ///         keeps the core default of none.
    ///     engine (YoYInflationCapFloorEngine | None): An engine installed on
    ///         the built instrument; None leaves it unpriced.
    ///     as_optionlet (bool): Whether to keep only the last optionlet.
    ///     forward_start (Period | None): The delay before the leg starts;
    ///         None keeps the core default of no forward start.
    ///     first_caplet_excluded (bool): Whether to drop the front optionlet.
    ///     strike (float | None): The explicit strike; exactly one of this and
    ///         atm_strike is required.
    ///     atm_strike (YieldTermStructure | None): The curve the at-the-money
    ///         strike is filled off; exactly one of this and strike is
    ///         required.
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

    /// Build the cap/floor, which already carries its engine when one was given.
    ///
    /// Returns:
    ///     YoYInflationCapFloor: The built instrument.
    ///
    /// Raises:
    ///     ItofinError: If both strike and atm_strike are given or neither is;
    ///         if the start date has to be derived and no evaluation date is
    ///         set; and on whatever the leg construction and the at-the-money
    ///         fill report.
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

/// A cap, floor or collar over a year-on-year inflation leg.
///
/// Built either through MakeYoYInflationCapFloor, the standard market builder,
/// or through the raw constructors below, which take the coupon vector
/// YoYInflationLeg.coupons() hands back (#848). The raw route is the only one
/// that reaches a collar: the builder carries a single strike.
///
/// Unlike a nominal cap/floor this instrument keeps its first optionlet, so the
/// strip spans its leg exactly and cap - floor is the year-on-year swap.
///
/// Pricing needs an engine: call set_engine before npv.
#[pyclass(name = "YoYInflationCapFloor", unsendable)]
pub struct PyYoYInflationCapFloor {
    inner: SharedMut<YoYInflationCapFloor>,
}

#[pymethods]
impl PyYoYInflationCapFloor {
    /// Build an instrument of cap_floor_type over coupons, struck at both vectors.
    ///
    /// Each strike vector is padded to the leg length by repeating its last
    /// entry, so a single strike stands for every optionlet.
    ///
    /// Args:
    ///     cap_floor_type (CapFloorType): Cap, Floor or Collar.
    ///     coupons (list[YoYInflationCoupon]): The leg the optionlets sit on.
    ///     cap_rates (list[float]): The cap strikes.
    ///     floor_rates (list[float]): The floor strikes.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     YoYInflationCapFloor: The built instrument.
    ///
    /// Raises:
    ///     ItofinError: On an empty leg, and on a strike vector the type needs
    ///         and did not get: a cap or a collar needs cap rates, a floor or
    ///         a collar floor rates.
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

    /// Build a cap over coupons struck at strikes.
    ///
    /// Args:
    ///     coupons (list[YoYInflationCoupon]): The leg the optionlets sit on.
    ///     strikes (list[float]): The cap strikes, padded as new() pads.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     YoYInflationCapFloor: The cap over that leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions new() reports.
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

    /// Build a floor over coupons struck at strikes.
    ///
    /// Args:
    ///     coupons (list[YoYInflationCoupon]): The leg the optionlets sit on.
    ///     strikes (list[float]): The floor strikes, padded as new() pads.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     YoYInflationCapFloor: The floor over that leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions new() reports.
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

    /// Build a collar: long the cap at cap_rates, short the floor at floor_rates.
    ///
    /// Args:
    ///     coupons (list[YoYInflationCoupon]): The leg the optionlets sit on.
    ///     cap_rates (list[float]): The cap strikes, padded as new() pads.
    ///     floor_rates (list[float]): The floor strikes, padded the same way.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     YoYInflationCapFloor: The collar over that leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions new() reports.
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

    /// Build a cap or a floor from a single strike vector.
    ///
    /// Args:
    ///     cap_floor_type (CapFloorType): Cap or Floor.
    ///     coupons (list[YoYInflationCoupon]): The leg the optionlets sit on.
    ///     strikes (list[float]): Cap rates for a Cap and floor rates for a
    ///         Floor.
    ///     settings (Settings): The explicit settings the instrument resolves
    ///         its dates against.
    ///
    /// Returns:
    ///     YoYInflationCapFloor: The built instrument.
    ///
    /// Raises:
    ///     ItofinError: On an empty strikes, on a Collar - which needs two
    ///         vectors, so collar() is its constructor - and on the same
    ///         conditions new() reports.
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

    /// Return the cap strikes, one per coupon.
    ///
    /// Returns:
    ///     list[float]: The cap strikes; empty on a floor.
    fn cap_rates(&self) -> Vec<f64> {
        self.inner.borrow().cap_rates().to_vec()
    }

    /// Return the floor strikes, one per coupon.
    ///
    /// Returns:
    ///     list[float]: The floor strikes; empty on a cap.
    fn floor_rates(&self) -> Vec<f64> {
        self.inner.borrow().floor_rates().to_vec()
    }

    /// Return the number of optionlets.
    ///
    /// Returns:
    ///     int: One per year-on-year coupon on the leg.
    fn coupon_count(&self) -> usize {
        self.inner.borrow().yoy_leg().len()
    }

    /// Return the leg's earliest accrual start.
    ///
    /// Returns:
    ///     Date: The first accrual start date.
    ///
    /// Raises:
    ///     ItofinError: On an empty leg, which the constructors already
    ///         refuse.
    fn start_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.borrow().start_date().map_err(PyQlError::from)?,
        ))
    }

    /// Return the leg's latest accrual end.
    ///
    /// Returns:
    ///     Date: The last accrual end date.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions start_date reports.
    fn maturity_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .borrow()
                .maturity_date()
                .map_err(PyQlError::from)?,
        ))
    }

    /// Return the strike at which the leg reprices on discount_curve.
    ///
    /// The core takes the curve itself rather than a handle, so the link is
    /// resolved for the call.
    ///
    /// Args:
    ///     discount_curve (YieldTermStructure): The curve the leg reprices on.
    ///
    /// Returns:
    ///     float: The at-the-money rate.
    ///
    /// Raises:
    ///     ItofinError: On an unlinked discount_curve, a curve with no
    ///         reference date, and a leg with no basis-point sensitivity to
    ///         solve over.
    fn atm_rate(&self, discount_curve: &PyYieldTermStructure) -> PyResult<f64> {
        let handle = discount_curve.handle();
        let link = handle.current_link().map_err(PyQlError::from)?;
        Ok(self
            .inner
            .borrow()
            .atm_rate(link.as_ref())
            .map_err(PyQlError::from)?)
    }

    /// Attach an engine, replacing whatever the factory installed.
    ///
    /// Args:
    ///     engine (YoYInflationCapFloorEngine): The engine, which must resolve
    ///         its dates against the same Settings object this cap/floor was
    ///         built with: two different ones would price the leg and the
    ///         optionlets on different dates with no error raised.
    fn set_engine(&mut self, engine: &PyYoYInflationCapFloorEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the engine refuses the instrument.
    fn calculate(&mut self) -> PyResult<()> {
        Ok(self
            .inner
            .borrow_mut()
            .calculate()
            .map_err(PyQlError::from)?)
    }

    /// Return whether the cached results are currently valid.
    ///
    /// Returns:
    ///     bool: True when the next accessor reads the cache.
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }

    /// Attach engine and return the NPV.
    ///
    /// Replaces whatever engine the factory installed.
    ///
    /// Args:
    ///     engine (YoYInflationCapFloorEngine): The engine to install and
    ///         price on.
    ///
    /// Returns:
    ///     float: The cap/floor value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, engine: &PyYoYInflationCapFloorEngine) -> PyResult<f64> {
        self.set_engine(engine);
        self.calculate()?;
        self.npv()
    }

    /// Return a frozen snapshot of the valuation, calculating first.
    ///
    /// Returns:
    ///     Results: A copy of the valuation results.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn results(&mut self) -> PyResult<Results> {
        self.calculate()?;
        let inner = self.inner.borrow();
        Ok(Results::snapshot(inner.base()))
    }

    /// Return the cap/floor NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, which the core reports as
    ///         "null pricing engine", and on whatever the engine reports.
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

/// The quoted year-on-year cap/floor price grid, bicubic-interpolated over
/// strike and maturity with a cubic ATM swap rate curve through the cap/floor
/// intersections.
///
/// Construction only validates and stores the quotes; the calculations - the
/// cap/floor intersection and the year-on-year bootstrap over its ATM swap
/// rates - run on the first read and are cached. The reference date moves with
/// the evaluation date carried by settings, which must be set before
/// construction.
///
/// A price alone does not say cap or floor without the ATM level, and ATM
/// prices are generally inaccurate, coming from extrapolation and
/// intersection: the quoted grid is the data, the ATM curve a derived read.
#[pyclass(name = "YoYCapFloorTermPriceSurface", unsendable)]
pub struct PyYoYCapFloorTermPriceSurface {
    inner: Shared<InterpolatedYoYCapFloorTermPriceSurface>,
}

#[pymethods]
impl PyYoYCapFloorTermPriceSurface {
    /// Build the surface over quoted cap and floor prices.
    ///
    /// Args:
    ///     fixing_days (int): The fixing days of the quoted instruments.
    ///     yy_lag (Period): The observation lag of the quoted instruments.
    ///     yoy_index (YoYInflationIndex): The year-on-year index the surface
    ///         is quoted on.
    ///     interpolation (CpiInterpolationType): How an observation
    ///         interpolates between the index fixings bracketing it.
    ///     nominal_term_structure (YieldTermStructure): The nominal discount
    ///         curve the derived year-on-year bootstrap prices against.
    ///     day_counter (DayCounter): The day count times are measured in.
    ///     calendar (Calendar): The calendar maturities resolve on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a date.
    ///     c_strikes (list[float]): The quoted cap strikes, one per cap grid
    ///         row; strictly increasing.
    ///     f_strikes (list[float]): The quoted floor strikes, one per floor
    ///         grid row; strictly increasing.
    ///     cf_maturities (list[Period]): The quoted maturities, one per grid
    ///         column; shared by both grids.
    ///     c_price (list[list[float]]): The cap prices, one row per cap
    ///         strike and one column per maturity.
    ///     f_price (list[list[float]]): The floor prices, one row per floor
    ///         strike and one column per maturity.
    ///     settings (Settings): The explicit settings supplying the
    ///         evaluation date the reference date moves with.
    ///
    /// Raises:
    ///     ItofinError: On an empty or ragged price grid, on grid dimensions
    ///         that do not match the strikes and maturities, on a
    ///         non-increasing axis, and on a cap/floor strike union that
    ///         overlaps the wrong way round.
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

    /// Return the interpolated cap price at date struck at strike.
    ///
    /// A pure surface lookup with the spline's extrapolation enabled, as in
    /// C++.
    ///
    /// Args:
    ///     date (Date): The maturity the price is read at.
    ///     strike (float): The strike the price is read at.
    ///
    /// Returns:
    ///     float: The interpolated cap price.
    ///
    /// Raises:
    ///     ItofinError: On an unset evaluation date, and on whatever stops
    ///         the first-read calculations: a failed intersection solve, an
    ///         intersection outside its arbitrage bounds past the
    ///         extrapolation horizon, or a bootstrap that cannot reprice its
    ///         helpers.
    fn cap_price(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .cap_price(date.inner(), strike)
            .map_err(PyQlError::from)?)
    }

    /// Return the interpolated floor price at date struck at strike.
    ///
    /// Args:
    ///     date (Date): The maturity the price is read at.
    ///     strike (float): The strike the price is read at.
    ///
    /// Returns:
    ///     float: The interpolated floor price.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions cap_price() reports.
    fn floor_price(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .floor_price(date.inner(), strike)
            .map_err(PyQlError::from)?)
    }

    /// Return the ATM year-on-year swap rate at date.
    ///
    /// Read off the cubic curve through the cap/floor intersections.
    ///
    /// Args:
    ///     date (Date): The maturity the rate is read at.
    ///     extrapolate (bool): Whether to answer outside the quoted
    ///         maturities; defaults True as in C++.
    ///
    /// Returns:
    ///     float: The ATM year-on-year swap rate.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions cap_price() reports, and on a
    ///         date outside the quoted maturities when extrapolate is False.
    #[pyo3(signature = (date, extrapolate = true))]
    fn atm_yoy_swap_rate(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .atm_yoy_swap_rate(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the cap/floor strike union.
    ///
    /// Every floor strike, then the cap strikes above them.
    ///
    /// Returns:
    ///     list[float]: The strike union, strictly increasing.
    fn strikes(&self) -> Vec<f64> {
        self.inner.strikes().to_vec()
    }

    /// Return the quoted maturities, one per grid column.
    ///
    /// Returns:
    ///     list[Period]: The maturities.
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
    pub(crate) fn shared(&self) -> Shared<dyn YoYCapFloorTermPriceSurface> {
        Shared::clone(&self.inner) as Shared<dyn YoYCapFloorTermPriceSurface>
    }
}

/// The year-on-year optionlet volatility surface stripped out of a quoted
/// YoYCapFloorTermPriceSurface, interpolating linearly across the quoted
/// strikes of each date's slice.
///
/// The stripping pipeline is built inside the constructor rather than passed
/// in: the stripper reprices each optionlet through an engine whose
/// volatility link it relinks every solver iteration, so engine and stripper
/// must share one relinkable handle that starts empty. The constructor
/// therefore takes the index and nominal curve the engine needs and wires the
/// handle itself; a caller-supplied engine would silently strip against
/// nothing.
///
/// Construction runs the stripping, so it is fallible and the evaluation date
/// carried by settings must be set first. The pricer is pinned to the
/// unit-displaced lognormal model.
#[pyclass(name = "KInterpolatedYoYOptionletVolatilitySurface", unsendable)]
pub struct PyKInterpolatedYoYOptionletVolatilitySurface {
    inner: Shared<KInterpolatedYoYOptionletVolatilitySurface<Linear>>,
    observation_lag: Period,
}

#[pymethods]
impl PyKInterpolatedYoYOptionletVolatilitySurface {
    /// Strip cap_floor_prices into an optionlet volatility surface.
    ///
    /// Args:
    ///     settlement_days (int): Days from the evaluation date to the
    ///         surface's reference date.
    ///     calendar (Calendar): The calendar the reference date resolves on.
    ///     business_day_convention (BusinessDayConvention): The roll applied
    ///         when resolving a date.
    ///     day_counter (DayCounter): The day count times are measured in.
    ///     observation_lag (Period): The lag the surface observes inflation
    ///         with, normally the price surface's own.
    ///     cap_floor_prices (YoYCapFloorTermPriceSurface): The quoted
    ///         cap/floor price grid to strip.
    ///     index (YoYInflationIndex): The year-on-year index the internal
    ///         engine forecasts off; it must be linked to a year-on-year
    ///         curve, which is the index's own rather than the price
    ///         surface's bootstrapped one.
    ///     nominal_term_structure (YieldTermStructure): The nominal discount
    ///         curve the internal engine discounts on.
    ///     slope (float): The assumed proportional change per year of the
    ///         unobserved initial caplet volatility, which the stripper
    ///         extends each strike's first observable volatility back with.
    ///     settings (Settings): The explicit settings supplying the
    ///         evaluation date; it must match the one behind index and
    ///         cap_floor_prices.
    ///
    /// Raises:
    ///     ItofinError: On whatever stops the stripping: an unset evaluation
    ///         date, an unlinked index, or a solve that cannot bracket an
    ///         optionlet volatility.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        settlement_days: u32,
        calendar: &PyCalendar,
        business_day_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        observation_lag: &PyPeriod,
        cap_floor_prices: &PyYoYCapFloorTermPriceSurface,
        index: &PyYoYInflationIndex,
        nominal_term_structure: &PyYieldTermStructure,
        slope: f64,
        settings: &PySettings,
    ) -> PyResult<Self> {
        let vol_handle = RelinkableHandle::<dyn YoYOptionletVolatilitySurface>::empty();
        let pricer = shared_mut(YoYInflationCapFloorEngine::unit_displaced(
            index.shared(),
            vol_handle.handle(),
            nominal_term_structure.handle(),
        ));
        let stripper = shared(InterpolatedYoYOptionletStripper::<Linear>::new())
            as Shared<dyn YoYOptionletStripper>;
        let inner = KInterpolatedYoYOptionletVolatilitySurface::<Linear>::new(
            settlement_days,
            calendar.inner(),
            business_day_convention.inner(),
            day_counter.inner(),
            observation_lag.inner(),
            cap_floor_prices.shared(),
            &pricer,
            &vol_handle,
            stripper,
            slope,
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(PyKInterpolatedYoYOptionletVolatilitySurface {
            inner: shared(inner),
            observation_lag: observation_lag.inner(),
        })
    }

    /// Return the stripped (strikes, volatilities) profile at date.
    ///
    /// C++'s Dslice: one volatility per strike of the price surface's
    /// cap/floor union.
    ///
    /// Args:
    ///     date (Date): The date the slice is stripped at.
    ///
    /// Returns:
    ///     tuple[list[float], list[float]]: The quoted strike union and the
    ///         volatility stripped at each strike.
    ///
    /// Raises:
    ///     ItofinError: On a date the stripper cannot price a slice at.
    fn d_slice(&self, date: &PyDate) -> PyResult<(Vec<f64>, Vec<f64>)> {
        Ok(self.inner.d_slice(date.inner()).map_err(PyQlError::from)?)
    }

    /// Return the date the surface measures its variance from.
    ///
    /// The reference date pulled back by the surface's own observation lag,
    /// snapped to the start of the publication period.
    ///
    /// Returns:
    ///     Date: The base date.
    ///
    /// Raises:
    ///     ItofinError: On an unset evaluation date, and on a frequency
    ///         admitting no publication period.
    fn base_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner.base_date().map_err(PyQlError::from)?,
        ))
    }

    /// Return the volatility for an exercise on date struck at strike.
    ///
    /// Observes inflation the surface's own observation lag back, C++'s
    /// default-lag behaviour: the slice at the observed date interpolated
    /// across its strikes.
    ///
    /// Args:
    ///     date (Date): The exercise date.
    ///     strike (float): The strike the volatility is read at.
    ///
    /// Returns:
    ///     float: The optionlet volatility.
    ///
    /// Raises:
    ///     ItofinError: On an observed date before base_date(), and on a
    ///         strike outside the surface's strike domain.
    fn volatility(&self, date: &PyDate, strike: f64) -> PyResult<f64> {
        Ok(self
            .inner
            .volatility(date.inner(), strike, self.observation_lag)
            .map_err(PyQlError::from)?)
    }

    /// Return the lowest quoted strike of the cap/floor union.
    ///
    /// Returns:
    ///     float: The lowest strike the surface answers for.
    fn min_strike(&self) -> f64 {
        self.inner.min_strike()
    }

    /// Return the highest quoted strike of the cap/floor union.
    ///
    /// Returns:
    ///     float: The highest strike the surface answers for.
    fn max_strike(&self) -> f64 {
        self.inner.max_strike()
    }

    /// Return the last date the surface answers for.
    ///
    /// The reference date advanced by the price surface's last quoted
    /// maturity.
    ///
    /// Returns:
    ///     Date: The last queryable date.
    fn max_date(&self) -> PyDate {
        PyDate::from_inner(self.inner.max_date())
    }
}
