//! CPI-scaled fixed coupons versus plain Ibor facade.
use crate::curve::PyYieldTermStructure;
use crate::float_float_swap::LegGuard;
use crate::hullwhite::PyIborIndex;
use crate::inflation::{PyCpiInterpolationType, PyZeroInflationIndex};
use crate::settings::PySettings;
use crate::swap::PySwapType;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDayCounter, PyPeriod, PySchedule};
use crate::{ItofinError, PyQlError};
use libitofin::handle::Handle;
use libitofin::indexes::Index;
use libitofin::instrument::Instrument;
use libitofin::instruments::CpiSwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{date::Date, schedule::Schedule};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

type Flags = (bool, Option<bool>, bool, bool);
fn flags(s: &Settings<Date>) -> Flags {
    (
        s.include_reference_date_events(),
        s.include_todays_cash_flows(),
        s.enforces_todays_historic_fixings(),
        s.using_at_par_coupons(),
    )
}
/// CPI-scaled fixed coupons against plain Ibor-plus-spread coupons.
/// Payer pays Ibor and receives CPI; Receiver reverses both signs.
#[gen_stub_pyclass]
#[pyclass(name = "CPISwap", unsendable, module = "itofin.instruments")]
pub struct PyCpiSwap {
    inner: CpiSwap,
    settings: Shared<Settings<Date>>,
    guard: LegGuard,
    _schedules: [Schedule; 2],
    _calendars: [PyCalendar; 2],
    discount: Option<Handle<dyn YieldTermStructure>>,
    frozen: bool,
    flags: Flags,
}
impl PyCpiSwap {
    fn prepare(&self, force: bool) -> PyResult<()> {
        if !force
            && (self.frozen
                || (self.inner.base().is_calculated() && self.flags == flags(&self.settings)))
        {
            return Ok(());
        }
        if self.inner.is_expired().map_err(PyQlError::from)? {
            return Ok(());
        }
        let Some(discount) = &self.discount else {
            return Ok(());
        };
        let settlement = discount
            .current_link()
            .map_err(PyQlError::from)?
            .reference_date()
            .map_err(PyQlError::from)?;
        for flow in self.inner.float_leg() {
            if flow
                .has_occurred(
                    &self.settings,
                    Some(settlement),
                    Some(self.settings.include_reference_date_events()),
                )
                .map_err(PyQlError::from)?
            {
                continue;
            }
            if let Some(coupon) = flow.as_coupon() {
                self.guard.check(
                    coupon.accrual_start_date(),
                    coupon.accrual_end_date(),
                    &self.settings,
                )?;
            }
        }
        Ok(())
    }
    fn value(&mut self, query: u8) -> PyResult<f64> {
        self.prepare(false)?;
        let value = match query {
            0 => self.inner.npv(),
            1 => self.inner.fixed_leg_npv(),
            2 => self.inner.float_leg_npv(),
            3 => self.inner.float_leg_bps(),
            _ => self.inner.fair_spread(),
        }
        .map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        Ok(value)
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiSwap {
    /// Construct plain coupon schedules and terminal exchanges, retaining live indexes.
    /// Ibor fixing days use the index default. Payer pays the floating leg.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(swap_type,nominal,subtract_inflation_nominal,spread,float_schedule,float_index,float_day_count,float_convention,fixed_rate,base_cpi,fixed_schedule,fixed_index,fixed_day_count,fixed_convention,observation_lag,settings,*,interpolation=None,inflation_nominal=None))]
    fn new(
        swap_type: &PySwapType,
        nominal: f64,
        subtract_inflation_nominal: bool,
        spread: f64,
        float_schedule: &PySchedule,
        float_index: &PyIborIndex,
        float_day_count: &PyDayCounter,
        float_convention: &PyBusinessDayConvention,
        fixed_rate: f64,
        base_cpi: f64,
        fixed_schedule: &PySchedule,
        fixed_index: &PyZeroInflationIndex,
        fixed_day_count: &PyDayCounter,
        fixed_convention: &PyBusinessDayConvention,
        observation_lag: &PyPeriod,
        settings: &PySettings,
        interpolation: Option<&PyCpiInterpolationType>,
        inflation_nominal: Option<f64>,
    ) -> PyResult<Self> {
        let settings = settings.inner();
        let guard = LegGuard {
            index: float_index.inner(),
            calendars: float_index.retained_calendars(),
        };
        let fixed_index = fixed_index.shared();
        if !std::ptr::eq(Index::settings(&*guard.index), &*settings)
            || !std::ptr::eq(Index::settings(&*fixed_index), &*settings)
        {
            return Err(ItofinError::new_err("indexes must share swap settings"));
        }
        let schedules = [fixed_schedule.inner(), float_schedule.inner()];
        let calendars = [
            fixed_schedule.retained_calendar(),
            float_schedule.retained_calendar(),
        ];
        let float_roll = float_convention.inner();
        let fixed_roll = fixed_convention.inner();
        guard.construction(&schedules[1], &calendars[1], Some(float_roll))?;
        crate::cpi_cashflows::check_leg_dates(
            &schedules[0],
            &calendars[0],
            &calendars[0],
            fixed_roll,
        )?;
        let inner = CpiSwap::new(
            swap_type.inner(),
            nominal,
            inflation_nominal.unwrap_or(nominal),
            subtract_inflation_nominal,
            spread,
            float_day_count.inner(),
            schedules[1].clone(),
            float_roll,
            guard.index.clone(),
            fixed_rate,
            base_cpi,
            fixed_day_count.inner(),
            schedules[0].clone(),
            fixed_roll,
            observation_lag.inner(),
            fixed_index,
            interpolation.map_or(
                libitofin::indexes::inflationindex::CpiInterpolationType::Flat,
                PyCpiInterpolationType::inner,
            ),
            settings.clone(),
        )
        .map_err(PyQlError::from)?;
        Ok(Self {
            inner,
            flags: flags(&settings),
            settings,
            guard,
            _schedules: schedules,
            _calendars: calendars,
            discount: None,
            frozen: false,
        })
    }
    /// Attach a retained live curve with native defaults; Settings must be the same object.
    fn set_engine(&mut self, curve: &PyYieldTermStructure, settings: &PySettings) -> PyResult<()> {
        let settings = settings.inner();
        if !Shared::ptr_eq(&self.settings, &settings) {
            return Err(ItofinError::new_err("engine must share swap settings"));
        }
        let discount = curve.handle();
        self.inner
            .base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                discount.clone(),
                None,
                None,
                None,
                settings,
            )));
        self.discount = Some(discount);
        Ok(())
    }
    /// Signed present value under the retained engine.
    fn npv(&mut self) -> PyResult<f64> {
        self.value(0)
    }
    /// Signed CPI-leg present value including indexed principal.
    fn fixed_leg_npv(&mut self) -> PyResult<f64> {
        self.value(1)
    }
    /// Signed Ibor-leg present value including any nominal exchange.
    fn float_leg_npv(&mut self) -> PyResult<f64> {
        self.value(2)
    }
    /// Signed sensitivity to a one-basis-point Ibor spread increase.
    fn float_leg_bps(&mut self) -> PyResult<f64> {
        self.value(3)
    }
    /// Absolute Ibor spread that zeroes NPV with the CPI leg unchanged.
    fn fair_spread(&mut self) -> PyResult<f64> {
        self.value(4)
    }
    /// Populate native cached results if needed.
    fn calculate(&mut self) -> PyResult<()> {
        self.prepare(false)?;
        self.inner.calculate().map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        Ok(())
    }
    /// Force native repricing, including while frozen.
    fn recalculate(&mut self) -> PyResult<()> {
        self.prepare(true)?;
        self.inner.recalculate().map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        Ok(())
    }
    /// Pin native cached results until unfreeze or explicit recalculate.
    fn freeze(&mut self) {
        self.inner.base_mut().freeze();
        self.frozen = true;
    }
    /// Resume native observer-driven repricing.
    fn unfreeze(&mut self) {
        self.inner.base_mut().unfreeze();
        self.frozen = false;
    }
    /// Read the native cache flag without calculating.
    fn is_calculated(&self) -> bool {
        self.inner.base().is_calculated()
    }
    /// Test native expiry without calculating.
    fn is_expired(&self) -> PyResult<bool> {
        self.inner
            .is_expired()
            .map_err(PyQlError::from)
            .map_err(Into::into)
    }
}
