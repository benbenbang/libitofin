//! Modern CPI bond with the common retained Bond valuation interface.
use crate::bonds::{PyBond, adjust, advance};
use crate::inflation::{PyCpiInterpolationType, PyZeroInflationIndex};
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::instruments::CpiBond;
use libitofin::time::calendars::NullCalendar;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

/// Plain CPI-indexed coupon bond with explicit base CPI and indexed principal.
#[gen_stub_pyclass]
#[pyclass(name="CPIBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyCpiBond;

#[gen_stub_pymethods]
#[pymethods]
impl PyCpiBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "CPIBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, face_amount, base_cpi, observation_lag, index, observation_interpolation, schedule, coupons, accrual_day_counter, settings, *, payment_convention=PyBusinessDayConvention::ModifiedFollowing, issue_date=None, payment_calendar=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false))]
    fn new(
        settlement_days: u32,
        face_amount: f64,
        base_cpi: f64,
        observation_lag: &PyPeriod,
        index: &PyZeroInflationIndex,
        observation_interpolation: PyCpiInterpolationType,
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
    ) -> PyResult<PyClassInitializer<Self>> {
        let calendar = payment_calendar
            .cloned()
            .unwrap_or_else(|| schedule.retained_calendar());
        let ex_calendar = ex_coupon_calendar
            .cloned()
            .unwrap_or_else(|| PyCalendar::from_inner(NullCalendar::new()));
        if ex_coupon_period.is_some_and(|period| period.inner().length() < 0) {
            return Err(ItofinError::new_err("ex-coupon period must be nonnegative"));
        }
        let retained_calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        crate::cpi_cashflows::check_leg_dates(
            &schedule,
            &retained_calendar,
            &calendar,
            payment_convention.inner(),
        )?;
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
        let bond = CpiBond::new(
            settlement_days,
            face_amount,
            base_cpi,
            observation_lag.inner(),
            index.shared(),
            observation_interpolation.inner(),
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
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        Ok(
            PyClassInitializer::from(PyBond::from_indexed(bond.into_bond(), calendar))
                .add_subclass(Self),
        )
    }
}
