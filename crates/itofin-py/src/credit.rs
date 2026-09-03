//! Facades for the credit hierarchy: the DefaultProbabilityTermStructure base,
//! the concrete FlatHazardRate, InterpolatedHazardRateCurve and
//! PiecewiseDefaultCurve curves, the ProtectionSide and PricingModel flags, the
//! CreditDefaultSwap instrument and its market-convention builder
//! MakeCreditDefaultSwap.

use crate::PyQlError;
use crate::creditengine::{PyIsdaCdsEngine, PyMidPointCdsEngine};
use crate::credithelpers::PyDefaultProbabilityHelper;
use crate::curve::PyYieldTermStructure;
use crate::market::PySimpleQuote;
use crate::results::Results;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PySchedule};
use libitofin::cashflow::CashFlow;
use libitofin::event::Event;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    CdsTerms, CreditDefaultSwap, MakeCreditDefaultSwap, PricingModel, ProtectionSide,
};
use libitofin::math::interpolations::flat::BackwardFlat;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::credit::defaultprobabilityhelpers::DefaultProbabilityHelper;
use libitofin::termstructures::credit::defaulttermstructure::DefaultProbabilityTermStructure;
use libitofin::termstructures::credit::flathazardrate::FlatHazardRate;
use libitofin::termstructures::credit::interpolatedhazardratecurve::InterpolatedHazardRateCurve;
use libitofin::termstructures::credit::piecewisedefaultcurve::PiecewiseDefaultCurve;
use libitofin::termstructures::credit::probabilitytraits::HazardRate;
use libitofin::time::date::Date;
use libitofin::types::Natural;
use pyo3::prelude::*;

/// Which leg of a default-protection contract a party holds: the buyer pays
/// the premium leg and receives the default payment, the seller the reverse.
#[pyclass(name = "ProtectionSide", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyProtectionSide {
    Buyer,
    Seller,
}

impl PyProtectionSide {
    /// The core ProtectionSide this variant stands for.
    pub(crate) fn inner(self) -> ProtectionSide {
        match self {
            PyProtectionSide::Buyer => ProtectionSide::Buyer,
            PyProtectionSide::Seller => ProtectionSide::Seller,
        }
    }
}

/// The model a quoted contract is inverted under by
/// CreditDefaultSwap.implied_hazard_rate: Midpoint is not ISDA conform, Isda
/// carries the three fidelity flags the core fixes at that call site.
#[pyclass(name = "PricingModel", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyPricingModel {
    Midpoint,
    Isda,
}

impl PyPricingModel {
    /// The core PricingModel this variant stands for.
    pub(crate) fn inner(self) -> PricingModel {
        match self {
            PyPricingModel::Midpoint => PricingModel::Midpoint,
            PyPricingModel::Isda => PricingModel::Isda,
        }
    }
}

/// Shared base for every credit curve: survival and default probabilities,
/// the default density and the hazard rate, each in a year-fraction and a date
/// form.
#[pyclass(name = "DefaultProbabilityTermStructure", subclass, unsendable)]
pub struct PyDefaultProbabilityTermStructure {
    inner: Handle<dyn DefaultProbabilityTermStructure>,
}

#[pymethods]
impl PyDefaultProbabilityTermStructure {
    /// Return the survival probability from the reference date to year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, in the curve's own day count.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The probability of surviving to t.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (t, extrapolate = false))]
    fn survival_probability(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .survival_probability(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the survival probability from the reference date to date.
    ///
    /// Args:
    ///     date (Date): The date survived to.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The survival probability.
    ///
    /// Raises:
    ///     ItofinError: If date is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn survival_probability_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .survival_probability_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the default probability from the reference date to year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, in the curve's own day count.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The probability of defaulting by t.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (t, extrapolate = false))]
    fn default_probability(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_probability(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the default probability from the reference date to date.
    ///
    /// Args:
    ///     date (Date): The date defaulted by.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The default probability.
    ///
    /// Raises:
    ///     ItofinError: If date is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn default_probability_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_probability_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the default density at year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, in the curve's own day count.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The default density.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (t, extrapolate = false))]
    fn default_density(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_density(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the default density at date.
    ///
    /// Args:
    ///     date (Date): The date the density is read at.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The default density.
    ///
    /// Raises:
    ///     ItofinError: If date is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn default_density_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_density_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the hazard rate at year-fraction t.
    ///
    /// Args:
    ///     t (float): The year fraction, in the curve's own day count.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The hazard rate, at annual frequency and continuous
    ///         compounding.
    ///
    /// Raises:
    ///     ItofinError: If t is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (t, extrapolate = false))]
    fn hazard_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .hazard_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// Return the hazard rate at date.
    ///
    /// Args:
    ///     date (Date): The date the rate is read at.
    ///     extrapolate (bool): Whether to answer past the curve's max date.
    ///
    /// Returns:
    ///     float: The hazard rate, at annual frequency and continuous
    ///         compounding.
    ///
    /// Raises:
    ///     ItofinError: If date is past the curve's range and extrapolation is
    ///         not allowed.
    #[pyo3(signature = (date, extrapolate = false))]
    fn hazard_rate_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .hazard_rate_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }
}

impl PyDefaultProbabilityTermStructure {
    /// The base half of a concrete curve's initializer chain.
    pub(crate) fn from_handle(inner: Handle<dyn DefaultProbabilityTermStructure>) -> Self {
        PyDefaultProbabilityTermStructure { inner }
    }

    /// A clone of the inner curve handle for the CDS instrument and engine
    /// facades that take a credit curve.
    pub(crate) fn handle(&self) -> Handle<dyn DefaultProbabilityTermStructure> {
        self.inner.clone()
    }
}

/// A credit curve quoting one hazard rate for every maturity, whose survival
/// probability is the closed form exp(-h t).
///
/// The quote-backed forms retain the caller's SimpleQuote, so a later set_value
/// moves the curve; the rate-backed forms wrap the value in a fresh, un-retained
/// quote. The moving forms fix the reference date settlement_days business days
/// past the evaluation date carried by settings.
#[pyclass(name = "FlatHazardRate", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyFlatHazardRate;

#[pymethods]
impl PyFlatHazardRate {
    /// Build a curve reading its hazard rate live, on a pinned reference date.
    ///
    /// Args:
    ///     reference_date (Date): The date times are measured from.
    ///     hazard_rate (SimpleQuote): The hazard rate; the caller keeps it, so
    ///         a later set_value moves the curve.
    ///     day_counter (DayCounter): The day count turning dates into times.
    #[new]
    fn new(
        reference_date: &PyDate,
        hazard_rate: &PySimpleQuote,
        day_counter: &PyDayCounter,
    ) -> PyClassInitializer<Self> {
        let curve = shared(FlatHazardRate::new(
            reference_date.inner(),
            hazard_rate.handle(),
            day_counter.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
            curve,
        )))
        .add_subclass(PyFlatHazardRate)
    }

    /// Build a curve at a fixed rate, on a pinned reference date.
    ///
    /// Args:
    ///     reference_date (Date): The date times are measured from.
    ///     rate (float): The hazard rate, wrapped in a fresh, un-retained
    ///         quote.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Returns:
    ///     FlatHazardRate: The curve at that rate.
    #[staticmethod]
    fn with_rate(
        py: Python<'_>,
        reference_date: &PyDate,
        rate: f64,
        day_counter: &PyDayCounter,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatHazardRate::with_rate(
            reference_date.inner(),
            rate,
            day_counter.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(PyFlatHazardRate),
        )
    }

    /// Build a curve reading its hazard rate live, on a floating reference date.
    ///
    /// The reference date sits settlement_days business days past the
    /// evaluation date, so a query made before settings carries one raises
    /// rather than falling back to a system clock.
    ///
    /// Args:
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar those days are counted on.
    ///     hazard_rate (SimpleQuote): The hazard rate; the caller keeps it, so
    ///         a later set_value moves the curve.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date.
    ///
    /// Returns:
    ///     FlatHazardRate: The moving curve.
    #[staticmethod]
    fn moving(
        py: Python<'_>,
        settlement_days: Natural,
        calendar: &PyCalendar,
        hazard_rate: &PySimpleQuote,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatHazardRate::moving(
            settlement_days,
            calendar.inner(),
            hazard_rate.handle(),
            day_counter.inner(),
            settings.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(PyFlatHazardRate),
        )
    }

    /// Build a curve at a fixed rate, on a floating reference date.
    ///
    /// As moving(), a query made before settings carries an evaluation date
    /// raises rather than falling back to a system clock.
    ///
    /// Args:
    ///     settlement_days (int): The business days the reference date sits
    ///         past the evaluation date.
    ///     calendar (Calendar): The calendar those days are counted on.
    ///     rate (float): The hazard rate, wrapped in a fresh, un-retained
    ///         quote.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date.
    ///
    /// Returns:
    ///     FlatHazardRate: The moving curve at that rate.
    #[staticmethod]
    fn moving_with_rate(
        py: Python<'_>,
        settlement_days: Natural,
        calendar: &PyCalendar,
        rate: f64,
        day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let curve = shared(FlatHazardRate::moving_with_rate(
            settlement_days,
            calendar.inner(),
            rate,
            day_counter.inner(),
            settings.inner(),
        )) as Shared<dyn DefaultProbabilityTermStructure>;
        Py::new(
            py,
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                curve,
            )))
            .add_subclass(PyFlatHazardRate),
        )
    }
}

/// A credit curve built from (date, hazard-rate) nodes, interpolating
/// backward-flat.
///
/// The first date is the reference date. Backward-flat reads the right-hand
/// node on every segment, so the hazard rate is a right-continuous step
/// function and the survival probability is exp(-integral) over those steps.
/// Finite in time: queries past the last node need extrapolate=True, which
/// continues at the last node's rate.
///
/// No interpolation argument is offered, and the curve carries no calendar: the
/// calendar-taking constructor is not exposed.
#[pyclass(name = "InterpolatedHazardRateCurve", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyInterpolatedHazardRateCurve {
    concrete: Shared<InterpolatedHazardRateCurve<BackwardFlat>>,
}

#[pymethods]
impl PyInterpolatedHazardRateCurve {
    /// Build the curve over its (date, hazard-rate) nodes.
    ///
    /// Backward-flat is pinned at the boundary: it is the only interpolator
    /// the credit side wires, so no interpolation argument is offered.
    ///
    /// Args:
    ///     dates (list[Date]): The node dates, the first being the reference
    ///         date.
    ///     hazard_rates (list[float]): The hazard rate at each node.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On too few dates, a dates and hazard_rates count
    ///         mismatch, a negative hazard rate, or unsorted dates.
    #[new]
    fn new(
        dates: Vec<PyRef<PyDate>>,
        hazard_rates: Vec<f64>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let dates: Vec<_> = dates.iter().map(|date| date.inner()).collect();
        let concrete = shared(
            InterpolatedHazardRateCurve::new(
                dates,
                hazard_rates,
                day_counter.inner(),
                BackwardFlat,
            )
            .map_err(PyQlError::from)?,
        );
        let erased = Shared::clone(&concrete) as Shared<dyn DefaultProbabilityTermStructure>;
        Ok(
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                erased,
            )))
            .add_subclass(PyInterpolatedHazardRateCurve { concrete }),
        )
    }

    /// Return the node dates.
    ///
    /// Returns:
    ///     list[Date]: The nodes, the first of which is the reference date.
    fn dates(&self) -> Vec<PyDate> {
        self.concrete
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }

    /// Return the node hazard rates.
    ///
    /// Returns:
    ///     list[float]: The rate at each node.
    fn hazard_rates(&self) -> Vec<f64> {
        self.concrete.hazard_rates().to_vec()
    }

    /// Return the curve's nodes as pairs.
    ///
    /// Returns:
    ///     list[tuple[Date, float]]: One (date, hazard rate) pair per node.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.concrete
            .nodes()
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect()
    }
}

/// A credit curve bootstrapped from CDS helpers, solving one hazard-rate
/// node per helper maturity (PiecewiseDefaultCurve<HazardRate, BackwardFlat>).
///
/// Lazy: the bootstrap runs on the first read, so the helpers' Settings flags
/// and evaluation date must be in place before that read, not merely before
/// the constructor. A helper quote moving invalidates the cache.
#[pyclass(name = "PiecewiseDefaultCurve", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyPiecewiseDefaultCurve {
    concrete: Shared<PiecewiseDefaultCurve<HazardRate, BackwardFlat>>,
}

#[pymethods]
impl PyPiecewiseDefaultCurve {
    /// Build the curve over helpers with a fixed reference date.
    ///
    /// Args:
    ///     reference_date (Date): The curve's reference date.
    ///     helpers (list[DefaultProbabilityHelper]): The bootstrap
    ///         instruments; any subclass is accepted.
    ///     day_counter (DayCounter): The day count turning dates into times.
    ///
    /// Raises:
    ///     ItofinError: On an empty helper list.
    #[new]
    fn new(
        reference_date: &PyDate,
        helpers: Vec<PyRef<PyDefaultProbabilityHelper>>,
        day_counter: &PyDayCounter,
    ) -> PyResult<PyClassInitializer<Self>> {
        let instruments: Vec<Shared<dyn DefaultProbabilityHelper>> =
            helpers.iter().map(|helper| helper.shared()).collect();
        let concrete = PiecewiseDefaultCurve::<HazardRate, BackwardFlat>::new(
            reference_date.inner(),
            instruments,
            day_counter.inner(),
            BackwardFlat,
        )
        .map_err(PyQlError::from)?;
        let erased = Shared::clone(&concrete) as Shared<dyn DefaultProbabilityTermStructure>;
        Ok(
            PyClassInitializer::from(PyDefaultProbabilityTermStructure::from_handle(Handle::new(
                erased,
            )))
            .add_subclass(PyPiecewiseDefaultCurve { concrete }),
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
    ///     list[float]: The nodes in the curve's own day count.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.times().map_err(PyQlError::from)?)
    }

    /// Return the node dates, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[Date]: The nodes, the first of which is the reference date.
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

    /// Return the solved node hazard rates, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[float]: The rate solved at each node.
    ///
    /// Raises:
    ///     ItofinError: On a bootstrap failure.
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }

    /// Return the solved nodes as pairs, triggering the bootstrap.
    ///
    /// Returns:
    ///     list[tuple[Date, float]]: One (date, hazard rate) pair per node.
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

/// A credit-default swap quoted as a running spread.
///
/// __init__ takes the C++ default terms with settles_accrual and
/// pays_at_default_time quoted; with_terms additionally exposes
/// protection_start and rebates_accrual.
///
/// These direct constructors keep the remaining CdsTerms fields at their core
/// defaults: claim (a face-value claim, which needs a claim facade that does
/// not exist yet), last_period_day_counter, upfront_date and
/// cash_settlement_days. trade_date and an upfront are not set here but are
/// reachable through MakeCreditDefaultSwap (with_trade_date and the upfront_rate
/// constructor argument).
///
/// Pricing needs an engine: call set_engine() or set_isda_engine() before
/// npv().
#[pyclass(name = "CreditDefaultSwap", unsendable)]
pub struct PyCreditDefaultSwap {
    inner: SharedMut<CreditDefaultSwap>,
}

#[pymethods]
impl PyCreditDefaultSwap {
    /// Build a contract on the C++ default terms.
    ///
    /// Args:
    ///     side (ProtectionSide): Whether protection is bought or sold.
    ///     notional (float): The notional the premium and protection are
    ///         quoted on.
    ///     spread (float): The running spread the premium leg pays.
    ///     schedule (Schedule): The premium leg's payment schedule.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         premium payment dates.
    ///     day_counter (DayCounter): The day count the premium accrues on.
    ///     settles_accrual (bool): Whether the accrued coupon settles on
    ///         default.
    ///     pays_at_default_time (bool): Whether the protection pays at default
    ///         rather than at maturity.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the contract prices against.
    ///
    /// Raises:
    ///     ItofinError: If the schedule is empty, if the protection start
    ///         follows the first accrual date under a pre-Big-Bang
    ///         date-generation rule, or if the premium leg cannot be built.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        side: PyProtectionSide,
        notional: f64,
        spread: f64,
        schedule: &PySchedule,
        payment_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        settles_accrual: bool,
        pays_at_default_time: bool,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(PyCreditDefaultSwap {
            inner: shared_mut(
                CreditDefaultSwap::new(
                    side.inner(),
                    notional,
                    spread,
                    schedule.inner(),
                    payment_convention.inner(),
                    day_counter.inner(),
                    settles_accrual,
                    pays_at_default_time,
                    settings.inner(),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Build a contract quoting the terms __init__ defaults.
    ///
    /// The three flags carry the core defaults verbatim, so calling this with
    /// only the positional arguments builds exactly what __init__ builds.
    ///
    /// Args:
    ///     side (ProtectionSide): Whether protection is bought or sold.
    ///     notional (float): The notional the premium and protection are
    ///         quoted on.
    ///     spread (float): The running spread the premium leg pays.
    ///     schedule (Schedule): The premium leg's payment schedule.
    ///     payment_convention (BusinessDayConvention): The roll applied to the
    ///         premium payment dates.
    ///     day_counter (DayCounter): The day count the premium accrues on.
    ///     settings (Settings): The explicit settings; it precedes the
    ///         defaulted terms because a Python signature cannot put a
    ///         required argument after an optional one.
    ///     protection_start (Date | None): The first date a default triggers
    ///         the contract; None takes the schedule's first date, which is
    ///         what __init__ does.
    ///     settles_accrual (bool): Whether the accrued coupon settles on
    ///         default.
    ///     pays_at_default_time (bool): Whether the protection pays at default
    ///         rather than at maturity.
    ///     rebates_accrual (bool): Whether the protection seller rebates the
    ///         accrued current coupon.
    ///
    /// Returns:
    ///     CreditDefaultSwap: The contract on those terms.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions __init__ reports.
    #[staticmethod]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        side,
        notional,
        spread,
        schedule,
        payment_convention,
        day_counter,
        settings,
        protection_start = None,
        settles_accrual = true,
        pays_at_default_time = true,
        rebates_accrual = true,
    ))]
    fn with_terms(
        side: PyProtectionSide,
        notional: f64,
        spread: f64,
        schedule: &PySchedule,
        payment_convention: &PyBusinessDayConvention,
        day_counter: &PyDayCounter,
        settings: &PySettings,
        protection_start: Option<&PyDate>,
        settles_accrual: bool,
        pays_at_default_time: bool,
        rebates_accrual: bool,
    ) -> PyResult<Self> {
        let terms = CdsTerms {
            settles_accrual,
            pays_at_default_time,
            protection_start: protection_start.map(PyDate::inner),
            rebates_accrual,
            ..CdsTerms::default()
        };
        Ok(PyCreditDefaultSwap {
            inner: shared_mut(
                CreditDefaultSwap::with_terms(
                    side.inner(),
                    notional,
                    spread,
                    schedule.inner(),
                    payment_convention.inner(),
                    day_counter.inner(),
                    terms,
                    settings.inner(),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }

    /// Attach a mid-point engine so the contract prices.
    ///
    /// The engine is built separately, so one engine can be shared across
    /// contracts. It must resolve its dates against the same Settings object
    /// as this contract.
    ///
    /// Args:
    ///     engine (MidPointCdsEngine): The engine and its default-probability
    ///         and discount curves.
    fn set_engine(&mut self, engine: &PyMidPointCdsEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Attach an ISDA engine so the contract prices under the standard model.
    ///
    /// A separate setter rather than a widened set_engine: the two engine
    /// facades are unrelated classes, so one argument cannot name both. The
    /// same sharing and same-Settings rules apply, and the ISDA engine
    /// additionally refuses curves outside its specification when the contract
    /// prices.
    ///
    /// Args:
    ///     engine (IsdaCdsEngine): The ISDA engine and its curves.
    fn set_isda_engine(&mut self, engine: &PyIsdaCdsEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Force the valuation. Idempotent.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, no evaluation date is set,
    ///         or the engine refuses the contract.
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

    /// Attach the mid-point engine and return the NPV.
    ///
    /// set_engine followed by npv, in one call. The mid-point engine is the
    /// primary because it is the core's own default CDS engine;
    /// set_isda_engine stays a separate setter.
    ///
    /// Args:
    ///     engine (MidPointCdsEngine): The engine to install and price on.
    ///
    /// Returns:
    ///     float: The contract value.
    ///
    /// Raises:
    ///     ItofinError: On anything that makes the valuation fail.
    fn price(&mut self, engine: &PyMidPointCdsEngine) -> PyResult<f64> {
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

    /// Return the contract NPV under the attached engine.
    ///
    /// Returns:
    ///     float: The present value.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, which the core reports as
    ///         "null pricing engine".
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// Return the running spread that prices the contract at zero.
    ///
    /// Returns:
    ///     float: The fair running spread.
    ///
    /// Raises:
    ///     ItofinError: If no engine is attached, and when the engine priced a
    ///         worthless premium leg and so provided no fair spread.
    fn fair_spread(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_spread()
            .map_err(PyQlError::from)?)
    }

    /// Return the upfront that prices the contract at zero.
    ///
    /// Returns:
    ///     float: The fair upfront, as a fraction of the notional.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions fair_spread reports.
    fn fair_upfront(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_upfront()
            .map_err(PyQlError::from)?)
    }

    /// Return the notional the premium and the protection are quoted on.
    ///
    /// Returns:
    ///     float: The contract notional.
    fn notional(&self) -> f64 {
        self.inner.borrow().notional()
    }

    /// Return the accrued coupon the protection seller rebates.
    ///
    /// A contract traded in the past still carries the flow: the core builds
    /// it whenever the flag is set, regardless of the trade date, so None here
    /// means the flag was off, never a stale trade. Such a flow carries a real
    /// accrued amount but settled on a past date, so it no longer reaches the
    /// value. The amount is returned bare rather than behind a cash-flow
    /// facade, there being none.
    ///
    /// Returns:
    ///     float | None: The rebated amount, or None when the contract does
    ///         not rebate accrual at all.
    fn accrual_rebate_amount(&self) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .borrow()
            .accrual_rebate()
            .map(|rebate| rebate.amount())
            .transpose()
            .map_err(PyQlError::from)?)
    }

    /// Return the date the accrual rebate settles on.
    ///
    /// Returns:
    ///     Date | None: The cash-settlement date the upfront also pays on, or
    ///         None on the same terms as accrual_rebate_amount.
    fn accrual_rebate_date(&self) -> Option<PyDate> {
        self.inner
            .borrow()
            .accrual_rebate()
            .map(|rebate| PyDate::from_inner(Event::date(rebate.as_ref())))
    }

    /// Return the premium leg's NPV.
    ///
    /// Returns:
    ///     float: The present value of the premium leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions fair_spread reports.
    fn coupon_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .coupon_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// Return the protection leg's NPV.
    ///
    /// Returns:
    ///     float: The present value of the protection leg.
    ///
    /// Raises:
    ///     ItofinError: On the same conditions fair_spread reports.
    fn default_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .default_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// Return the flat hazard rate at which this contract is worth target_npv.
    ///
    /// The solve stands on its own engine rather than on whichever one
    /// set_engine attached: it builds a flat, quote-backed probability curve
    /// and prices on model against discount. There is therefore no
    /// probability-curve argument - the curve being solved for is the one the
    /// core builds.
    ///
    /// Args:
    ///     target_npv (float): The value the contract is solved to.
    ///     discount (YieldTermStructure): The curve the flows discount on.
    ///     day_counter (DayCounter): The day count of the internal flat curve,
    ///         not of the contract. Under PricingModel.Isda both it and
    ///         discount must count Act/365 (Fixed), which is what the ISDA
    ///         engine requires of its curves.
    ///     recovery_rate (float): The recovery assumed on default.
    ///     accuracy (float): The tolerance the solve stops at, on the rate.
    ///     model (PricingModel): The model the contract is inverted under.
    ///
    /// Returns:
    ///     float: The flat hazard rate.
    ///
    /// Raises:
    ///     ItofinError: On a malformed contract, and when the solve does not
    ///         converge, which includes a pricing failure at some hazard rate.
    fn implied_hazard_rate(
        &self,
        target_npv: f64,
        discount: &PyYieldTermStructure,
        day_counter: &PyDayCounter,
        recovery_rate: f64,
        accuracy: f64,
        model: PyPricingModel,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow()
            .implied_hazard_rate(
                target_npv,
                &discount.handle(),
                day_counter.inner(),
                recovery_rate,
                accuracy,
                model.inner(),
            )
            .map_err(PyQlError::from)?)
    }
}

impl PyCreditDefaultSwap {
    /// Wraps a contract another facade built, the shape
    /// MakeCreditDefaultSwap.build() hands back.
    pub(crate) fn from_inner(inner: SharedMut<CreditDefaultSwap>) -> Self {
        PyCreditDefaultSwap { inner }
    }
}

/// Market-convention builder for a CreditDefaultSwap: derives the premium
/// schedule from a maturity and the post-Big-Bang CDS conventions, and takes
/// the trade date from the evaluation date settings carries.
///
/// An unset optional keeps the core default: a Buyer side, a nominal of 1, no
/// upfront, a 3M coupon tenor, the pre-CDS2015 DateGeneration.CDS rule, a
/// Following roll, an Act/360 day counter and three cash-settlement days. Only
/// the term-date quotation is exposed; the tenor and explicit-schedule ones and
/// the accrual-rebate flag are not, the latter being reachable through
/// CreditDefaultSwap.with_terms.
///
/// Each build() runs a fresh chain, so one builder object cannot carry a
/// setting into a later contract.
#[pyclass(name = "MakeCreditDefaultSwap", unsendable)]
pub struct PyMakeCreditDefaultSwap {
    term_date: Date,
    running_spread: f64,
    settings: Shared<Settings<Date>>,
    nominal: Option<f64>,
    upfront_rate: Option<f64>,
    side: Option<PyProtectionSide>,
    trade_date: Option<Date>,
}

#[pymethods]
impl PyMakeCreditDefaultSwap {
    /// Store the configuration the chain is assembled from in build().
    ///
    /// Each build() runs a fresh chain, so one builder object cannot carry a
    /// setting into a later contract.
    ///
    /// Args:
    ///     term_date (Date): The maturity the premium schedule is derived
    ///         from.
    ///     running_spread (float): The running spread the premium leg pays.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date the trade is dated off.
    ///     nominal (float | None): The notional; None keeps the core default
    ///         of 1.
    ///     upfront_rate (float | None): The upfront as a fraction of the
    ///         notional; None keeps the core default of none.
    ///     side (ProtectionSide | None): Which side the contract holds; None
    ///         keeps the core default of Buyer.
    ///     trade_date (Date | None): Overrides the evaluation date the trade
    ///         is otherwise dated off, which is how a contract traded in the
    ///         past is built.
    #[new]
    #[pyo3(signature = (
        term_date,
        running_spread,
        settings,
        nominal = None,
        upfront_rate = None,
        side = None,
        trade_date = None,
    ))]
    fn new(
        term_date: &PyDate,
        running_spread: f64,
        settings: &PySettings,
        nominal: Option<f64>,
        upfront_rate: Option<f64>,
        side: Option<PyProtectionSide>,
        trade_date: Option<&PyDate>,
    ) -> Self {
        PyMakeCreditDefaultSwap {
            term_date: term_date.inner(),
            running_spread,
            settings: settings.inner(),
            nominal,
            upfront_rate,
            side,
            trade_date: trade_date.map(PyDate::inner),
        }
    }

    /// Build the contract, which carries no engine.
    ///
    /// Attach one with set_engine or set_isda_engine before pricing.
    ///
    /// Returns:
    ///     CreditDefaultSwap: The contract on the market conventions.
    ///
    /// Raises:
    ///     ItofinError: If no evaluation date is set, the trade date being
    ///         derived from it, and on whatever the contract construction
    ///         rejects.
    fn build(&self) -> PyResult<PyCreditDefaultSwap> {
        let mut maker = MakeCreditDefaultSwap::from_term_date(
            self.term_date,
            self.running_spread,
            Shared::clone(&self.settings),
        );
        if let Some(nominal) = self.nominal {
            maker = maker.with_nominal(nominal);
        }
        if let Some(upfront_rate) = self.upfront_rate {
            maker = maker.with_upfront_rate(upfront_rate);
        }
        if let Some(side) = self.side {
            maker = maker.with_side(side.inner());
        }
        if let Some(trade_date) = self.trade_date {
            maker = maker.with_trade_date(trade_date);
        }
        let cds = maker.build().map_err(PyQlError::from)?;
        Ok(PyCreditDefaultSwap::from_inner(shared_mut(cds)))
    }
}
