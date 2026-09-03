//! Facades for the Hull-White short-rate stack: HullWhite and the IborIndex
//! index base with its Euribor, UsdLibor, JpyLibor, GbpLibor, EurLibor and
//! CustomIborIndex subclasses.

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

/// The one-factor Hull-White short-rate model.
///
/// Fitted to the term structure it is built on; a calibration overwrites a and
/// sigma in place, so the getters read the fitted values afterwards.
#[pyclass(name = "HullWhite", unsendable)]
pub struct PyHullWhite {
    inner: SharedMut<HullWhite>,
}

#[pymethods]
impl PyHullWhite {
    /// Fit the model to a term structure.
    ///
    /// Args:
    ///     curve (YieldTermStructure): The term structure the model fits; its forward rate at 0 is
    ///         read at construction.
    ///     a (float): The mean-reversion speed, under the Vasicek positivity
    ///         constraint.
    ///     sigma (float): The short-rate volatility, under the same constraint.
    ///
    /// Raises:
    ///     ItofinError: If the curve is empty or a parameter violates its
    ///         constraint.
    #[new]
    fn new(curve: &PyYieldTermStructure, a: f64, sigma: f64) -> PyResult<Self> {
        let inner = HullWhite::new(curve.handle(), a, sigma).map_err(PyQlError::from)?;
        Ok(PyHullWhite { inner })
    }

    /// Return the mean-reversion speed.
    ///
    /// Returns:
    ///     float: The current value of a, read as the first calibrated-model
    ///     parameter.
    fn a(&self) -> f64 {
        self.inner.borrow().calibrated_model().params()[0]
    }

    /// Return the short-rate volatility.
    ///
    /// Returns:
    ///     float: The current value of sigma, read as the second calibrated-model
    ///     parameter.
    fn sigma(&self) -> f64 {
        self.inner.borrow().calibrated_model().params()[1]
    }

    /// Return the fitted initial short rate.
    ///
    /// Returns:
    ///     float: The short rate r0 implied by the fitted term structure.
    fn r0(&self) -> f64 {
        self.inner.borrow().r0()
    }

    /// Price a European option on a zero-coupon bond.
    ///
    /// Args:
    ///     option_type (OptionType): Call or put.
    ///     strike (float): The option strike, as a bond price.
    ///     maturity (float): The option expiry, as a time in years.
    ///     bond_maturity (float): The maturity of the underlying zero-coupon bond, as a
    ///         time in years.
    ///
    /// Returns:
    ///     float: The option price.
    ///
    /// Raises:
    ///     ItofinError: If the fitted curve is not linked or the arguments are
    ///         rejected by the underlying Black formula.
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

    /// Fit a and sigma to the helpers and write them back.
    ///
    /// One Jamshidian swaption engine is built on this model and installed on
    /// every helper, so all swaptions price through the same analytic engine
    /// the optimizer drives.
    ///
    /// Args:
    ///     helpers (list[SwaptionHelper]): The calibration instruments to fit; must not be empty.
    ///     method (LevenbergMarquardt): The optimizer driving the fit.
    ///     end_criteria (EndCriteria): The stopping rule handed to the optimizer.
    ///     fix_reversion (bool): Pin the mean reversion a and free only sigma; when
    ///         False both parameters are free.
    ///
    /// Raises:
    ///     ItofinError: If helpers is empty or the optimization itself fails.
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

/// A general Inter-Bank-Offered-Rate index, spelling out every convention.
///
/// The form for an index outside the named families (the USD-3M IsdaIbor the
/// ISDA CDS curve bootstraps off, say). Pass forwarding=None to build it over
/// an empty handle, the form the bootstrap rate helpers need.
///
/// It is the base of Euribor, and every Ibor-index consumer takes this type and
/// accepts either: the deposit, swap, FRA and futures rate helpers, and the
/// swap, swap-index, optionlet-volatility, cap/floor and swaption-helper
/// facades. The OIS helper is not one of them; it takes the overnight Estr,
/// which is not an IborIndex.
#[pyclass(name = "IborIndex", subclass, unsendable)]
pub struct PyIborIndex {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyIborIndex {
    /// Build an index spelling out every convention the core constructor takes.
    ///
    /// The index fixes settlement_days before its value date on the fixing
    /// calendar, rolls to maturity under convention and end_of_month, accrues
    /// on day_counter and forecasts off forwarding.
    ///
    /// Args:
    ///     family_name (str): The index family the fixings are stored under.
    ///     tenor (Period): The index tenor, normalized at construction.
    ///     settlement_days (int): The business days between the fixing date and
    ///         the value date.
    ///     currency (Currency): The currency the index is quoted in.
    ///     fixing_calendar (Calendar): The calendar the fixing and value dates
    ///         roll on.
    ///     convention (BusinessDayConvention): The convention applied when
    ///         rolling the value date to maturity.
    ///     end_of_month (bool): Whether the maturity roll keeps to month ends.
    ///     day_counter (DayCounter): The day count the index accrues on.
    ///     forwarding (YieldTermStructure | None): The curve fixings are
    ///         forecast off; None builds the index over an empty handle, the
    ///         form the bootstrap rate helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
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

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }

    /// Return the value date of the loan fixed on fixing_date.
    ///
    /// The fixing date moved forward by the index's fixing days on the fixing
    /// calendar.
    ///
    /// Args:
    ///     fixing_date (Date): The fixing date to advance.
    ///
    /// Returns:
    ///     Date: The value date.
    ///
    /// Raises:
    ///     ItofinError: If fixing_date is not a business day on the fixing
    ///         calendar.
    fn value_date(&self, fixing_date: &PyDate) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .value_date(fixing_date.inner())
                .map_err(PyQlError::from)?,
        ))
    }

    /// Return the fixing date of the loan starting on value_date.
    ///
    /// The value date moved back by the index's fixing days, the inverse of
    /// value_date.
    ///
    /// Args:
    ///     value_date (Date): The value date to step back from.
    ///
    /// Returns:
    ///     Date: The fixing date.
    fn fixing_date(&self, value_date: &PyDate) -> PyDate {
        PyDate::from_inner(self.inner.fixing_date(value_date.inner()))
    }

    /// Return the maturity of the loan starting on value_date.
    ///
    /// The value date rolled on by the index tenor under the index's own
    /// convention and end-of-month flag.
    ///
    /// Args:
    ///     value_date (Date): The date the loan starts on.
    ///
    /// Returns:
    ///     Date: The maturity date.
    fn maturity_date(&self, value_date: &PyDate) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .maturity_date(value_date.inner())
                .map_err(PyQlError::from)?,
        ))
    }

    /// Return the index tenor, normalized at construction.
    ///
    /// Returns:
    ///     Period: The index tenor.
    fn tenor(&self) -> PyPeriod {
        PyPeriod::from_inner(self.inner.tenor())
    }

    /// Return the day counter the index accrues on.
    ///
    /// Returns:
    ///     DayCounter: The index day count.
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.inner.day_counter().clone())
    }

    /// Return the calendar the fixing and value dates roll on.
    ///
    /// Returns:
    ///     Calendar: The fixing calendar.
    fn fixing_calendar(&self) -> PyCalendar {
        PyCalendar::from_inner(self.inner.fixing_calendar())
    }

    /// Return the convention applied when rolling the value date to maturity.
    ///
    /// Returns:
    ///     BusinessDayConvention: The stored convention.
    fn business_day_convention(&self) -> PyBusinessDayConvention {
        PyBusinessDayConvention::from_inner(self.inner.business_day_convention())
    }

    /// Return whether the maturity roll keeps to month ends.
    ///
    /// Returns:
    ///     bool: True if the roll is end-of-month.
    fn end_of_month(&self) -> bool {
        self.inner.end_of_month()
    }

    /// Return the composed index name, e.g. "Euribor6M Actual/360".
    ///
    /// Returns:
    ///     str: The name the fixings are stored under.
    fn name(&self) -> String {
        self.inner.name()
    }

    /// Return the currency the index is quoted in.
    ///
    /// Returns:
    ///     Currency: The index currency.
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

/// An Ibor index with three separate calendars.
///
/// The general form of what EurLibor configures: fixing dates roll back on the
/// value calendar and adjust Preceding on the fixing calendar, value dates
/// advance on the value calendar, and maturity dates advance on the maturity
/// calendar. Passing the same calendar three times reproduces a plain
/// IborIndex, so this is the escape hatch for a Libor-like index outside the
/// named families. A subclass of IborIndex, so it is accepted wherever the
/// general index is, and the base half carries the three-calendar roll.
#[pyclass(name = "CustomIborIndex", extends = PyIborIndex, unsendable)]
pub struct PyCustomIborIndex {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyCustomIborIndex {
    /// Build a three-calendar Ibor index.
    ///
    /// Args:
    ///     family_name (str): The family name the composed index name is built
    ///         from.
    ///     tenor (Period): The index tenor.
    ///     settlement_days (int): The business days between a fixing and its
    ///         value date.
    ///     currency (Currency): The currency the index is quoted in.
    ///     fixing_calendar (Calendar): The calendar fixing dates are adjusted
    ///         Preceding on.
    ///     value_calendar (Calendar): The calendar value dates are advanced on.
    ///     maturity_calendar (Calendar): The calendar maturity dates are
    ///         advanced on.
    ///     convention (BusinessDayConvention): The convention the roll to
    ///         maturity applies.
    ///     end_of_month (bool): Whether the maturity roll keeps to month ends.
    ///     day_counter (DayCounter): The day counter the index accrues on.
    ///     forwarding (YieldTermStructure | None): The forwarding curve; None
    ///         builds the index over an empty handle, the form the bootstrap
    ///         rate helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
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

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The Euribor IBOR index family.
///
/// A subclass of IborIndex, so a Euribor is accepted wherever the general index
/// is. It retains its own clone of the index the base holds - the same object,
/// not a rebuild - so its own fixing reads exactly what the base reads.
#[pyclass(name = "Euribor", extends = PyIborIndex, unsendable)]
pub struct PyEuribor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyEuribor {
    /// Build a Euribor index of the given tenor.
    ///
    /// Args:
    ///     tenor (Period): The index tenor.
    ///     curve (YieldTermStructure | None): The forwarding curve; None builds
    ///         the index over an empty handle, the form the bootstrap rate
    ///         helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If tenor is a daily tenor, which needs the dedicated
    ///         daily-tenor constructor the core keeps separate.
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

    /// Return the 3-month Euribor index forwarding off curve.
    ///
    /// Args:
    ///     curve (YieldTermStructure): The forwarding curve.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Returns:
    ///     Euribor: The Euribor3M index.
    #[staticmethod]
    fn three_months(
        py: Python<'_>,
        curve: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let index = shared(Euribor::three_months(curve.handle(), settings.inner()));
        Py::new(py, init_euribor(index))
    }

    /// Return the 6-month Euribor index forwarding off curve.
    ///
    /// Args:
    ///     curve (YieldTermStructure): The forwarding curve.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Returns:
    ///     Euribor: The Euribor6M index.
    #[staticmethod]
    fn six_months(
        py: Python<'_>,
        curve: &PyYieldTermStructure,
        settings: &PySettings,
    ) -> PyResult<Py<Self>> {
        let index = shared(Euribor::six_months(curve.handle(), settings.inner()));
        Py::new(py, init_euribor(index))
    }

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The base/subclass initializer shared by the three Euribor constructors: one
/// index object feeds both halves, so the base IborIndex every consumer reads
/// and the Euribor the subclass holds are the same core index.
fn init_euribor(index: Shared<IborIndex>) -> PyClassInitializer<PyEuribor> {
    let base = PyIborIndex {
        inner: Shared::clone(&index),
    };
    PyClassInitializer::from(base).add_subclass(PyEuribor { inner: index })
}

/// The USD Libor index family.
///
/// A subclass of IborIndex, so a USD Libor is accepted wherever the general
/// index is. It retains its own clone of the index the base holds - the same
/// object, not a rebuild - so its own fixing reads exactly what the base reads.
#[pyclass(name = "UsdLibor", extends = PyIborIndex, unsendable)]
pub struct PyUsdLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyUsdLibor {
    /// Build a USD Libor index of the given tenor.
    ///
    /// Args:
    ///     tenor (Period): The index tenor.
    ///     curve (YieldTermStructure | None): The forwarding curve; None builds
    ///         the index over an empty handle, the form the bootstrap rate
    ///         helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If tenor is a daily tenor, which needs a dedicated
    ///         daily-tenor constructor the core has not ported.
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

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The JPY Libor index family.
///
/// A subclass of IborIndex, so a JPY Libor is accepted wherever the general
/// index is. It retains its own clone of the index the base holds - the same
/// object, not a rebuild - so its own fixing reads exactly what the base reads.
#[pyclass(name = "JpyLibor", extends = PyIborIndex, unsendable)]
pub struct PyJpyLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyJpyLibor {
    /// Build a JPY Libor index of the given tenor.
    ///
    /// Args:
    ///     tenor (Period): The index tenor.
    ///     curve (YieldTermStructure | None): The forwarding curve; None builds
    ///         the index over an empty handle, the form the bootstrap rate
    ///         helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If tenor is a daily tenor, which needs a dedicated
    ///         daily-tenor constructor the core has not ported.
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

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The GBP Libor index family.
///
/// A subclass of IborIndex, so a GBP Libor is accepted wherever the general
/// index is. It retains its own clone of the index the base holds - the same
/// object, not a rebuild - so its own fixing reads exactly what the base reads.
#[pyclass(name = "GbpLibor", extends = PyIborIndex, unsendable)]
pub struct PyGbpLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyGbpLibor {
    /// Build a GBP Libor index of the given tenor.
    ///
    /// Args:
    ///     tenor (Period): The index tenor.
    ///     curve (YieldTermStructure | None): The forwarding curve; None builds
    ///         the index over an empty handle, the form the bootstrap rate
    ///         helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If tenor is a daily tenor, which needs a dedicated
    ///         daily-tenor constructor the core has not ported.
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

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// The EUR Libor index family, the Euro ICE Libor fixed in London.
///
/// Three calendars, not one: fixing dates roll on the joint UK-Exchange plus
/// TARGET calendar while value and maturity dates roll on TARGET alone. A
/// subclass of IborIndex, so a EUR Libor is accepted wherever the general index
/// is, and the base half carries the three-calendar roll rather than a
/// single-calendar approximation of it. It retains its own clone of the index
/// the base holds - the same object, not a rebuild - so its own fixing reads
/// exactly what the base reads.
#[pyclass(name = "EurLibor", extends = PyIborIndex, unsendable)]
pub struct PyEurLibor {
    inner: Shared<IborIndex>,
}

#[pymethods]
impl PyEurLibor {
    /// Build a EUR Libor index of the given tenor.
    ///
    /// Args:
    ///     tenor (Period): The index tenor.
    ///     curve (YieldTermStructure | None): The forwarding curve; None builds
    ///         the index over an empty handle, the form the bootstrap rate
    ///         helpers need.
    ///     settings (Settings): The explicit settings supplying the evaluation
    ///         date and the stored fixings.
    ///
    /// Raises:
    ///     ItofinError: If tenor is a daily tenor, which needs a dedicated
    ///         daily-tenor constructor the core has not ported.
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

    /// Return the index fixing for fixing_date.
    ///
    /// Forecast off the forwarding curve for a future date, or read from the
    /// stored fixings for a past one.
    ///
    /// Args:
    ///     fixing_date (Date): The date the fixing is read or forecast for.
    ///     forecast_todays_fixing (bool): Whether a fixing dated today is
    ///         forecast rather than looked up.
    ///
    /// Returns:
    ///     float: The fixing rate.
    ///
    /// Raises:
    ///     ItofinError: If the fixing date is not a valid one, the evaluation
    ///         date is unset, a past fixing is missing from the store, or the
    ///         forwarding handle is empty on a forecast.
    fn fixing(&self, fixing_date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(fixing_date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
}

/// A co-terminal swaption calibration instrument.
///
/// Builds its own European swaption from the maturity, length and index, so no
/// swap or swaption object is needed. The swaption is struck at the forward on
/// shifted-lognormal volatility with zero shift, takes the index's own
/// settlement days, and compounds its averaging.
#[pyclass(name = "SwaptionHelper", unsendable)]
pub struct PySwaptionHelper {
    inner: SharedMut<SwaptionHelper>,
}

#[pymethods]
impl PySwaptionHelper {
    /// Build the helper and the swaption underlying it.
    ///
    /// Args:
    ///     maturity (Period): The option tenor, the time to the swaption expiry.
    ///     length (Period): The tenor of the underlying swap.
    ///     volatility (float): The market volatility, held as a quote.
    ///     index (IborIndex): The index the floating leg fixes on.
    ///     fixed_leg_tenor (Period): The payment tenor of the fixed leg.
    ///     fixed_leg_day_counter (DayCounter): The day count the fixed leg accrues on.
    ///     floating_leg_day_counter (DayCounter): The day count the floating leg accrues on.
    ///     curve (YieldTermStructure): The discount curve.
    ///     error_type (CalibrationErrorType): How the market and model prices are compared.
    ///     nominal (float): The notional of the underlying swap.
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

    /// Return the error between the market and model values.
    ///
    /// Meaningful once a calibration has installed a pricing engine on the
    /// helper; the comparison follows the helper's error type.
    ///
    /// Returns:
    ///     float: The calibration error under the configured error type.
    ///
    /// Raises:
    ///     ItofinError: If the market or model valuation fails, or the implied
    ///         volatility solve does.
    fn calibration_error(&mut self) -> PyResult<f64> {
        Ok(self
            .inner
            .borrow_mut()
            .calibration_error()
            .map_err(PyQlError::from)?)
    }
}
