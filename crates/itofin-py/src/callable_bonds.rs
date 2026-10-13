//! Retained hard call/put schedules and native callable bond owners.

use crate::bonds::{BondBacking, PyBond, adjust, advance, finite, positive};
use crate::helpers::PyBondPriceType;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::instruments::{
    BondPrice, Callability, CallabilityType, CallableFixedRateBond, CallableZeroCouponBond,
};
use libitofin::shared::shared_mut;
use libitofin::time::date::Date;
use libitofin::time::frequency::Frequency;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pyclass_enum, gen_stub_pymethods};

/// Issuer redemption right or holder sale right.
#[gen_stub_pyclass_enum]
#[pyclass(
    name = "CallabilityType",
    eq,
    eq_int,
    from_py_object,
    module = "itofin.instruments"
)]
#[derive(Clone, Copy, PartialEq)]
pub enum PyCallabilityType {
    Call = 0,
    Put = 1,
}
impl PyCallabilityType {
    fn inner(self) -> CallabilityType {
        match self {
            Self::Call => CallabilityType::Call,
            Self::Put => CallabilityType::Put,
        }
    }
}

/// Immutable hard exercise right with a clean or dirty per-100 price.
#[gen_stub_pyclass]
#[pyclass(name = "Callability", from_py_object, module = "itofin.instruments")]
#[derive(Clone)]
pub struct PyCallability {
    inner: Callability,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCallability {
    #[new]
    #[pyo3(signature=(price, call_type, date, price_type=PyBondPriceType::Clean))]
    fn new(
        price: f64,
        call_type: PyCallabilityType,
        date: &PyDate,
        price_type: PyBondPriceType,
    ) -> PyResult<Self> {
        let price = match price_type {
            PyBondPriceType::Clean => BondPrice::Clean(price),
            PyBondPriceType::Dirty => BondPrice::Dirty(price),
        };
        Ok(Self {
            inner: Callability::new(price, call_type.inner(), date.inner())
                .map_err(PyQlError::from)?,
        })
    }
    fn price(&self) -> f64 {
        self.inner.price().amount()
    }
    fn price_type(&self) -> PyBondPriceType {
        match self.inner.price() {
            BondPrice::Clean(_) => PyBondPriceType::Clean,
            BondPrice::Dirty(_) => PyBondPriceType::Dirty,
        }
    }
    fn call_type(&self) -> PyCallabilityType {
        match self.inner.call_type() {
            CallabilityType::Call => PyCallabilityType::Call,
            CallabilityType::Put => PyCallabilityType::Put,
        }
    }
    fn date(&self) -> PyDate {
        PyDate::from_inner(self.inner.date())
    }
}
fn amounts(
    settlement_days: u32,
    face_amount: f64,
    redemption: f64,
    issue: Option<&PyDate>,
) -> PyResult<()> {
    if settlement_days > (Date::max_date() - Date::min_date()) as u32 {
        return Err(ItofinError::new_err(
            "settlement days exceed supported date range",
        ));
    }
    positive(face_amount)?;
    finite(redemption)?;
    finite(face_amount * redemption / 100.0)?;
    if redemption < 0.0 {
        return Err(ItofinError::new_err("redemption must be nonnegative"));
    }
    if issue.is_some_and(|date| date.inner() == Date::null()) {
        return Err(ItofinError::new_err("null issue date"));
    }
    Ok(())
}

/// Callable fixed bond sharing its actual native lazy base with Bond methods.
#[gen_stub_pyclass]
#[pyclass(name="CallableFixedRateBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyCallableFixedRateBond {
    inner: libitofin::shared::SharedMut<CallableFixedRateBond>,
    frequency: Frequency,
    day_counter: PyDayCounter,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCallableFixedRateBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "CallableFixedRateBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, face_amount, schedule, coupons, accrual_day_counter, callability, settings, *, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false))]
    fn new(
        settlement_days: u32,
        face_amount: f64,
        schedule: &PySchedule,
        coupons: Vec<f64>,
        accrual_day_counter: &PyDayCounter,
        callability: Vec<PyCallability>,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_convention: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
    ) -> PyResult<PyClassInitializer<Self>> {
        amounts(settlement_days, face_amount, redemption, issue_date)?;
        if ex_coupon_period.is_some_and(|period| period.inner().length() < 0) {
            return Err(ItofinError::new_err("ex-coupon period must be nonnegative"));
        }
        for &coupon in &coupons {
            finite(coupon)?;
        }
        let calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        let ex_calendar = ex_coupon_calendar.cloned().unwrap_or_else(|| {
            PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new())
        });
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
                    &calendar,
                    schedule.date(1),
                    -i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
            if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.end_of_month(),
                )?;
            }
        }
        let frequency = if schedule.has_tenor() {
            schedule.tenor().frequency()
        } else {
            Frequency::NoFrequency
        };
        let bond = CallableFixedRateBond::with_ex_coupon(
            settlement_days,
            face_amount,
            schedule,
            coupons,
            accrual_day_counter.inner(),
            payment_convention.inner(),
            redemption,
            issue_date.map(PyDate::inner),
            callability.into_iter().map(|call| call.inner).collect(),
            ex_coupon_period.map(PyPeriod::inner),
            ex_calendar.inner(),
            ex_coupon_convention.inner(),
            ex_coupon_end_of_month,
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        for flow in bond.bond().cashflows() {
            finite(flow.amount().map_err(PyQlError::from)?)?;
        }
        let inner = shared_mut(bond);
        Ok(PyClassInitializer::from(PyBond::from_backing(
            BondBacking::CallableFixed(inner.clone()),
            calendar,
        ))
        .add_subclass(Self {
            inner,
            frequency,
            day_counter: PyDayCounter::from_inner(accrual_day_counter.inner()),
        }))
    }
    fn callability(&self) -> Vec<PyCallability> {
        self.inner
            .borrow()
            .callability()
            .iter()
            .cloned()
            .map(|inner| PyCallability { inner })
            .collect()
    }
    fn frequency(&self) -> PyResult<PyFrequency> {
        PyFrequency::from_inner(self.frequency)
    }
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.day_counter.inner())
    }
}

/// Callable single-redemption bond retaining its native day counter and base.
#[gen_stub_pyclass]
#[pyclass(name="CallableZeroCouponBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyCallableZeroCouponBond {
    inner: libitofin::shared::SharedMut<CallableZeroCouponBond>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCallableZeroCouponBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "CallableZeroCouponBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, calendar, face_amount, maturity_date, day_counter, callability, settings, *, payment_convention=PyBusinessDayConvention::Following, redemption=100.0, issue_date=None))]
    fn new(
        settlement_days: u32,
        calendar: &PyCalendar,
        face_amount: f64,
        maturity_date: &PyDate,
        day_counter: &PyDayCounter,
        callability: Vec<PyCallability>,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        redemption: f64,
        issue_date: Option<&PyDate>,
    ) -> PyResult<PyClassInitializer<Self>> {
        amounts(settlement_days, face_amount, redemption, issue_date)?;
        adjust(calendar, maturity_date.inner(), payment_convention.inner())?;
        let inner = shared_mut(
            CallableZeroCouponBond::new(
                settlement_days,
                face_amount,
                calendar.inner(),
                maturity_date.inner(),
                day_counter.inner(),
                payment_convention.inner(),
                redemption,
                issue_date.map(PyDate::inner),
                callability.into_iter().map(|call| call.inner).collect(),
                settings.inner(),
            )
            .map_err(PyQlError::from)?,
        );
        Ok(PyClassInitializer::from(PyBond::from_backing(
            BondBacking::CallableZero(inner.clone()),
            calendar.clone(),
        ))
        .add_subclass(Self { inner }))
    }
    fn callability(&self) -> Vec<PyCallability> {
        self.inner
            .borrow()
            .callability()
            .iter()
            .cloned()
            .map(|inner| PyCallability { inner })
            .collect()
    }
    fn day_counter(&self) -> PyDayCounter {
        PyDayCounter::from_inner(self.inner.borrow().day_counter().clone())
    }
}
