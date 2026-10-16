//! Typed plain common-notional two-Ibor swap facade.
use crate::bonds::{adjust, advance, step};
use crate::curve::PyYieldTermStructure;
use crate::hullwhite::PyIborIndex;
use crate::settings::PySettings;
use crate::time::{PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PySchedule};
use crate::{ItofinError, PyQlError};
use libitofin::handle::Handle;
use libitofin::indexes::{IborIndex, Index, InterestRateIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::FloatFloatSwap;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared_mut};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as Bdc, date::Date, schedule::Schedule,
    timeunit::TimeUnit,
};
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
struct LegGuard {
    index: Shared<IborIndex>,
    calendars: [PyCalendar; 3],
}
impl LegGuard {
    fn check(&self, start: Date, end: Date, settings: &Settings<Date>) -> PyResult<()> {
        let [fixing_calendar, value_calendar, maturity_calendar] = &self.calendars;
        let days = i64::from(self.index.fixing_days());
        let fixing = advance(
            fixing_calendar,
            start,
            -days,
            TimeUnit::Days,
            Bdc::Preceding,
            false,
        )?;
        fixing_calendar.checked(&PyDate::from_inner(fixing), "value a float-float coupon")?;
        let Some(today) = settings.evaluation_date() else {
            return Ok(());
        };
        if fixing < today
            || (fixing == today
                && (settings.enforces_todays_historic_fixings()
                    || self.index.has_historical_fixing(fixing)))
        {
            return Ok(());
        }
        let value = advance(
            fixing_calendar,
            fixing,
            days,
            TimeUnit::Days,
            Bdc::Following,
            false,
        )?;
        let tenor = self.index.tenor();
        let maturity = |value| {
            advance(
                maturity_calendar,
                value,
                i64::from(tenor.length()),
                tenor.units(),
                self.index.business_day_convention(),
                self.index.end_of_month(),
            )
        };
        maturity(value)?;
        let natural = advance(
            value_calendar,
            fixing,
            days,
            TimeUnit::Days,
            Bdc::Following,
            false,
        )?;
        maturity(adjust(maturity_calendar, natural, Bdc::Following)?)?;
        if settings.using_at_par_coupons() {
            let next = advance(
                fixing_calendar,
                end,
                -days,
                TimeUnit::Days,
                Bdc::Following,
                false,
            )?;
            advance(
                fixing_calendar,
                next,
                days,
                TimeUnit::Days,
                Bdc::Following,
                false,
            )?;
            step(value, 1)?;
        }
        Ok(())
    }
    fn construction(
        &self,
        schedule: &Schedule,
        calendar: &PyCalendar,
        payment: Option<Bdc>,
    ) -> PyResult<()> {
        for pair in schedule.dates().windows(2) {
            adjust(
                calendar,
                pair[1],
                payment.unwrap_or_else(|| schedule.business_day_convention()),
            )?;
            advance(
                &self.calendars[0],
                pair[0],
                -i64::from(self.index.fixing_days()),
                TimeUnit::Days,
                Bdc::Preceding,
                false,
            )?;
        }
        if schedule.len() >= 2 && schedule.has_tenor() && schedule.has_is_regular() {
            let tenor = schedule.tenor();
            for (position, anchor, amount) in [
                (1, schedule.date(1), -tenor.length()),
                (
                    schedule.len() - 1,
                    schedule.date(schedule.len() - 2),
                    tenor.length(),
                ),
            ] {
                if !schedule.is_regular_at(position) {
                    advance(
                        calendar,
                        anchor,
                        i64::from(amount),
                        tenor.units(),
                        schedule.business_day_convention(),
                        false,
                    )?;
                }
            }
        }
        Ok(())
    }
}
/// Plain same-currency swap paying the first Ibor leg and receiving the second.
/// Both legs share one positive constant nominal; no principal is exchanged.
#[gen_stub_pyclass]
#[pyclass(name = "FloatFloatSwap", unsendable, module = "itofin.instruments")]
pub struct PyFloatFloatSwap {
    inner: FloatFloatSwap,
    settings: Shared<Settings<Date>>,
    guards: [LegGuard; 2],
    _schedules: [Schedule; 2],
    _calendars: [PyCalendar; 2],
    discount: Option<Handle<dyn YieldTermStructure>>,
    frozen: bool,
    flags: Flags,
}
impl PyFloatFloatSwap {
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
        for (leg, guard) in self.inner.swap().legs().iter().zip(&self.guards) {
            for flow in leg {
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
                    guard.check(
                        coupon.accrual_start_date(),
                        coupon.accrual_end_date(),
                        &self.settings,
                    )?;
                }
            }
        }
        Ok(())
    }
    fn value(&mut self, query: u8) -> PyResult<f64> {
        self.prepare(false)?;
        let value = match query {
            0 => self.inner.npv(),
            1 => self.inner.pay_leg_npv(),
            2 => self.inner.receive_leg_npv(),
            3 => self.inner.pay_leg_bps(),
            4 => self.inner.receive_leg_bps(),
            5 => self.inner.fair_pay_spread(),
            _ => self.inner.fair_receive_spread(),
        }
        .map_err(PyQlError::from)?;
        self.flags = flags(&self.settings);
        Ok(value)
    }
}
#[gen_stub_pymethods]
#[pymethods]
impl PyFloatFloatSwap {
    /// Build plain Ibor legs; omitted payment convention uses each schedule's convention.
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(nominal,pay_schedule,pay_index,pay_spread,pay_day_count,receive_schedule,receive_index,receive_spread,receive_day_count,settings,*,payment_convention=None))]
    fn new(
        nominal: f64,
        pay_schedule: &PySchedule,
        pay_index: &PyIborIndex,
        pay_spread: f64,
        pay_day_count: &PyDayCounter,
        receive_schedule: &PySchedule,
        receive_index: &PyIborIndex,
        receive_spread: f64,
        receive_day_count: &PyDayCounter,
        settings: &PySettings,
        payment_convention: Option<&PyBusinessDayConvention>,
    ) -> PyResult<Self> {
        let settings = settings.inner();
        let guards = [
            LegGuard {
                index: pay_index.inner(),
                calendars: pay_index.retained_calendars(),
            },
            LegGuard {
                index: receive_index.inner(),
                calendars: receive_index.retained_calendars(),
            },
        ];
        let schedules = [pay_schedule.inner(), receive_schedule.inner()];
        let payment = payment_convention.map(PyBusinessDayConvention::inner);
        let calendars = [
            pay_schedule.retained_calendar(),
            receive_schedule.retained_calendar(),
        ];
        for ((guard, schedule), calendar) in guards.iter().zip(&schedules).zip(&calendars) {
            if !std::ptr::eq(Index::settings(&*guard.index), &*settings) {
                return Err(ItofinError::new_err("indexes must share swap settings"));
            }
            guard.construction(schedule, calendar, payment)?;
        }
        let inner = FloatFloatSwap::new(
            nominal,
            schedules[0].clone(),
            guards[0].index.clone(),
            pay_spread,
            pay_day_count.inner(),
            schedules[1].clone(),
            guards[1].index.clone(),
            receive_spread,
            receive_day_count.inner(),
            payment,
            settings.clone(),
        )
        .map_err(PyQlError::from)?;
        Ok(Self {
            inner,
            flags: flags(&settings),
            settings,
            guards,
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
}
