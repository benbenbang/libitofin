//! Facades for the Hull-White short-rate stack: [`PyHullWhite`] and the
//! [`PyIborIndex`] index base with its [`PyEuribor`], [`PyUsdLibor`],
//! [`PyJpyLibor`], [`PyGbpLibor`], [`PyEurLibor`] and [`PyCustomIborIndex`]
//! subclasses.

use crate::PyQlError;
use crate::calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use crate::currency::PyCurrency;
use crate::curve::PyYieldTermStructure;
use crate::option::PyOptionType;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::ibor::{CustomIborIndex, EurLibor, GbpLibor, JpyLibor};
use libitofin::indexes::{Euribor, IborIndex, Index, InterestRateIndex, UsdLibor};
use libitofin::models::calibrationhelper::{BlackCalibrationHelper, CalibrationHelper};
use libitofin::models::shortrate::SwaptionHelper;
use libitofin::models::{CalibratedModelHolder, HullWhite, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::JamshidianSwaptionEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::VolatilityType;
use libitofin::types::Natural;
use pyo3::prelude::*;

/// Python `HullWhite`: the one-factor Hull-White short-rate model
/// (`models::shortrate::hullwhite::HullWhite`).
///
/// The ctor is fallible (`hullwhite.rs:188`): it reads the curve's forward rate
/// at `0` and applies the Vasicek positivity constraints on `a`/`sigma`, so an
/// empty curve or a constraint-violating parameter surfaces as an `ItofinError`.
#[pyclass(name = "HullWhite", unsendable)]
pub struct PyHullWhite {
    inner: SharedMut<HullWhite>,
}

#[pymethods]
impl PyHullWhite {
    #[new]
    fn new(curve: &PyYieldTermStructure, a: f64, sigma: f64) -> PyResult<Self> {
        let inner = HullWhite::new(curve.handle(), a, sigma).map_err(PyQlError::from)?;
        Ok(PyHullWhite { inner })
    }

    /// The mean-reversion speed `a`, read as `params()[0]`.
    ///
    /// `HullWhite` exposes no direct `a()` (the Vasicek base field is private).
    /// The public route is the flattened calibrated-model parameters, whose
    /// `[0]`/`[1]` order (`a`, then `sigma`) is pinned by the core calibration
    /// oracle (`hullwhite.rs:892,898`).
    fn a(&self) -> f64 {
        self.inner.borrow().calibrated_model().params()[0]
    }

    /// The short-rate volatility `sigma`, read as `params()[1]`.
    fn sigma(&self) -> f64 {
        self.inner.borrow().calibrated_model().params()[1]
    }

    /// The fitted initial short rate `r0` (`hullwhite.rs:225`).
    fn r0(&self) -> f64 {
        self.inner.borrow().r0()
    }

    /// The price of a European option, exercised at `maturity`, on a zero-coupon
    /// bond maturing at `bond_maturity` (`hullwhite.rs:263`, the 4-argument
    /// overload). Fallible: the fitted curve must be linked and the underlying
    /// `black_formula` arguments valid.
    fn discount_bond_option(
        &self,
        option_type: PyOptionType,
        strike: f64,
        maturity: f64,
        bond_maturity: f64,
    ) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow()
            .discount_bond_option(option_type.inner(), strike, maturity, bond_maturity)
            .map_err(PyQlError::from)?)
    }

    /// Calibrates the model to `helpers` with `method` under `end_criteria`,
    /// then writes the fitted `a`/`sigma` back (readable through the getters).
    ///
    /// Mirrors the core oracle (`hullwhite.rs:809-864`): one
    /// [`JamshidianSwaptionEngine`] is built on this model and installed on every
    /// helper, so all swaptions price through the same analytic engine the
    /// optimizer drives (keeping W2 independent of the user-facing engine facade).
    /// `fix_reversion` pins the mean reversion `a` and frees only `sigma`
    /// (`fix_parameters = [true, false]`, `hullwhite.rs:1043`); otherwise both are
    /// free. [`calibrate`](libitofin::models::calibrate) fails on an empty helper
    /// list.
    fn calibrate(
        &mut self,
        helpers: Vec<PyRef<PySwaptionHelper>>,
        method: &mut PyLevenbergMarquardt,
        end_criteria: &PyEndCriteria,
        fix_reversion: bool,
    ) -> PyResult<()> {
        let engine = shared_mut(JamshidianSwaptionEngine::new(SharedMut::clone(&self.inner)))
            as SharedMut<dyn PricingEngine>;
        for helper in &helpers {
            helper
                .inner
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(SharedMut::clone(&engine));
        }
        let dyn_helpers: Vec<SharedMut<dyn CalibrationHelper>> = helpers
            .iter()
            .map(|helper| SharedMut::clone(&helper.inner) as SharedMut<dyn CalibrationHelper>)
            .collect();
        let fix_parameters = if fix_reversion {
            vec![true, false]
        } else {
            Vec::new()
        };
        calibrate(
            &self.inner,
            &dyn_helpers,
            method.inner_mut(),
            end_criteria.inner(),
            None,
            Vec::new(),
            fix_parameters,
        )
        .map_err(PyQlError::from)?;
        Ok(())
    }
}

impl PyHullWhite {
    /// A clone of the inner model handle for the calibration (W2) and
    /// Jamshidian-engine (X3) facades, which consume `SharedMut<HullWhite>`.
    pub(crate) fn inner(&self) -> SharedMut<HullWhite> {
        SharedMut::clone(&self.inner)
    }
}

/// Python `IborIndex`: a general Inter-Bank-Offered-Rate index
/// (`indexes::iborindex::IborIndex`).
///
/// The constructor spells out every convention the core ctor takes
/// (`iborindex.rs:58`), so an index outside the named families - the USD-3M
/// `IsdaIbor` the ISDA CDS curve bootstraps off, say - is expressible without a
/// dedicated facade. `forwarding` `None` builds the index over an empty handle,
/// the form the bootstrap rate helpers need, exactly as the C++ default
/// `Handle<YieldTermStructure> h = {}` allows.
///
/// It is the base of [`PyEuribor`], and since #868 every Ibor-index consumer
/// takes this type and accepts either: the deposit, swap, FRA and futures rate
/// helpers, and the swap, swap-index, optionlet-volatility, cap/floor and
/// swaption-helper facades. The OIS helper is not one of them; it takes the
/// overnight [`PyEstr`](crate::helpers::PyEstr), which is not an `IborIndex`.
#[pyclass(name = "IborIndex", subclass, unsendable)]
pub struct PyIborIndex {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyIborIndex {
    /// An index of `tenor` fixing `settlement_days` before its value date on
    /// `fixing_calendar`, rolling to maturity under `convention`/`end_of_month`,
    /// accruing on `day_counter` and forecasting off `forwarding` (or off an
    /// empty handle when `None`).
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        family_name,
        tenor,
        settlement_days,
        currency,
        fixing_calendar,
        convention,
        end_of_month,
        day_counter,
        forwarding,
        settings,
    ))]
    fn new(
        family_name: String,
        tenor: &PyPeriod,
        settlement_days: Natural,
        currency: &PyCurrency,
        fixing_calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
        end_of_month: bool,
        day_counter: &PyDayCounter,
        forwarding: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> Self {
        PyIborIndex {
            inner: shared(IborIndex::new(
                family_name,
                tenor.inner(),
                settlement_days,
                currency.inner(),
                fixing_calendar.inner(),
                convention.inner(),
                end_of_month,
                day_counter.inner(),
                forwarding
                    .map(|curve| curve.handle())
                    .unwrap_or_else(Handle::empty),
                settings.inner(),
            )),
        }
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// The value date of the loan fixed on `fixing_date`: the fixing date moved
    /// forward `fixing_days` business days on the fixing calendar
    /// (`interestrateindex.rs:210`). Fallible: the core rejects a `fixing_date`
    /// that is not a business day there.
    fn value_date(&self, fixing_date: &PyDate) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .value_date(fixing_date.inner())
                .map_err(PyQlError::from)?,
        ))
    }

    /// The fixing date of the loan starting on `value_date`: the value date
    /// moved back `fixing_days` business days (`interestrateindex.rs:197`), the
    /// inverse of [`Self::value_date`].
    fn fixing_date(&self, value_date: &PyDate) -> PyDate {
        PyDate::from_inner(self.inner.fixing_date(value_date.inner()))
    }

    /// The maturity of the loan starting on `value_date`: the value date rolled
    /// on by the index tenor under the index's own convention and end-of-month
    /// flag (`iborindex.rs:154`).
    fn maturity_date(&self, value_date: &PyDate) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .maturity_date(value_date.inner())
                .map_err(PyQlError::from)?,
        ))
    }

    /// The index tenor, normalized at construction
    /// (`interestrateindex.rs:176`).
    fn tenor(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.tenor())
    }

    /// The day counter the index accrues on (`interestrateindex.rs:191`).
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.inner.day_counter().clone())
    }

    /// The calendar the fixing and value dates roll on
    /// (`interestrateindex.rs:234`).
    fn fixing_calendar(&self) -> PyCalendar {
        PyCalendar::from_inner(self.inner.fixing_calendar())
    }

    /// The convention applied when rolling the value date to maturity
    /// (`iborindex.rs:89`). Infallible: the core returns the stored field.
    fn business_day_convention(&self) -> PyBusinessDayConvention {
        PyBusinessDayConvention::from_inner(self.inner.business_day_convention())
    }

    /// Whether the maturity roll keeps to month ends (`iborindex.rs:94`).
    fn end_of_month(&self) -> bool {
        self.inner.end_of_month()
    }

    /// The composed index name the fixings are stored under
    /// (`interestrateindex.rs:230`), e.g. `Euribor6M Actual/360`.
    fn name(&self) -> String {
        self.inner.name()
    }

    /// The currency the index is quoted in (`interestrateindex.rs:186`).
    fn currency(&self) -> PyCurrency {
        PyCurrency::from_inner(self.inner.currency().clone())
    }
}

impl PyIborIndex {
    /// A clone of the inner index for the rate-helper facades, which take a
    /// `&IborIndex` and are generic over the family.
    pub(crate) fn inner(&self) -> Shared<IborIndex> {
        Shared::clone(&self.inner)
    }
}

/// Python `CustomIborIndex`: an Ibor index with three separate calendars
/// (`indexes::ibor::CustomIborIndex`).
///
/// The general form of what [`PyEurLibor`] configures: fixing dates roll back
/// on the value calendar and adjust `Preceding` on the fixing calendar, value
/// dates advance on the value calendar, and maturity dates advance on the
/// maturity calendar (`custom.rs:5-13`). Passing the same calendar three times
/// reproduces a plain [`PyIborIndex`], so this is the escape hatch for a
/// LIBOR-like index outside the named families.
///
/// `CustomIborIndex::new` returns the newtype; `upcast()` (`custom.rs:105`)
/// hands out the inner `IborIndex` carrying the three-calendar roll as data, so
/// a consumer taking the base half rolls all three dates the way C++ does.
#[pyclass(name = "CustomIborIndex", extends = PyIborIndex, unsendable)]
pub struct PyCustomIborIndex {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyCustomIborIndex {
    /// An index of `tenor` fixing `settlement_days` before its value date,
    /// with each of the three date calculations rolling on its own calendar,
    /// accruing on `day_counter` and forecasting off `forwarding` (or off an
    /// empty handle when `None`).
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (
        family_name,
        tenor,
        settlement_days,
        currency,
        fixing_calendar,
        value_calendar,
        maturity_calendar,
        convention,
        end_of_month,
        day_counter,
        forwarding,
        settings,
    ))]
    fn new(
        family_name: String,
        tenor: &PyPeriod,
        settlement_days: Natural,
        currency: &PyCurrency,
        fixing_calendar: &PyCalendar,
        value_calendar: &PyCalendar,
        maturity_calendar: &PyCalendar,
        convention: &PyBusinessDayConvention,
        end_of_month: bool,
        day_counter: &PyDayCounter,
        forwarding: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyClassInitializer<Self> {
        let index = CustomIborIndex::new(
            family_name,
            tenor.inner(),
            settlement_days,
            currency.inner(),
            fixing_calendar.inner(),
            value_calendar.inner(),
            maturity_calendar.inner(),
            convention.inner(),
            end_of_month,
            day_counter.inner(),
            forwarding
                .map(|curve| curve.handle())
                .unwrap_or_else(Handle::empty),
            settings.inner(),
        )
        .upcast();
        let base = PyIborIndex {
            inner: Shared::clone(&index),
        };
        PyClassInitializer::from(base).add_subclass(PyCustomIborIndex { inner: index })
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// Python `Euribor`: the Euribor IBOR index family (`indexes::Euribor`).
///
/// The general constructor takes a tenor, an optional forwarding curve, and the
/// settings; passing `None` for the curve builds the index over an empty
/// handle, the form the bootstrap rate helpers need (#528). The `six_months`
/// and `three_months` staticmethods keep the curve-required convenience
/// constructors. `Euribor::new` returns an `IborIndex` by value; it is wrapped
/// in `shared()` so downstream ctors that take a `Shared<IborIndex>`
/// (VanillaSwap/SwaptionHelper/rate helpers) can hold the same object.
///
/// A subclass of [`PyIborIndex`], so a Euribor is accepted wherever the general
/// index is. It retains its own clone of the index the base holds - the same
/// object, not a rebuild - so its own `fixing` reads exactly what the base
/// reads.
#[pyclass(name = "Euribor", extends = PyIborIndex, unsendable)]
pub struct PyEuribor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyEuribor {
    /// A Euribor index of `tenor` forwarding off `curve`, or off an empty
    /// handle when `curve` is `None`. Fallible: the core rejects daily tenors
    /// (`euribor.rs:65`), which need the dedicated `DailyTenor` constructor.
    #[new]
    #[pyo3(signature = (tenor, curve, settings))]
    fn new(
        tenor: &PyPeriod,
        curve: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        let index =
            Euribor::new(tenor.inner(), forwarding, settings.inner()).map_err(PyQlError::from)?;
        Ok(init_euribor(shared(index)))
    }

    /// The 3-month Euribor index (`Euribor3M`) forwarding off `curve`.
    #[staticmethod]
    fn three_months(
        py: Python<'_>,
        curve: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let index = shared(Euribor::three_months(curve.handle(), settings.inner()));
        Py::new(py, init_euribor(index))
    }

    /// The 6-month Euribor index (`Euribor6M`) forwarding off `curve`.
    #[staticmethod]
    fn six_months(
        py: Python<'_>,
        curve: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let index = shared(Euribor::six_months(curve.handle(), settings.inner()));
        Py::new(py, init_euribor(index))
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The base/subclass initializer shared by the three Euribor constructors: one
/// index object feeds both halves, so the base [`PyIborIndex`] every consumer
/// reads and the [`PyEuribor`] the subclass holds are the same core index.
fn init_euribor(index: Shared<IborIndex>) -> PyClassInitializer<PyEuribor> {
    let base = PyIborIndex {
        inner: Shared::clone(&index),
    };
    PyClassInitializer::from(base).add_subclass(PyEuribor { inner: index })
}

/// Python `UsdLibor`: the USD Libor index family (`indexes::UsdLibor`).
///
/// The constructor takes a tenor, an optional forwarding curve, and the
/// settings; passing `None` for the curve builds the index over an empty
/// handle, the form the bootstrap rate helpers need. `UsdLibor::new` returns
/// a plain `IborIndex` by value (`usdlibor.rs:42`); it is wrapped in
/// `shared()` exactly as the Euribor facade wraps its family.
///
/// A subclass of [`PyIborIndex`], so a USD Libor is accepted wherever the
/// general index is. It retains its own clone of the index the base holds -
/// the same object, not a rebuild - so its own `fixing` reads exactly what
/// the base reads.
#[pyclass(name = "UsdLibor", extends = PyIborIndex, unsendable)]
pub struct PyUsdLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyUsdLibor {
    /// A USD Libor index of `tenor` forwarding off `curve`, or off an empty
    /// handle when `curve` is `None`. Fallible: the Libor base rejects daily
    /// tenors (`libor.rs:89`), which need a dedicated `DailyTenor`
    /// constructor the core has not ported.
    #[new]
    #[pyo3(signature = (tenor, curve, settings))]
    fn new(
        tenor: &PyPeriod,
        curve: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        let index =
            UsdLibor::new(tenor.inner(), forwarding, settings.inner()).map_err(PyQlError::from)?;
        let index = shared(index);
        let base = PyIborIndex {
            inner: Shared::clone(&index),
        };
        Ok(PyClassInitializer::from(base).add_subclass(PyUsdLibor { inner: index }))
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// Python `JpyLibor`: the JPY Libor index family (`indexes::ibor::JpyLibor`).
///
/// The constructor takes a tenor, an optional forwarding curve, and the
/// settings; passing `None` for the curve builds the index over an empty
/// handle, the form the bootstrap rate helpers need. `JpyLibor::new` returns
/// a plain `IborIndex` by value (`jpylibor.rs:43`); it is wrapped in
/// `shared()` exactly as the Euribor facade wraps its family.
///
/// A subclass of [`PyIborIndex`], so a JPY Libor is accepted wherever the
/// general index is. It retains its own clone of the index the base holds -
/// the same object, not a rebuild - so its own `fixing` reads exactly what
/// the base reads.
#[pyclass(name = "JpyLibor", extends = PyIborIndex, unsendable)]
pub struct PyJpyLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyJpyLibor {
    /// A JPY Libor index of `tenor` forwarding off `curve`, or off an empty
    /// handle when `curve` is `None`. Fallible: the Libor base rejects daily
    /// tenors (`libor.rs:89`), which need a dedicated `DailyTenor`
    /// constructor the core has not ported.
    #[new]
    #[pyo3(signature = (tenor, curve, settings))]
    fn new(
        tenor: &PyPeriod,
        curve: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        let index =
            JpyLibor::new(tenor.inner(), forwarding, settings.inner()).map_err(PyQlError::from)?;
        let index = shared(index);
        let base = PyIborIndex {
            inner: Shared::clone(&index),
        };
        Ok(PyClassInitializer::from(base).add_subclass(PyJpyLibor { inner: index }))
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// Python `GbpLibor`: the GBP Libor index family (`indexes::ibor::GbpLibor`).
///
/// The constructor takes a tenor, an optional forwarding curve, and the
/// settings; passing `None` for the curve builds the index over an empty
/// handle, the form the bootstrap rate helpers need. `GbpLibor::new` returns
/// a plain `IborIndex` by value (`gbplibor.rs:45`); it is wrapped in
/// `shared()` exactly as the Euribor facade wraps its family.
///
/// A subclass of [`PyIborIndex`], so a GBP Libor is accepted wherever the
/// general index is. It retains its own clone of the index the base holds -
/// the same object, not a rebuild - so its own `fixing` reads exactly what
/// the base reads.
#[pyclass(name = "GbpLibor", extends = PyIborIndex, unsendable)]
pub struct PyGbpLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyGbpLibor {
    /// A GBP Libor index of `tenor` forwarding off `curve`, or off an empty
    /// handle when `curve` is `None`. Fallible: the Libor base rejects daily
    /// tenors (`libor.rs:89`), which need a dedicated `DailyTenor`
    /// constructor the core has not ported.
    #[new]
    #[pyo3(signature = (tenor, curve, settings))]
    fn new(
        tenor: &PyPeriod,
        curve: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        let index =
            GbpLibor::new(tenor.inner(), forwarding, settings.inner()).map_err(PyQlError::from)?;
        let index = shared(index);
        let base = PyIborIndex {
            inner: Shared::clone(&index),
        };
        Ok(PyClassInitializer::from(base).add_subclass(PyGbpLibor { inner: index }))
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// Python `EurLibor`: the EUR Libor index family (`indexes::ibor::EurLibor`).
///
/// The constructor takes a tenor, an optional forwarding curve, and the
/// settings; passing `None` for the curve builds the index over an empty
/// handle, the form the bootstrap rate helpers need. `EurLibor::new` returns a
/// `CustomIborIndex` (`eurlibor.rs:64`), the three-calendar index whose fixing
/// dates roll on the joint UK-Exchange-plus-TARGET calendar and whose value and
/// maturity dates roll on TARGET alone; `upcast()` (`custom.rs:105`) hands out
/// the inner `IborIndex` carrying that roll as data, so the facade holds one
/// shared index like the other families.
///
/// A subclass of [`PyIborIndex`], so a EUR Libor is accepted wherever the
/// general index is. It retains its own clone of the index the base holds -
/// the same object, not a rebuild - so its own `fixing` reads exactly what
/// the base reads.
#[pyclass(name = "EurLibor", extends = PyIborIndex, unsendable)]
pub struct PyEurLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyEurLibor {
    /// A EUR Libor index of `tenor` forwarding off `curve`, or off an empty
    /// handle when `curve` is `None`. Fallible: the core rejects daily tenors
    /// (`eurlibor.rs:87`), which need the dedicated `DailyTenorEURLibor`
    /// constructor the core has not ported.
    #[new]
    #[pyo3(signature = (tenor, curve, settings))]
    fn new(
        tenor: &PyPeriod,
        curve: Option<&PyYieldTermStructure>,
        settings: &PySettings,
    ) -> PyResult<PyClassInitializer<Self>> {
        let forwarding = match curve {
            Some(curve) => curve.handle(),
            None => Handle::empty(),
        };
        let index =
            EurLibor::new(tenor.inner(), forwarding, settings.inner()).map_err(PyQlError::from)?;
        let index = index.upcast();
        let base = PyIborIndex {
            inner: Shared::clone(&index),
        };
        Ok(PyClassInitializer::from(base).add_subclass(PyEurLibor { inner: index }))
    }

    /// The index's fixing for `fixing_date`, forecast off the forwarding curve
    /// for a future date or read from stored fixings for a past one. Fallible:
    /// an empty forwarding handle or an unset evaluation date is an error.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// Python `SwaptionHelper`: a co-terminal swaption calibration instrument
/// (`models::shortrate::calibrationhelpers::swaptionhelper::SwaptionHelper`).
///
/// The helper builds its own European swaption from the maturity/length and the
/// index, so no swap or swaption facade is needed. The volatility is assembled
/// into a `Handle<dyn Quote>` internally from a `SimpleQuote`. The oracle's fixed
/// defaults are pinned here (`hullwhite.rs:839-844`): `strike = None` (struck at
/// the forward), `ShiftedLognormal` volatility with zero shift, the index's own
/// settlement days, and `Compound` averaging. Held as `SharedMut` so a
/// calibration can install the Jamshidian engine on it and upcast it to
/// `SharedMut<dyn CalibrationHelper>`.
#[pyclass(name = "SwaptionHelper", unsendable)]
pub struct PySwaptionHelper {
    inner: SharedMut<SwaptionHelper>,
}

#[pymethods]
impl PySwaptionHelper {
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        maturity: &PyPeriod,
        length: &PyPeriod,
        volatility: f64,
        index: &PyIborIndex,
        fixed_leg_tenor: &PyPeriod,
        fixed_leg_day_counter: &PyDayCounter,
        floating_leg_day_counter: &PyDayCounter,
        curve: &PyYieldTermStructure,
        error_type: &PyCalibrationErrorType,
        nominal: f64,
    ) -> Self {
        let vol = Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
        PySwaptionHelper {
            inner: shared_mut(SwaptionHelper::new(
                maturity.inner(),
                length.inner(),
                vol,
                index.inner(),
                fixed_leg_tenor.inner(),
                fixed_leg_day_counter.inner(),
                floating_leg_day_counter.inner(),
                curve.handle(),
                error_type.inner(),
                None,
                nominal,
                VolatilityType::ShiftedLognormal,
                0.0,
                None,
                RateAveraging::Compound,
            )),
        }
    }

    /// The calibration error under the helper's error type, after a calibration
    /// has installed a pricing engine on it.
    fn calibration_error(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .calibration_error()
            .map_err(PyQlError::from)?)
    }
}
