//! Municipal-rate index, coupon, swap and bootstrap facades.
use crate::curve::PyYieldTermStructure;
use crate::helpers::PyRateHelper;
use crate::hullwhite::PyIborIndex;
use crate::market::PySimpleQuote;
use crate::settings::PySettings;
use crate::swap::PySwapType;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::cashflows::{AverageBMACoupon, Coupon};
use libitofin::handle::Handle;
use libitofin::indexes::{BMAIndex, Index, InterestRateIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::BMASwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::yields::BMASwapRateHelper;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Weekly SIFMA municipal index retaining its curve and settings history.
#[gen_stub_pyclass]
#[pyclass(name = "BMAIndex", unsendable, module = "itofin.indexes")]
pub struct PyBMAIndex {
    inner: Shared<BMAIndex>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyBMAIndex {
    /// Create an index with a retained curve and settings history.
    #[new]
    fn new(forwarding: Option<&PyYieldTermStructure>, settings: &PySettings) -> Self {
        Self {
            inner: shared(BMAIndex::new(
                forwarding.map(|v| v.handle()).unwrap_or_else(Handle::empty),
                settings.inner(),
            )),
        }
    }
    /// Calendar used for fixing, value-date and maturity conventions.
    fn fixing_calendar(&self) -> PyCalendar {
        PyCalendar::from_inner(self.inner.fixing_calendar())
    }
    /// Remove this index history and invalidate all retained consumers.
    fn clear_fixings(&self) {
        self.inner.clear_fixings();
    }
    /// Stored fixing without forecasting; None denotes absent history.
    fn past_fixing(&self, date: &PyDate) -> PyResult<Option<f64>> {
        Ok(self
            .inner
            .past_fixing(date.inner())
            .map_err(PyQlError::from)?)
    }
    /// Whether this settings history contains the date.
    fn has_historical_fixing(&self, date: &PyDate) -> bool {
        self.inner.has_historical_fixing(date.inner())
    }
    /// Store a valid weekly fixing in the retained settings history.
    fn add_fixing(&self, date: &PyDate, value: f64) -> PyResult<()> {
        if !value.is_finite() {
            return Err(ItofinError::new_err("fixing must be finite"));
        }
        Ok(self
            .inner
            .add_fixing(date.inner(), value)
            .map_err(PyQlError::from)?)
    }
    /// Read a historical fixing or forecast a future fixing.
    #[pyo3(signature=(date, forecast_todays_fixing=false))]
    fn fixing(&self, date: &PyDate, forecast_todays_fixing: bool) -> PyResult<f64> {
        Ok(self
            .inner
            .fixing(date.inner(), forecast_todays_fixing)
            .map_err(PyQlError::from)?)
    }
    /// Whether the date is the week's valid municipal fixing day.
    fn is_valid_fixing_date(&self, date: &PyDate) -> bool {
        self.inner.is_valid_fixing_date(date.inner())
    }
    /// First value date for the given weekly fixing.
    fn value_date(&self, date: &PyDate) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .value_date(date.inner())
                .map_err(PyQlError::from)?,
        ))
    }
    /// Maturity of the weekly rate starting on the value date.
    fn maturity_date(&self, date: &PyDate) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .maturity_date(date.inner())
                .map_err(PyQlError::from)?,
        ))
    }
    /// Copy of fixing dates bracketing both dates.
    fn fixing_schedule(&self, start: &PyDate, end: &PyDate) -> PyResult<Vec<PyDate>> {
        Ok(self
            .inner
            .fixing_schedule(start.inner(), end.inner())
            .map_err(PyQlError::from)?
            .dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect())
    }
}

/// Calendar-day weighted municipal coupon with its arithmetic-average pricer.
#[gen_stub_pyclass]
#[pyclass(name = "AverageBMACoupon", unsendable, module = "itofin.cashflows")]
pub struct PyAverageBMACoupon {
    inner: AverageBMACoupon,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyAverageBMACoupon {
    /// Create a calendar-day averaged coupon with explicit accrual conventions.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(payment_date, nominal, start_date, end_date, index, day_counter, gearing=1.0, spread=0.0, reference_start=None, reference_end=None))]
    fn new(
        payment_date: &PyDate,
        nominal: f64,
        start_date: &PyDate,
        end_date: &PyDate,
        index: &PyBMAIndex,
        day_counter: &PyDayCounter,
        gearing: f64,
        spread: f64,
        reference_start: Option<&PyDate>,
        reference_end: Option<&PyDate>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: AverageBMACoupon::new(
                payment_date.inner(),
                nominal,
                start_date.inner(),
                end_date.inner(),
                index.inner.clone(),
                gearing,
                spread,
                reference_start.map(PyDate::inner),
                reference_end.map(PyDate::inner),
                day_counter.inner(),
            )
            .map_err(PyQlError::from)?,
        })
    }
    /// Weighted rate after gearing and spread.
    fn rate(&self) -> PyResult<f64> {
        Ok(self.inner.rate().map_err(PyQlError::from)?)
    }
    /// Undiscounted coupon payment.
    fn amount(&self) -> PyResult<f64> {
        Ok(self.inner.amount().map_err(PyQlError::from)?)
    }
    /// Coupon accrual year fraction, including explicit reference dates.
    fn accrual_period(&self) -> f64 {
        self.inner.accrual_period()
    }
    /// Copy of weekly fixing boundaries.
    fn fixing_dates(&self) -> Vec<PyDate> {
        self.inner
            .fixing_dates()
            .iter()
            .copied()
            .map(PyDate::from_inner)
            .collect()
    }
}

/// Municipal swap: payer pays BMA and receives the specified fraction of Ibor.
#[gen_stub_pyclass]
#[pyclass(name = "BMASwap", unsendable, module = "itofin.instruments")]
pub struct PyBMASwap {
    inner: BMASwap,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyBMASwap {
    /// Create a municipal swap retaining both schedules, indexes and settings.
    #[new]
    #[allow(clippy::too_many_arguments)]
    fn new(
        swap_type: &PySwapType,
        nominal: f64,
        libor_schedule: &PySchedule,
        libor_fraction: f64,
        libor_spread: f64,
        libor_index: &PyIborIndex,
        libor_day_counter: &PyDayCounter,
        bma_schedule: &PySchedule,
        bma_index: &PyBMAIndex,
        bma_day_counter: &PyDayCounter,
        settings: &PySettings,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: BMASwap::new(
                swap_type.inner(),
                nominal,
                libor_schedule.inner(),
                libor_fraction,
                libor_spread,
                libor_index.inner(),
                libor_day_counter.inner(),
                bma_schedule.inner(),
                bma_index.inner.clone(),
                bma_day_counter.inner(),
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        })
    }
    /// Attach a retained discount curve using settings-driven engine defaults.
    fn set_engine(&mut self, curve: &PyYieldTermStructure, settings: &PySettings) {
        self.inner
            .base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                curve.handle(),
                None,
                None,
                None,
                settings.inner(),
            )));
    }
    /// Present value of both signed legs.
    fn npv(&mut self) -> PyResult<f64> {
        Ok(self.inner.npv().map_err(PyQlError::from)?)
    }
    /// Fraction of Ibor that makes the swap's value zero.
    fn fair_libor_fraction(&mut self) -> PyResult<f64> {
        Ok(self.inner.fair_libor_fraction().map_err(PyQlError::from)?)
    }
    /// Ibor spread that makes the swap's value zero.
    fn fair_libor_spread(&mut self) -> PyResult<f64> {
        Ok(self.inner.fair_libor_spread().map_err(PyQlError::from)?)
    }
    /// Signed leg value: zero selects Ibor, one selects BMA.
    fn leg_npv(&mut self, leg: usize) -> PyResult<f64> {
        Ok(self
            .inner
            .swap_mut()
            .leg_npv(leg)
            .map_err(PyQlError::from)?)
    }
    /// Signed basis-point value: zero selects Ibor, one selects BMA.
    fn leg_bps(&mut self, leg: usize) -> PyResult<f64> {
        Ok(self
            .inner
            .swap_mut()
            .leg_bps(leg)
            .map_err(PyQlError::from)?)
    }
    /// Whether cached pricing results are currently valid.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }
}

/// Bootstrap helper quoting the fair Ibor fraction of a municipal swap.
#[gen_stub_pyclass]
#[pyclass(name="BMASwapRateHelper", extends=PyRateHelper, unsendable, module="itofin.termstructures")]
pub struct PyBMASwapRateHelper;
#[gen_stub_pymethods]
#[pymethods]
impl PyBMASwapRateHelper {
    /// Create a helper quoting the municipal swap's fair Ibor fraction.
    #[new]
    #[gen_stub(override_return_type(type_repr = "BMASwapRateHelper"))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        quote: &PySimpleQuote,
        tenor: &PyPeriod,
        settlement_days: u32,
        calendar: &PyCalendar,
        bma_period: &PyPeriod,
        bma_convention: &PyBusinessDayConvention,
        bma_day_counter: &PyDayCounter,
        bma_index: &PyBMAIndex,
        libor_index: &PyIborIndex,
    ) -> PyResult<PyClassInitializer<Self>> {
        let helper = BMASwapRateHelper::new(
            quote.handle(),
            tenor.inner(),
            settlement_days,
            calendar.inner(),
            bma_period.inner(),
            bma_convention.inner(),
            bma_day_counter.inner(),
            &bma_index.inner,
            &libor_index.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(PyClassInitializer::from(PyRateHelper::from_inner(helper)).add_subclass(Self))
    }
}
