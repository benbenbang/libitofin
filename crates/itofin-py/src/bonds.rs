//! Fixed and zero-coupon bonds sharing a retained, lazy bond base.

use crate::cashflows::PyLeg;
use crate::curve::PyYieldTermStructure;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, FixedRateBond, ZeroCouponBond};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::date::Date;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_methods_from_python, gen_stub_pyclass, gen_stub_pymethods};
use pyo3_stub_gen::inventory::submit;

fn finite(value: f64) -> PyResult<f64> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(ItofinError::new_err("bond amount must be finite"))
    }
}
fn positive(value: f64) -> PyResult<()> {
    finite(value)?;
    if value <= 0.0 {
        return Err(ItofinError::new_err(
            "face amount and redemption must be positive",
        ));
    }
    Ok(())
}
fn step(date: Date, days: i64) -> PyResult<Date> {
    let serial = i64::from(date.serial_number()) + days;
    if !(i64::from(Date::min_date().serial_number())..=i64::from(Date::max_date().serial_number()))
        .contains(&serial)
    {
        return Err(ItofinError::new_err("bond date outside supported range"));
    }
    Ok(Date::from_serial(serial as i32))
}
fn business(calendar: &PyCalendar, date: Date) -> PyResult<bool> {
    calendar.checked(&PyDate::from_inner(date), "price a bond")?;
    Ok(calendar.inner().is_business_day(date))
}
fn adjust(calendar: &PyCalendar, date: Date, convention: Bdc) -> PyResult<Date> {
    if date == Date::null() {
        return Err(ItofinError::new_err("null bond date"));
    }
    if convention == Bdc::Unadjusted {
        return Ok(date);
    }
    let direction = match convention {
        Bdc::Preceding | Bdc::ModifiedPreceding => -1,
        _ => 1,
    };
    let mut result = date;
    if convention == Bdc::Nearest {
        let mut previous = date;
        while !business(calendar, result)? && !business(calendar, previous)? {
            result = step(result, 1)?;
            previous = step(previous, -1)?;
        }
        return if business(calendar, result)? {
            Ok(result)
        } else {
            Ok(previous)
        };
    }
    while !business(calendar, result)? {
        result = step(result, direction)?;
    }
    if ((convention == Bdc::ModifiedFollowing
        || convention == Bdc::HalfMonthModifiedFollowing
        || convention == Bdc::ModifiedPreceding)
        && result.month() != date.month())
        || (convention == Bdc::HalfMonthModifiedFollowing
            && date.day_of_month() <= 15
            && result.day_of_month() > 15)
    {
        return adjust(
            calendar,
            date,
            if direction > 0 {
                Bdc::Preceding
            } else {
                Bdc::Following
            },
        );
    }
    Ok(result)
}
fn advance(
    calendar: &PyCalendar,
    date: Date,
    n: i64,
    unit: TimeUnit,
    convention: Bdc,
    eom: bool,
) -> PyResult<Date> {
    if n == 0 {
        return adjust(calendar, date, convention);
    }
    if unit == TimeUnit::Days {
        let mut result = date;
        for _ in 0..n.unsigned_abs() {
            result = step(result, n.signum())?;
            while !business(calendar, result)? {
                result = step(result, n.signum())?;
            }
        }
        return Ok(result);
    }
    let shifted = match unit {
        TimeUnit::Weeks => step(
            date,
            n.checked_mul(7)
                .ok_or_else(|| ItofinError::new_err("bond period overflow"))?,
        )?,
        TimeUnit::Months | TimeUnit::Years => {
            let months = if unit == TimeUnit::Years {
                n.checked_mul(12)
            } else {
                Some(n)
            }
            .ok_or_else(|| ItofinError::new_err("bond period overflow"))?;
            let year = (i64::from(date.year()) * 12 + i64::from(date.month().ordinal()) - 1
                + months)
                .div_euclid(12);
            if !(1901..=2199).contains(&year) {
                return Err(ItofinError::new_err("bond period outside supported years"));
            }
            date + Period::new(
                i32::try_from(n).map_err(|_| ItofinError::new_err("bond period overflow"))?,
                unit,
            )
        }
        _ => return Err(ItofinError::new_err("unsupported bond period unit")),
    };
    if eom && (unit == TimeUnit::Months || unit == TimeUnit::Years) {
        if convention == Bdc::Unadjusted {
            if Date::is_end_of_month(date) {
                return Ok(Date::end_of_month(shifted));
            }
        } else if date >= adjust(calendar, Date::end_of_month(date), Bdc::Preceding)? {
            return adjust(calendar, Date::end_of_month(shifted), Bdc::Preceding);
        }
    }
    adjust(calendar, shifted, convention)
}

/// Shared bond interface, constructed only through concrete bond classes.
#[gen_stub_pyclass]
#[pyclass(name = "Bond", subclass, unsendable, module = "itofin.instruments")]
pub struct PyBond {
    inner: SharedMut<Bond>,
    calendar: PyCalendar,
    flags: (bool, Option<bool>),
}
impl PyBond {
    fn from_inner(inner: Bond, calendar: PyCalendar) -> Self {
        let flags = (
            inner.settings().include_reference_date_events(),
            inner.settings().include_todays_cash_flows(),
        );
        Self {
            inner: shared_mut(inner),
            calendar,
            flags,
        }
    }
    fn prepare(&mut self) -> PyResult<()> {
        self.settlement_date(None)?;
        let flags = {
            let bond = self.inner.borrow();
            (
                bond.settings().include_reference_date_events(),
                bond.settings().include_todays_cash_flows(),
            )
        };
        if flags != self.flags {
            self.inner.borrow().base().observer().borrow_mut().update();
            self.flags = flags;
        }
        Ok(())
    }
}
submit! {
    gen_methods_from_python! {
        r#"
        import typing
        class PyBond:
            def __new__(cls, _unconstructible: typing.NoReturn) -> Bond: ...
        "#
    }
}

#[gen_stub_pymethods]
#[pymethods]
impl PyBond {
    #[new]
    #[gen_stub(skip)]
    fn new(_unconstructible: &Bound<'_, PyAny>) -> PyResult<Self> {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "Bond is constructed through a concrete bond class",
        ))
    }
    /// Attach an engine sharing this bond's settings. Dependencies remain live.
    fn set_engine(&mut self, engine: &PyDiscountingBondEngine) -> PyResult<()> {
        if !Shared::ptr_eq(&self.inner.borrow().settings_handle(), &engine.settings) {
            return Err(ItofinError::new_err("bond and engine must share settings"));
        }
        self.inner
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(SharedMut::clone(&engine.inner) as SharedMut<dyn PricingEngine>);
        Ok(())
    }
    /// Present value in currency units, discounted to the curve reference date.
    fn npv(&mut self) -> PyResult<f64> {
        self.prepare()?;
        finite(self.inner.borrow_mut().npv().map_err(PyQlError::from)?)
    }
    /// Dirty currency value at settlement; the today-flow flag can override exclusion.
    fn settlement_value(&mut self) -> PyResult<f64> {
        self.prepare()?;
        finite(
            self.inner
                .borrow_mut()
                .settlement_value()
                .map_err(PyQlError::from)?,
        )
    }
    /// Settlement price per 100 of current notional.
    fn dirty_price(&mut self) -> PyResult<f64> {
        self.prepare()?;
        let notional = self
            .inner
            .borrow()
            .notional(None)
            .map_err(PyQlError::from)?;
        if notional == 0.0 {
            return Ok(0.0);
        }
        let value = self
            .inner
            .borrow_mut()
            .settlement_value()
            .map_err(PyQlError::from)?;
        let scaled = value * 100.0;
        finite(if scaled.is_finite() {
            scaled / notional
        } else {
            value / notional * 100.0
        })
    }
    /// Dirty quote less accrued interest, per 100 of current notional.
    fn clean_price(&mut self) -> PyResult<f64> {
        finite(self.dirty_price()? - self.accrued_amount(None)?)
    }
    /// Accrued interest per 100; omitted date uses settlement.
    #[pyo3(signature = (date=None))]
    fn accrued_amount(&self, date: Option<&PyDate>) -> PyResult<f64> {
        let date = match date {
            Some(date) => {
                if date.inner() == Date::null() {
                    return Err(ItofinError::new_err("null accrual date"));
                }
                date.inner()
            }
            None => self.settlement_date(None)?.inner(),
        };
        finite(
            self.inner
                .borrow()
                .accrued_amount(Some(date))
                .map_err(PyQlError::from)?,
        )
    }
    /// Outstanding notional on date; omitted date uses settlement.
    #[pyo3(signature = (date=None))]
    fn notional(&self, date: Option<&PyDate>) -> PyResult<f64> {
        let date = match date {
            Some(date) => date.inner(),
            None => self.settlement_date(None)?.inner(),
        };
        if date == Date::null() {
            return Err(ItofinError::new_err("null notional date"));
        }
        finite(
            self.inner
                .borrow()
                .notional(Some(date))
                .map_err(PyQlError::from)?,
        )
    }
    /// Calendar-adjusted settlement, floored at the issue date.
    #[pyo3(signature = (date=None))]
    fn settlement_date(&self, date: Option<&PyDate>) -> PyResult<PyDate> {
        let bond = self.inner.borrow();
        let date = date
            .map(PyDate::inner)
            .or_else(|| bond.settings().evaluation_date())
            .ok_or_else(|| ItofinError::new_err("no evaluation date set"))?;
        let settled = advance(
            &self.calendar,
            date,
            i64::from(bond.settlement_days()),
            TimeUnit::Days,
            Bdc::Following,
            false,
        )?;
        Ok(PyDate::from_inner(
            bond.issue_date()
                .map_or(settled, |issue| settled.max(issue)),
        ))
    }
    fn settlement_days(&self) -> u32 {
        self.inner.borrow().settlement_days()
    }
    fn calendar(&self) -> PyCalendar {
        self.calendar.clone()
    }
    fn maturity_date(&self) -> PyResult<PyDate> {
        Ok(PyDate::from_inner(
            self.inner
                .borrow()
                .maturity_date()
                .map_err(PyQlError::from)?,
        ))
    }
    fn issue_date(&self) -> Option<PyDate> {
        self.inner.borrow().issue_date().map(PyDate::from_inner)
    }
    fn cashflows(&self) -> PyLeg {
        PyLeg::from_inner(self.inner.borrow().cashflows().clone())
    }
    fn redemptions(&self) -> PyLeg {
        PyLeg::from_inner(self.inner.borrow().redemptions().clone())
    }
    fn is_expired(&self) -> PyResult<bool> {
        Ok(self.inner.borrow().is_expired().map_err(PyQlError::from)?)
    }
    fn is_calculated(&self) -> bool {
        self.inner.borrow().base().is_calculated()
    }
    /// Pin cached results while unexpired; native expiry can still bypass freeze.
    fn freeze(&self) {
        self.inner.borrow_mut().base_mut().freeze();
    }
    fn unfreeze(&self) {
        self.inner.borrow_mut().base_mut().unfreeze();
    }
    fn recalculate(&mut self) -> PyResult<()> {
        self.prepare()?;
        self.inner
            .borrow_mut()
            .recalculate()
            .map_err(PyQlError::from)?;
        Ok(())
    }
}

/// Fixed coupon bond, retaining the common Bond valuation interface.
#[gen_stub_pyclass]
#[pyclass(name = "FixedRateBond", extends=PyBond, unsendable, module = "itofin.instruments")]
pub struct PyFixedRateBond {
    frequency: PyFrequency,
    day_counter: PyDayCounter,
    first_period_day_counter: Option<PyDayCounter>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyFixedRateBond {
    #[gen_stub(override_return_type(type_repr = "FixedRateBond"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, face_amount, schedule, coupons, accrual_day_counter, settings, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None, payment_calendar=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false, first_period_day_counter=None))]
    fn new(
        settlement_days: u32,
        face_amount: f64,
        schedule: &PySchedule,
        coupons: Vec<f64>,
        accrual_day_counter: &PyDayCounter,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
        payment_calendar: Option<&PyCalendar>,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_convention: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
        first_period_day_counter: Option<&PyDayCounter>,
    ) -> PyResult<PyClassInitializer<Self>> {
        if settlement_days > (Date::max_date() - Date::min_date()) as u32 {
            return Err(ItofinError::new_err(
                "settlement days exceed supported date range",
            ));
        }
        positive(face_amount)?;
        positive(redemption)?;
        positive(face_amount * redemption / 100.0)?;
        for &coupon in &coupons {
            finite(coupon)?;
        }
        let retained_calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        let calendar = payment_calendar
            .cloned()
            .unwrap_or_else(|| retained_calendar.clone());
        let ex_calendar = ex_coupon_calendar.cloned().unwrap_or_else(|| {
            PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new())
        });
        if issue_date.is_some_and(|date| date.inner() == Date::null()) {
            return Err(ItofinError::new_err("null issue date"));
        }
        for &date in schedule.dates().iter().skip(1) {
            let payment = adjust(&calendar, date, payment_convention.inner())?;
            if let Some(period) = ex_coupon_period {
                let period = period.inner();
                advance(
                    &ex_calendar,
                    payment,
                    -i64::from(period.length()),
                    period.units(),
                    ex_coupon_convention.inner(),
                    ex_coupon_end_of_month,
                )?;
            }
        }
        if schedule.has_tenor() && schedule.has_is_regular() && schedule.has_end_of_month() {
            let tenor = schedule.tenor();
            if !schedule.is_regular_at(1) {
                advance(
                    &retained_calendar,
                    schedule.date(1),
                    -i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
            if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &retained_calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
        }
        let bond = FixedRateBond::new(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            accrual_day_counter.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            Some(calendar.inner()),
            ex_coupon_period.map(PyPeriod::inner),
            ex_calendar.inner(),
            ex_coupon_convention.inner(),
            ex_coupon_end_of_month,
            first_period_day_counter.map(PyDayCounter::inner),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        for flow in bond.bond().cashflows() {
            finite(flow.amount().map_err(PyQlError::from)?)?;
        }
        let typed = Self {
            frequency: PyFrequency::from_inner(bond.frequency())?,
            day_counter: PyDayCounter::from_inner(bond.day_counter().clone()),
            first_period_day_counter: bond
                .first_period_day_counter()
                .map(|dc| PyDayCounter::from_inner(dc.clone())),
        };
        Ok(
            PyClassInitializer::from(PyBond::from_inner(bond.into_bond(), calendar))
                .add_subclass(typed),
        )
    }
    fn frequency(&self) -> PyFrequency {
        self.frequency
    }
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.day_counter.inner())
    }
    fn first_period_day_counter(&self) -> Option<PyDayCounter> {
        self.first_period_day_counter
            .as_ref()
            .map(|dc| PyDayCounter::from_inner(dc.inner()))
    }
}

/// One redemption on an adjusted payment date, with contractual maturity intact.
#[gen_stub_pyclass]
#[pyclass(name="ZeroCouponBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyZeroCouponBond;
#[gen_stub_pymethods]
#[pymethods]
impl PyZeroCouponBond {
    #[gen_stub(override_return_type(type_repr = "ZeroCouponBond"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, calendar, face_amount, maturity_date, settings, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None))]
    fn new(
        settlement_days: u32,
        calendar: &PyCalendar,
        face_amount: f64,
        maturity_date: &PyDate,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        if settlement_days > (Date::max_date() - Date::min_date()) as u32 {
            return Err(ItofinError::new_err(
                "settlement days exceed supported date range",
            ));
        }
        positive(face_amount)?;
        positive(redemption)?;
        positive(face_amount * redemption / 100.0)?;
        if issue_date.is_some_and(|date| date.inner() == Date::null()) {
            return Err(ItofinError::new_err("null issue date"));
        }
        adjust(calendar, maturity_date.inner(), payment_convention.inner())?;
        let bond = ZeroCouponBond::new(
            settlement_days,
            calendar.inner(),
            face_amount,
            maturity_date.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(
            PyClassInitializer::from(PyBond::from_inner(bond.into_bond(), calendar.clone()))
                .add_subclass(Self),
        )
    }
}

/// Observable discounting engine, with live curve and explicit retained settings.
#[gen_stub_pyclass]
#[pyclass(
    name = "DiscountingBondEngine",
    unsendable,
    module = "itofin.pricingengines"
)]
pub struct PyDiscountingBondEngine {
    inner: SharedMut<DiscountingBondEngine>,
    settings: Shared<Settings<Date>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyDiscountingBondEngine {
    #[new]
    #[pyo3(signature=(discount_curve, settings, include_settlement_date_flows=None))]
    fn new(
        discount_curve: &PyYieldTermStructure,
        settings: &PySettings,
        include_settlement_date_flows: Option<bool>,
    ) -> Self {
        Self {
            inner: shared_mut(DiscountingBondEngine::new(
                discount_curve.handle(),
                include_settlement_date_flows,
                settings.inner(),
            )),
            settings: settings.inner(),
        }
    }
}
