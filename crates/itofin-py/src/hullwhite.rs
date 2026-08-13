//! Facades for the Hull-White short-rate stack: [`PyHullWhite`] and the
//! [`PyIborIndex`] index base with its [`PyEuribor`] subclass.

use crate::PyQlError;
use crate::calibration::{PyCalibrationErrorType, PyEndCriteria, PyLevenbergMarquardt};
use crate::currency::PyCurrency;
use crate::curve::PyYieldTermStructure;
use crate::option::PyOptionType;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod};
use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::{Euribor, IborIndex, Index};
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
/// It is the base of [`PyEuribor`], so the deposit and swap rate helpers take
/// this type and accept either. Every other index consumer still takes
/// `PyEuribor` concretely, including two in the rate-helper module itself: the
/// four `FraRateHelper` constructors and `FuturesRateHelper::from_index`. So do
/// the swap, swap-index, optionlet-volatility, cap/floor and swaption-helper
/// facades. Widening them is deferred to its own follow-up; no bootstrap this
/// ticket serves needs it.
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
}

impl PyIborIndex {
    /// A clone of the inner index for the rate-helper facades, which take a
    /// `&IborIndex` and are generic over the family.
    pub(crate) fn inner(&self) -> Shared<IborIndex> {
        Shared::clone(&self.inner)
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
/// object, not a rebuild - so the facades still typed on `PyEuribor` read
/// exactly what the base reads.
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

impl PyEuribor {
    /// A clone of the inner index for the swap/swaption facades.
    pub(crate) fn inner(&self) -> Shared<IborIndex> {
        Shared::clone(&self.inner)
    }
}

/// The base/subclass initializer shared by the three Euribor constructors: one
/// index object feeds both halves, so the base [`PyIborIndex`] the rate helpers
/// read and the [`PyEuribor`] the swap facades read are the same core index.
fn init_euribor(index: Shared<IborIndex>) -> PyClassInitializer<PyEuribor> {
    let base = PyIborIndex {
        inner: Shared::clone(&index),
    };
    PyClassInitializer::from(base).add_subclass(PyEuribor { inner: index })
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
        index: &PyEuribor,
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
