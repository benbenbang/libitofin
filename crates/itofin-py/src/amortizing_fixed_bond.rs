//! Named bounded amortizing fixed-rate bond facade over the shared Bond interface.

use crate::bonds::{PyBond, adjust, advance, finite, positive};
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyFrequency, PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::instruments::FixedRateBond;
use libitofin::time::date::Date;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Par-amortizing fixed bond with one positive nonincreasing balance per period.
#[gen_stub_pyclass]
#[pyclass(name = "AmortizingFixedRateBond", extends=PyBond, unsendable, module = "itofin.instruments")]
pub struct PyAmortizingFixedRateBond {
    frequency: PyFrequency,
    day_counter: PyDayCounter,
    first_period_day_counter: Option<PyDayCounter>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyAmortizingFixedRateBond {
    #[gen_stub(override_return_type(type_repr = "AmortizingFixedRateBond"))]
    #[new]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, notionals, schedule, coupons, accrual_day_counter, settings, payment_convention=PyBusinessDayConvention::Following, issue_date=None, payment_calendar=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false, first_period_day_counter=None))]
    fn new(
        settlement_days: u32,
        notionals: Vec<f64>,
        schedule: &PySchedule,
        coupons: Vec<f64>,
        accrual_day_counter: &PyDayCounter,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
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
        for &notional in &notionals {
            positive(notional)?;
        }
        if ex_coupon_period.is_some_and(|period| period.inner().length() < 0) {
            return Err(ItofinError::new_err("ex-coupon period must be nonnegative"));
        }
        for &coupon in &coupons {
            finite(coupon)?;
        }
        let retained_calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        if schedule.len() < 2
            || notionals.len() != schedule.len() - 1
            || !notionals.windows(2).all(|n| n[1] <= n[0])
        {
            return Err(ItofinError::new_err(
                "one positive nonincreasing notional per accrual period required",
            ));
        }
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
        if schedule.has_tenor() && schedule.has_is_regular() {
            let tenor = schedule.tenor();
            if !schedule.is_regular_at(1) {
                advance(
                    &retained_calendar,
                    schedule.date(1),
                    -i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.has_end_of_month() && schedule.end_of_month(),
                )?;
            }
            if schedule.len() > 2 && !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &retained_calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    schedule.has_end_of_month() && schedule.end_of_month(),
                )?;
            }
        }
        let bond = FixedRateBond::new_amortizing(
            settlement_days,
            notionals,
            schedule,
            coupons,
            accrual_day_counter.inner(),
            payment_convention.inner(),
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
