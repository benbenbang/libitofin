//! Facades for the credit hierarchy: the [`PyDefaultProbabilityTermStructure`]
//! base, the concrete [`PyFlatHazardRate`], [`PyInterpolatedHazardRateCurve`]
//! and [`PyPiecewiseDefaultCurve`] curves, the [`PyProtectionSide`] and
//! [`PyPricingModel`] flags, the [`PyCreditDefaultSwap`] instrument and its
//! market-convention builder [`PyMakeCreditDefaultSwap`].

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

/// Python `ProtectionSide`: which leg of a default-protection contract a party
/// holds (core `instruments::ProtectionSide`).
///
/// A fieldless pyo3 enum exposing `ProtectionSide.Buyer` /
/// `ProtectionSide.Seller`; the buyer pays the premium leg and receives the
/// default payment, the seller the reverse.
#[pyclass(name = "ProtectionSide", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyProtectionSide {
    Buyer,
    Seller,
}

impl PyProtectionSide {
    /// The core [`ProtectionSide`] this variant stands for.
    pub(crate) fn inner(self) -> ProtectionSide {
        match self {
            PyProtectionSide::Buyer => ProtectionSide::Buyer,
            PyProtectionSide::Seller => ProtectionSide::Seller,
        }
    }
}

/// Python `PricingModel`: the model a quoted contract is inverted under
/// (core `instruments::PricingModel`).
///
/// A fieldless pyo3 enum exposing `PricingModel.Midpoint` / `PricingModel.Isda`,
/// the two choices
/// [`CreditDefaultSwap.implied_hazard_rate`](PyCreditDefaultSwap::implied_hazard_rate)
/// solves on. `Midpoint` is not ISDA conform; `Isda` carries the three fidelity
/// flags the core fixes at that call site.
#[pyclass(name = "PricingModel", eq, eq_int, from_py_object)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyPricingModel {
    Midpoint,
    Isda,
}

impl PyPricingModel {
    /// The core [`PricingModel`] this variant stands for.
    pub(crate) fn inner(self) -> PricingModel {
        match self {
            PyPricingModel::Midpoint => PricingModel::Midpoint,
            PyPricingModel::Isda => PricingModel::Isda,
        }
    }
}

/// Python `DefaultProbabilityTermStructure`: the shared base for every credit
/// curve (`termstructures::credit::defaulttermstructure`).
///
/// Holds the erased `Handle<dyn DefaultProbabilityTermStructure>` and exposes
/// the query surface every concrete curve inherits: survival and default
/// probabilities, the default density, and the hazard rate, each in a
/// year-fraction and a date form. Concrete curves such as [`PyFlatHazardRate`]
/// subclass this and supply only their constructor.
#[pyclass(name = "DefaultProbabilityTermStructure", subclass, unsendable)]
pub struct PyDefaultProbabilityTermStructure {
    inner: Handle<dyn DefaultProbabilityTermStructure>,
}

#[pymethods]
impl PyDefaultProbabilityTermStructure {
    /// The survival probability from the reference date to year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn survival_probability(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .survival_probability(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The survival probability from the reference date to `date`.
    #[pyo3(signature = (date, extrapolate = false))]
    fn survival_probability_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .survival_probability_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default probability from the reference date to year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn default_probability(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_probability(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default probability from the reference date to `date`.
    #[pyo3(signature = (date, extrapolate = false))]
    fn default_probability_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_probability_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default density at year-fraction `t`.
    #[pyo3(signature = (t, extrapolate = false))]
    fn default_density(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_density(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The default density at `date`.
    #[pyo3(signature = (date, extrapolate = false))]
    fn default_density_date(&self, date: &PyDate, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .default_density_date(date.inner(), extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The hazard rate at year-fraction `t`, annual frequency and continuous
    /// compounding.
    #[pyo3(signature = (t, extrapolate = false))]
    fn hazard_rate(&self, t: f64, extrapolate: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .current_link()
            .map_err(PyQlError::from)?
            .hazard_rate(t, extrapolate)
            .map_err(PyQlError::from)?)
    }

    /// The hazard rate at `date`, annual frequency and continuous compounding.
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
    /// The base half of a concrete curve's [`PyClassInitializer`] chain.
    pub(crate) fn from_handle(inner: Handle<dyn DefaultProbabilityTermStructure>) -> Self {
        PyDefaultProbabilityTermStructure { inner }
    }

    /// A clone of the inner curve handle for the CDS instrument and engine
    /// facades that take a credit curve.
    pub(crate) fn handle(&self) -> Handle<dyn DefaultProbabilityTermStructure> {
        self.inner.clone()
    }
}

/// Python `FlatHazardRate`: a credit curve quoting one hazard rate for every
/// maturity, with the closed-form survival probability `exp(-h t)`
/// (`termstructures::credit::flathazardrate::FlatHazardRate`).
///
/// Extends [`PyDefaultProbabilityTermStructure`], which carries the whole query
/// surface. All four core constructors are infallible, so `__init__` hands back
/// the initializer directly and the three alternates are staticmethods
/// returning the built object, the shape [`PyFraRateHelper`](crate::helpers)
/// already uses. The quote-backed forms retain the caller's [`PySimpleQuote`],
/// so a later `set_value` moves the curve; the rate-backed forms wrap the value
/// in a fresh, un-retained quote. The `moving` forms take an explicit
/// [`PySettings`] (D5) and fix the reference date `settlement_days` business
/// days past the evaluation date, so a query before one is set returns
/// [`struct@crate::ItofinError`] rather than falling back to a system clock.
#[pyclass(name = "FlatHazardRate", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyFlatHazardRate;

#[pymethods]
impl PyFlatHazardRate {
    /// A curve reading `hazard_rate` live, with a fixed `reference_date`.
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

    /// A curve at a fixed `rate`, with a fixed `reference_date`.
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

    /// A curve reading `hazard_rate` live, whose reference date moves off the
    /// evaluation date held by `settings`.
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

    /// A curve at a fixed `rate`, whose reference date moves off the evaluation
    /// date held by `settings`.
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

/// Python `InterpolatedHazardRateCurve`: a credit curve built from
/// (date, hazard-rate) nodes, interpolating backward-flat
/// (`termstructures::credit::InterpolatedHazardRateCurve<BackwardFlat>`).
///
/// Extends [`PyDefaultProbabilityTermStructure`], which carries the whole query
/// surface. The first date is the reference date, and the day counter turns the
/// rest into node times. Backward-flat reads the right-hand node on every
/// segment, so the hazard rate is a right-continuous step function and the
/// survival probability is `exp(-integral)` over those steps. Finite in time:
/// queries past the last node need `extrapolate=True`, which continues at the
/// last node's rate.
///
/// The concrete curve is retained alongside the erased handle so the node
/// inspectors [`dates`](Self::dates), [`hazard_rates`](Self::hazard_rates) and
/// [`nodes`](Self::nodes) stay reachable, the shape
/// [`PyPiecewiseLogLinearDiscount`](crate::curve) uses.
///
/// Fallible, like [`PySpreadCdsHelper`](crate::credithelpers) and unlike the
/// [`PyFlatHazardRate`] chain: the core rejects too few dates, a dates/rates
/// count mismatch, a negative hazard rate and unsorted dates
/// (`interpolatedhazardratecurve.rs:75-88`), each as
/// [`struct@crate::ItofinError`].
///
/// `BackwardFlat` is pinned at the boundary: it is the only interpolator the
/// credit side wires (`interpolatedhazardratecurve.rs:148-152`), so no
/// interpolation argument is offered.
///
/// Deferred (visible): the calendar-carrying `with_calendar` constructor
/// (`interpolatedhazardratecurve.rs:68`) is not exposed, so the curve has no
/// calendar; and `times()` / `data()` are omitted, the former derivable from
/// the day counter and the latter identical to [`hazard_rates`](Self::hazard_rates).
#[pyclass(name = "InterpolatedHazardRateCurve", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyInterpolatedHazardRateCurve {
    concrete: Shared<InterpolatedHazardRateCurve<BackwardFlat>>,
}

#[pymethods]
impl PyInterpolatedHazardRateCurve {
    /// A curve over the `(dates, hazard_rates)` nodes, with `dates[0]` as the
    /// reference date.
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

    /// The node dates, the first of which is the reference date.
    fn dates(&self) -> Vec<PyDate> {
        self.concrete
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }

    /// The node hazard rates.
    fn hazard_rates(&self) -> Vec<f64> {
        self.concrete.hazard_rates().to_vec()
    }

    /// The `(date, hazard rate)` nodes.
    fn nodes(&self) -> Vec<(PyDate, f64)> {
        self.concrete
            .nodes()
            .into_iter()
            .map(|(date, rate)| (PyDate::from_inner(date), rate))
            .collect()
    }
}

/// Python `PiecewiseDefaultCurve`: a credit curve bootstrapped from CDS helpers,
/// solving one hazard-rate node per helper maturity
/// (`termstructures::credit::PiecewiseDefaultCurve<HazardRate, BackwardFlat>`).
///
/// Extends [`PyDefaultProbabilityTermStructure`], which carries the whole query
/// surface. Each helper's maturity marks a segment boundary, and its node is
/// solved so the helper reprices its own quote off the curve; the resulting
/// curve is the market-implied credit term structure the helpers quote.
///
/// Lazy, like the yield-side [`PyPiecewiseLogLinearDiscount`](crate::curve): the
/// constructor only registers on the helpers, and the bootstrap runs on the
/// first read - a query, an inspector, or an explicit
/// [`calculate`](Self::calculate). A helper quote moving invalidates the cache,
/// so the next read re-bootstraps. Both the helpers' `Settings` flags (notably
/// `include_todays_cash_flows`) and the evaluation date must therefore be in
/// place before the first read, not merely before the constructor.
///
/// The concrete curve is retained alongside the erased handle so the node
/// inspectors stay reachable, as
/// [`PyInterpolatedHazardRateCurve`] and [`PyPiecewiseLogLinearDiscount`](crate::curve)
/// both do. All five are exposed here, where the interpolated curve omits
/// `times()` and `data()`: on a bootstrapped curve the solved node values are
/// the result rather than a constructor input, so there is nothing for a caller
/// to read them back from.
///
/// Fallible: the core rejects an empty helper set, and every inspector
/// propagates a bootstrap failure as [`struct@crate::ItofinError`].
///
/// `HazardRate` and `BackwardFlat` are pinned at the boundary: they are the only
/// traits/interpolator pair the core wires
/// (`piecewisedefaultcurve.rs:36-45`), so neither is offered as an argument.
#[pyclass(name = "PiecewiseDefaultCurve", extends = PyDefaultProbabilityTermStructure, unsendable)]
pub struct PyPiecewiseDefaultCurve {
    concrete: Shared<PiecewiseDefaultCurve<HazardRate, BackwardFlat>>,
}

#[pymethods]
impl PyPiecewiseDefaultCurve {
    /// A curve over `helpers` with a fixed `reference_date`. `helpers` accepts
    /// any [`DefaultProbabilityHelper`](PyDefaultProbabilityHelper) subclass.
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

    /// Runs the bootstrap if the cache is stale, so a solver failure surfaces
    /// here rather than inside a later query.
    fn calculate(&self) -> PyResult<()> {
        Ok(self.concrete.calculate().map_err(PyQlError::from)?)
    }

    /// The node times, in the curve's own day count (triggers the bootstrap).
    fn times(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.times().map_err(PyQlError::from)?)
    }

    /// The node dates, the first of which is the reference date (triggers the
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

    /// The solved node hazard rates (triggers the bootstrap).
    fn data(&self) -> PyResult<Vec<f64>> {
        Ok(self.concrete.data().map_err(PyQlError::from)?)
    }

    /// The solved `(date, hazard rate)` nodes (triggers the bootstrap).
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

/// Python `CreditDefaultSwap`: a credit-default swap quoted as a running spread
/// (`instruments::creditdefaultswap::CreditDefaultSwap`).
///
/// One side pays the premium leg and receives the protection payment, the other
/// the reverse; [`ProtectionSide`] says which way round. The instrument is held
/// behind a `SharedMut` because every priced accessor takes `&mut self`
/// (`creditdefaultswap.rs:516-576`).
///
/// `__init__` takes the C++ default terms with `settles_accrual` and
/// `pays_at_default_time` quoted; [`with_terms`](Self::with_terms) additionally
/// exposes `protection_start` and `rebates_accrual`. Both are fallible: the
/// schedule must be non-empty, the protection start must not follow the first
/// accrual date under a pre-Big-Bang date-generation rule, and the premium leg
/// carries its own preconditions.
///
/// Pricing needs an engine: call [`set_engine`](Self::set_engine) before
/// [`npv`](Self::npv).
///
/// Deferred (visible): five of the nine `CdsTerms` fields are not exposed and
/// keep their core defaults - `claim` (a `FaceValueClaim`, which needs a claim
/// facade that does not exist yet), `last_period_day_counter` (the spread's own
/// day counter), `trade_date` (deduced from the protection start),
/// `upfront_date` (deduced from the trade date) and `cash_settlement_days` (3).
/// The core's upfront-quoted constructors are likewise not exposed here, so
/// `upfront` is always absent.
#[pyclass(name = "CreditDefaultSwap", unsendable)]
pub struct PyCreditDefaultSwap {
    inner: SharedMut<CreditDefaultSwap>,
}

#[pymethods]
impl PyCreditDefaultSwap {
    /// A contract on the C++ default terms, accruing and paying as
    /// `settles_accrual` and `pays_at_default_time` say.
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

    /// A contract quoting the terms `__init__` defaults.
    ///
    /// `protection_start` is the first date a default triggers the contract;
    /// `None` takes the schedule's first date, which is what `__init__` does.
    /// The three flags carry the core defaults verbatim, so calling this with
    /// only the positional arguments builds exactly what `__init__` builds.
    ///
    /// `settings` precedes the defaulted terms because a Python signature
    /// cannot put a required argument after an optional one; the positional
    /// order otherwise matches `__init__`.
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

    /// Attaches a [`PyMidPointCdsEngine`] so the contract prices.
    ///
    /// The engine is built separately and installed here, so one engine can be
    /// shared across contracts. It must resolve its dates against the same
    /// `Settings` object this contract was built with.
    fn set_engine(&mut self, engine: &PyMidPointCdsEngine) {
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.engine());
    }

    /// Attaches a [`PyIsdaCdsEngine`] so the contract prices under the ISDA
    /// standard model.
    ///
    /// A separate setter rather than a widened [`set_engine`](Self::set_engine):
    /// the two engine facades are unrelated pyo3 classes, so one argument cannot
    /// name both. The same sharing and same-`Settings` rules apply, and the ISDA
    /// engine additionally refuses curves outside its specification when the
    /// contract prices.
    fn set_isda_engine(&mut self, engine: &PyIsdaCdsEngine) {
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

    /// Attaches the mid-point engine `engine` and returns the NPV, the one-shot
    /// form of [`set_engine`](Self::set_engine) followed by [`npv`](Self::npv).
    ///
    /// The mid-point engine is the primary because it is the core's own default
    /// CDS engine; [`set_isda_engine`](Self::set_isda_engine) stays a separate
    /// setter and composes with [`calculate`](Self::calculate) and
    /// [`results`](Self::results) as before.
    fn price(&mut self, engine: &PyMidPointCdsEngine) -> PyResult<f64> {
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

    /// The contract NPV under the attached engine.
    ///
    /// Fallible: with no engine attached the core reports `"null pricing
    /// engine"` as an `ItofinError`.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }

    /// The running spread that prices the contract at zero.
    ///
    /// Fallible as [`npv`](Self::npv), and additionally when the engine priced
    /// a worthless premium leg and so provided no fair spread.
    fn fair_spread(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_spread()
            .map_err(PyQlError::from)?)
    }

    /// The upfront that prices the contract at zero, as a fraction of the
    /// notional. Fallible as [`fair_spread`](Self::fair_spread).
    fn fair_upfront(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .fair_upfront()
            .map_err(PyQlError::from)?)
    }

    /// The notional the premium and the protection are quoted on.
    fn notional(&self) -> f64 {
        self.inner.borrow().notional()
    }

    /// The accrued coupon the protection seller rebates, or `None` when the
    /// contract does not rebate accrual at all (`rebates_accrual` false).
    ///
    /// A contract traded in the past still carries the flow: the core builds it
    /// whenever the flag is set, regardless of the trade date
    /// (`creditdefaultswap.rs:515-544`), so `None` here means the flag, never a
    /// stale trade. Such a flow carries a real accrued amount but settled on a
    /// past date, so it no longer reaches the value.
    ///
    /// The amount is returned bare rather than behind a cash-flow facade, there
    /// being none. Fallible in principle, the core's amount being a `Result`.
    fn accrual_rebate_amount(&self) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .borrow()
            .accrual_rebate()
            .map(|rebate| rebate.amount())
            .transpose()
            .map_err(PyQlError::from)?)
    }

    /// The date the accrual rebate settles on, which is the cash-settlement
    /// date the upfront also pays on, or `None` on the same terms as
    /// [`accrual_rebate_amount`](Self::accrual_rebate_amount).
    fn accrual_rebate_date(&self) -> Option<PyDate> {
        self.inner
            .borrow()
            .accrual_rebate()
            .map(|rebate| PyDate::from_inner(Event::date(rebate.as_ref())))
    }

    /// The premium leg's NPV. Fallible as [`fair_spread`](Self::fair_spread).
    fn coupon_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .coupon_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// The protection leg's NPV. Fallible as
    /// [`fair_spread`](Self::fair_spread).
    fn default_leg_npv(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .default_leg_npv()
            .map_err(PyQlError::from)?)
    }

    /// The flat hazard rate at which this contract is worth `target_npv`.
    ///
    /// The solve stands on its own engine rather than on whichever one
    /// [`set_engine`](Self::set_engine) attached: it builds a flat, quote-backed
    /// probability curve counting `day_counter` and prices on `model` against
    /// `discount`, solving to `accuracy` on the rate. There is therefore no
    /// probability-curve argument - the curve being solved for is the one the
    /// core builds.
    ///
    /// `day_counter` is the day counter of that internal curve, not of the
    /// contract. Under [`PricingModel`]`.Isda` both it and `discount` must count
    /// Act/365 (Fixed), which is what the ISDA engine requires of its curves.
    ///
    /// Fallible: on a malformed contract, and when the solve does not converge -
    /// which includes a pricing failure at some hazard rate, since that reaches
    /// the solver as the non-finite value it rejects.
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
    /// [`PyMakeCreditDefaultSwap::build`] hands back.
    pub(crate) fn from_inner(inner: SharedMut<CreditDefaultSwap>) -> Self {
        PyCreditDefaultSwap { inner }
    }
}

/// Python `MakeCreditDefaultSwap`: the market-convention builder for a
/// credit-default swap (`instruments::makecds::MakeCreditDefaultSwap`).
///
/// It derives the premium schedule from a maturity and the post-Big-Bang CDS
/// conventions, and takes the trade date from the evaluation date the settings
/// carry (D5). Only the term-date quotation is exposed here; the tenor and
/// explicit-schedule quotations (`makecds.rs:111` and `:133`) are deferred.
///
/// The core builder is a consumed-self fluent chain, which does not cross the
/// FFI boundary; this facade takes the whole configuration up front and
/// assembles the chain inside [`build`](Self::build), as
/// [`PyMakeYoYInflationCapFloor`](crate::inflation) and
/// [`PyMakeVanillaSwap`](crate::swap) do. Each `build` runs a fresh chain, so
/// one builder object cannot carry a setting into a later contract. An unset
/// optional leaves the core default in place (`makecds.rs:150-171`).
///
/// Deferred (visible): four of the core's setters are exposed. The
/// accrual-rebate flag (`makecds.rs:257`) stays unbound, the `with_terms`
/// constructor on [`PyCreditDefaultSwap`] already reaching it; the rest keep
/// their defaults, notably the 3M coupon tenor, the pre-CDS2015
/// `DateGeneration::CDS` rule, the `Following` roll, the Act/360 day counter
/// and the three cash-settlement days.
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
    /// A builder for a contract maturing on `term_date` and paying
    /// `running_spread`.
    ///
    /// `trade_date` overrides the evaluation date the trade would otherwise be
    /// dated off (`makecds.rs:263`), which is how a contract traded in the past
    /// is built.
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

    /// Builds the contract, which carries no engine: attach one with
    /// [`set_engine`](PyCreditDefaultSwap::set_engine) or
    /// [`set_isda_engine`](PyCreditDefaultSwap::set_isda_engine) before pricing.
    ///
    /// # Errors
    ///
    /// Reports an unset evaluation date, which the trade date is derived from
    /// (`makecds.rs:286-294`), and whatever the contract construction rejects.
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
