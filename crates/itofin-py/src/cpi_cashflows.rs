//! Explicit-base plain CPI coupon, pricer and leg facades.
use crate::PyQlError;
use crate::bonds::{adjust, advance, finite};
use crate::cashflows::PyLeg;
use crate::curve::PyYieldTermStructure;
use crate::inflation::{PyCpiInterpolationType, PyZeroInflationIndex};
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use libitofin::cashflow::Leg;
use libitofin::cashflows::{Coupon, CpiCoupon, CpiCouponPricer, CpiLeg};
use libitofin::handle::Handle;
use libitofin::indexes::inflationindex::InflationIndex;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::Date;
use libitofin::time::{businessdayconvention::BusinessDayConvention as Bdc, schedule::Schedule};
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};

pub(crate) fn check_leg_dates(
    schedule: &Schedule,
    schedule_calendar: &PyCalendar,
    payment: &PyCalendar,
    adjustment: Bdc,
) -> PyResult<()> {
    for &date in schedule.dates() {
        adjust(payment, date, adjustment)?;
    }
    if schedule.len() >= 2 && schedule.has_tenor() && schedule.has_is_regular() {
        let n = schedule.len() - 1;
        let tenor = schedule.tenor();
        let arithmetic = PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new());
        for (position, anchor, direction) in [
            (1, schedule.date(1), -1_i64),
            (n, schedule.date(n - 1), 1_i64),
        ] {
            if !schedule.is_regular_at(position) {
                let raw = advance(
                    &arithmetic,
                    anchor,
                    i64::from(tenor.length()) * direction,
                    tenor.units(),
                    Bdc::Unadjusted,
                    false,
                )?;
                adjust(schedule_calendar, raw, schedule.business_day_convention())?;
            }
        }
    }
    Ok(())
}

/// Plain CPI swaplet pricer. Discounted prices are per unit nominal.
#[gen_stub_pyclass]
#[pyclass(name = "CPICouponPricer", unsendable, module = "itofin.cashflows")]
pub struct PyCpiCouponPricer {
    inner: Shared<CpiCouponPricer>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiCouponPricer {
    /// Build a plain CPI swaplet pricer, optionally retaining a nominal curve.
    ///
    /// Rate queries need no nominal curve; swaplet_price requires one and
    /// returns a discounted value per unit nominal, not a currency amount.
    #[new]
    #[pyo3(signature=(nominal_curve=None))]
    fn new(nominal_curve: Option<&PyYieldTermStructure>) -> Self {
        Self {
            inner: shared(CpiCouponPricer::new(
                nominal_curve.map_or_else(Handle::empty, PyYieldTermStructure::handle),
            )),
        }
    }
    fn swaplet_rate(&self, coupon: &PyCpiCoupon) -> PyResult<f64> {
        Ok(self
            .inner
            .swaplet_rate(&coupon.inner)
            .map_err(PyQlError::from)?)
    }
    fn swaplet_price(&self, coupon: &PyCpiCoupon) -> PyResult<f64> {
        Ok(self
            .inner
            .swaplet_price(&coupon.inner)
            .map_err(PyQlError::from)?)
    }
    fn accrued_rate(&self, coupon: &PyCpiCoupon, date: &PyDate) -> PyResult<f64> {
        Ok(self
            .inner
            .accrued_rate(&coupon.inner, date.inner())
            .map_err(PyQlError::from)?)
    }
}

/// Explicit-base CPI-scaled fixed coupon. Attach a plain pricer before pricing.
#[gen_stub_pyclass]
#[pyclass(name = "CPICoupon", unsendable, module = "itofin.cashflows")]
pub struct PyCpiCoupon {
    inner: Shared<CpiCoupon>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiCoupon {
    /// Build a coupon using an explicit CPI denominator and lagged observation.
    ///
    /// Attach a CPICouponPricer before rate or amount queries. The base CPI
    /// must be finite and nonzero; its interpolation convention is caller-owned.
    ///
    /// Raises:
    ///     ItofinError: On invalid terms, dates, lags or a non-finite parameter.
    #[new]
    #[pyo3(signature=(base_cpi,payment_date,nominal,start_date,end_date,index,observation_lag,interpolation,day_counter,fixed_rate,reference_start=None,reference_end=None,ex_coupon_date=None))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        base_cpi: f64,
        payment_date: &PyDate,
        nominal: f64,
        start_date: &PyDate,
        end_date: &PyDate,
        index: &PyZeroInflationIndex,
        observation_lag: &PyPeriod,
        interpolation: PyCpiInterpolationType,
        day_counter: &PyDayCounter,
        fixed_rate: f64,
        reference_start: Option<&PyDate>,
        reference_end: Option<&PyDate>,
        ex_coupon_date: Option<&PyDate>,
    ) -> PyResult<Self> {
        let inner = CpiCoupon::new(
            base_cpi,
            payment_date.inner(),
            nominal,
            start_date.inner(),
            end_date.inner(),
            index.shared(),
            observation_lag.inner(),
            interpolation.inner(),
            day_counter.inner(),
            fixed_rate,
            reference_start.map(PyDate::inner),
            reference_end.map(PyDate::inner),
            ex_coupon_date.map(PyDate::inner),
        )
        .map_err(PyQlError::from)?;
        Ok(Self {
            inner: shared(inner),
        })
    }
    /// Build a coupon with a lazy, live CPI denominator observed from base_date.
    /// Existing explicit-base construction is unchanged. Use base_fixing(), not
    /// the native stored-base MAX sentinel, for the effective denominator.
    #[staticmethod]
    #[pyo3(signature=(base_date,payment_date,nominal,start_date,end_date,index,observation_lag,interpolation,day_counter,fixed_rate,reference_start=None,reference_end=None,ex_coupon_date=None))]
    #[allow(clippy::too_many_arguments)]
    fn from_base_date(
        base_date: &PyDate,
        payment_date: &PyDate,
        nominal: f64,
        start_date: &PyDate,
        end_date: &PyDate,
        index: &PyZeroInflationIndex,
        observation_lag: &PyPeriod,
        interpolation: PyCpiInterpolationType,
        day_counter: &PyDayCounter,
        fixed_rate: f64,
        reference_start: Option<&PyDate>,
        reference_end: Option<&PyDate>,
        ex_coupon_date: Option<&PyDate>,
    ) -> PyResult<Self> {
        Ok(Self {
            inner: shared(
                CpiCoupon::from_base_date(
                    base_date.inner(),
                    payment_date.inner(),
                    nominal,
                    start_date.inner(),
                    end_date.inner(),
                    index.shared(),
                    observation_lag.inner(),
                    interpolation.inner(),
                    day_counter.inner(),
                    fixed_rate,
                    reference_start.map(PyDate::inner),
                    reference_end.map(PyDate::inner),
                    ex_coupon_date.map(PyDate::inner),
                )
                .map_err(PyQlError::from)?,
            ),
        })
    }
    /// Effective denominator, read live for a date-based coupon.
    fn base_fixing(&self) -> PyResult<f64> {
        Ok(self.inner.base_fixing().map_err(PyQlError::from)?)
    }
    fn set_pricer(&self, pricer: &PyCpiCouponPricer) {
        self.inner.set_pricer(pricer.inner.clone());
    }
    fn rate(&self) -> PyResult<f64> {
        Ok(self.inner.rate().map_err(PyQlError::from)?)
    }
    fn amount(&self) -> PyResult<f64> {
        Ok(self.inner.amount().map_err(PyQlError::from)?)
    }
    fn accrued_amount(&self, date: &PyDate) -> PyResult<f64> {
        Ok(self
            .inner
            .accrued_amount(date.inner())
            .map_err(PyQlError::from)?)
    }
    fn index_ratio(&self, date: &PyDate) -> PyResult<f64> {
        Ok(self
            .inner
            .index_ratio(date.inner())
            .map_err(PyQlError::from)?)
    }
    fn index_fixing(&self) -> PyResult<f64> {
        Ok(self.inner.index_fixing().map_err(PyQlError::from)?)
    }
    fn accrual_period(&self) -> f64 {
        self.inner.accrual_period()
    }
}

/// Plain CPI coupons and one terminal indexed principal payment.
#[gen_stub_pyclass]
#[pyclass(name = "CPILeg", unsendable, module = "itofin.cashflows")]
pub struct PyCpiLeg {
    inner: Leg,
    settings: Shared<Settings<Date>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyCpiLeg {
    /// Materialize plain CPI coupons and one indexed terminal payment.
    ///
    /// Payment and ex-coupon dates are fixed at construction. CPI histories
    /// and linked curves remain live; build clones the retained cash-flow graph.
    /// A default plain pricer is attached, and growth_only selects whether the
    /// uninflated terminal principal is subtracted.
    ///
    /// Raises:
    ///     ItofinError: On invalid terms, calendars, dates or leg parameters.
    #[new]
    #[pyo3(signature=(schedule,index,base_cpi,observation_lag,notionals,fixed_rates,day_counter,payment_calendar,payment_adjustment,interpolation=PyCpiInterpolationType::Flat,growth_only=true,ex_coupon_period=None,ex_coupon_calendar=None,ex_coupon_adjustment=PyBusinessDayConvention::Unadjusted,ex_coupon_end_of_month=false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        schedule: &PySchedule,
        index: &PyZeroInflationIndex,
        base_cpi: f64,
        observation_lag: &PyPeriod,
        notionals: Vec<f64>,
        fixed_rates: Vec<f64>,
        day_counter: &PyDayCounter,
        payment_calendar: &PyCalendar,
        payment_adjustment: PyBusinessDayConvention,
        interpolation: PyCpiInterpolationType,
        growth_only: bool,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_adjustment: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
    ) -> PyResult<Self> {
        make_leg(
            schedule,
            index,
            base_cpi,
            observation_lag,
            notionals,
            fixed_rates,
            day_counter,
            payment_calendar,
            payment_adjustment,
            interpolation,
            growth_only,
            ex_coupon_period,
            ex_coupon_calendar,
            ex_coupon_adjustment,
            ex_coupon_end_of_month,
            None,
        )
    }
    /// Build CPI coupons with a live base date and native terminal observation.
    /// Omitted base_date uses schedule start minus observation_lag. No fixings
    /// are read while constructing the retained, fixed-payment-date graph.
    #[staticmethod]
    #[pyo3(signature=(schedule,index,observation_lag,notionals,fixed_rates,day_counter,payment_calendar,payment_adjustment,base_date=None,interpolation=PyCpiInterpolationType::Flat,growth_only=true,ex_coupon_period=None,ex_coupon_calendar=None,ex_coupon_adjustment=PyBusinessDayConvention::Unadjusted,ex_coupon_end_of_month=false))]
    #[allow(clippy::too_many_arguments)]
    fn from_base_date(
        schedule: &PySchedule,
        index: &PyZeroInflationIndex,
        observation_lag: &PyPeriod,
        notionals: Vec<f64>,
        fixed_rates: Vec<f64>,
        day_counter: &PyDayCounter,
        payment_calendar: &PyCalendar,
        payment_adjustment: PyBusinessDayConvention,
        base_date: Option<&PyDate>,
        interpolation: PyCpiInterpolationType,
        growth_only: bool,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_adjustment: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
    ) -> PyResult<Self> {
        make_leg(
            schedule,
            index,
            0.0,
            observation_lag,
            notionals,
            fixed_rates,
            day_counter,
            payment_calendar,
            payment_adjustment,
            interpolation,
            growth_only,
            ex_coupon_period,
            ex_coupon_calendar,
            ex_coupon_adjustment,
            ex_coupon_end_of_month,
            Some(base_date.map(PyDate::inner)),
        )
    }
    fn build(&self) -> PyResult<PyLeg> {
        Ok(PyLeg::with_cpi_settings(
            self.inner.clone(),
            self.settings.clone(),
        ))
    }
}

#[allow(clippy::too_many_arguments)]
fn make_leg(
    schedule: &PySchedule,
    index: &PyZeroInflationIndex,
    base_cpi: f64,
    observation_lag: &PyPeriod,
    notionals: Vec<f64>,
    fixed_rates: Vec<f64>,
    day_counter: &PyDayCounter,
    payment_calendar: &PyCalendar,
    payment_adjustment: PyBusinessDayConvention,
    interpolation: PyCpiInterpolationType,
    growth_only: bool,
    ex_coupon_period: Option<&PyPeriod>,
    ex_coupon_calendar: Option<&PyCalendar>,
    ex_coupon_adjustment: PyBusinessDayConvention,
    ex_coupon_end_of_month: bool,
    base_date: Option<Option<Date>>,
) -> PyResult<PyCpiLeg> {
    check_leg_dates(
        &schedule.inner(),
        &schedule.retained_calendar(),
        payment_calendar,
        payment_adjustment.inner(),
    )?;
    let builder = match base_date {
        Some(date) => CpiLeg::from_base_date(
            schedule.inner(),
            index.shared(),
            date,
            observation_lag.inner(),
        ),
        None => CpiLeg::new(
            schedule.inner(),
            index.shared(),
            finite(base_cpi)?,
            observation_lag.inner(),
        ),
    };
    let mut leg = builder
        .with_notionals(notionals)
        .with_fixed_rates(fixed_rates)
        .with_payment_day_counter(day_counter.inner())
        .with_payment_calendar(payment_calendar.inner())
        .with_payment_adjustment(payment_adjustment.inner())
        .with_observation_interpolation(interpolation.inner())
        .with_subtract_inflation_nominal(growth_only);
    if let Some(period) = ex_coupon_period.filter(|p| p.inner().length() != 0) {
        let calendar = ex_coupon_calendar.ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("ex-coupon calendar required")
        })?;
        for &date in schedule.inner().dates() {
            let payment = adjust(payment_calendar, date, payment_adjustment.inner())?;
            advance(
                calendar,
                payment,
                -i64::from(period.inner().length()),
                period.inner().units(),
                ex_coupon_adjustment.inner(),
                ex_coupon_end_of_month,
            )?;
        }
        leg = leg.with_ex_coupon_period(
            period.inner(),
            calendar.inner(),
            ex_coupon_adjustment.inner(),
            ex_coupon_end_of_month,
        );
    }
    let leg = leg.build().map_err(PyQlError::from)?;
    let settings = index.shared().inflation_base().settings().clone();
    Ok(PyCpiLeg {
        inner: leg,
        settings,
    })
}
