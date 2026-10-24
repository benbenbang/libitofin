//! Floating bond facade retaining typed coupons beside the shared Bond base.

use crate::bonds::{PyBond, adjust, advance, finite, positive, step};
use crate::cashflows::PyBlackIborCouponPricer;
use crate::hullwhite::PyIborIndex;
use crate::optionletvol::MovingOptionletMetadata;
use crate::settings::PySettings;
use crate::time::{
    PyBusinessDayConvention, PyCalendar, PyDate, PyDayCounter, PyPeriod, PySchedule,
};
use crate::{ItofinError, PyQlError};
use libitofin::cashflows::IborCoupon;
use libitofin::indexes::{IborIndex, Index, InterestRateIndex};
use libitofin::instruments::FloatingRateBond;
use libitofin::shared::{Shared, SharedMut, shared};
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::date::Date;
use libitofin::time::timeunit::TimeUnit;
use libitofin::utilities::null::Null;
use pyo3::prelude::*;
use pyo3_stub_gen::derive::{gen_stub_pyclass, gen_stub_pymethods};
use std::cell::RefCell;

struct FloatingGuard {
    index: Shared<IborIndex>,
    calendars: [PyCalendar; 3],
    dates: Vec<(Date, Date)>,
    floating: Vec<bool>,
    optionlets: Vec<bool>,
    fixing_days: u32,
    convention: Bdc,
    moving: RefCell<Option<Shared<MovingOptionletMetadata>>>,
}
impl FloatingGuard {
    fn check(&self, position: Option<usize>) -> PyResult<()> {
        self.check_inner(position, false)
    }
    fn check_inner(&self, position: Option<usize>, construction: bool) -> PyResult<()> {
        let [fixing_calendar, value_calendar, maturity_calendar] = &self.calendars;
        let index_days = i64::from(self.index.fixing_days());
        let evaluation = Index::settings(&*self.index).evaluation_date();
        let mut needs_volatility = false;
        let positions = match position {
            Some(i) if i < self.dates.len() => i..i + 1,
            Some(_) => 0..0,
            None => 0..self.dates.len(),
        };
        for i in positions {
            let (start, end) = self.dates[i];
            if !self.floating[i] {
                continue;
            }
            let fixing = advance(
                fixing_calendar,
                start,
                -i64::from(self.fixing_days),
                TimeUnit::Days,
                self.convention,
                false,
            )?;
            if construction {
                continue;
            }
            fixing_calendar.checked(&PyDate::from_inner(fixing), "value a floating coupon")?;
            let future = evaluation.is_none_or(|evaluation| fixing > evaluation);
            let forecast = future
                || evaluation.is_some_and(|evaluation| {
                    fixing == evaluation
                        && !Index::settings(&*self.index).enforces_todays_historic_fixings()
                        && !self.index.has_historical_fixing(fixing)
                });
            needs_volatility |= future && self.optionlets[i];
            if !forecast {
                continue;
            }
            let value = advance(
                fixing_calendar,
                fixing,
                index_days,
                TimeUnit::Days,
                Bdc::Following,
                false,
            )?;
            let tenor = self.index.tenor();
            advance(
                maturity_calendar,
                value,
                i64::from(tenor.length()),
                tenor.units(),
                self.index.business_day_convention(),
                self.index.end_of_month(),
            )?;
            if Index::settings(&*self.index).using_at_par_coupons() {
                let next_fixing = advance(
                    fixing_calendar,
                    end,
                    -i64::from(self.fixing_days),
                    TimeUnit::Days,
                    Bdc::Following,
                    false,
                )?;
                advance(
                    fixing_calendar,
                    next_fixing,
                    index_days,
                    TimeUnit::Days,
                    Bdc::Following,
                    false,
                )?;
                step(value, 1)?;
            }
            {
                let calendar = value_calendar;
                let value = advance(
                    calendar,
                    fixing,
                    index_days,
                    TimeUnit::Days,
                    Bdc::Following,
                    false,
                )?;
                let value = adjust(maturity_calendar, value, Bdc::Following)?;
                advance(
                    maturity_calendar,
                    value,
                    i64::from(tenor.length()),
                    tenor.units(),
                    self.index.business_day_convention(),
                    self.index.end_of_month(),
                )?;
            }
        }
        if needs_volatility {
            let moving = self.moving.borrow().clone();
            if let Some(moving) = moving {
                moving.check()?;
            }
        }
        Ok(())
    }
}

/// Scheduled ibor bond with optional gearing, spread, cap and floor vectors.
#[gen_stub_pyclass]
#[pyclass(name="FloatingRateBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyFloatingRateBond {
    guard: Shared<FloatingGuard>,
    coupons: Vec<Option<Shared<IborCoupon>>>,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyFloatingRateBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "FloatingRateBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, face_amount, schedule, index, accrual_day_counter, settings, *, payment_convention=PyBusinessDayConvention::Following, fixing_days=None, gearings=None, spreads=None, caps=None, floors=None, redemption=100.0, issue_date=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false, fixing_convention=PyBusinessDayConvention::Preceding))]
    fn new(
        settlement_days: u32,
        face_amount: f64,
        schedule: &PySchedule,
        index: &PyIborIndex,
        accrual_day_counter: &PyDayCounter,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        fixing_days: Option<u32>,
        gearings: Option<Vec<f64>>,
        spreads: Option<Vec<f64>>,
        caps: Option<Vec<Option<f64>>>,
        floors: Option<Vec<Option<f64>>>,
        redemption: f64,
        issue_date: Option<&PyDate>,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_convention: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
        fixing_convention: PyBusinessDayConvention,
    ) -> PyResult<PyClassInitializer<Self>> {
        positive(face_amount)?;
        positive(redemption)?;
        positive(face_amount * redemption / 100.0)?;
        let index_inner = index.inner();
        let effective_fixing_days = fixing_days.unwrap_or_else(|| index_inner.fixing_days());
        let date_span = (Date::max_date() - Date::min_date()) as u32;
        if settlement_days > date_span
            || effective_fixing_days > date_span
            || index_inner.fixing_days() > date_span
        {
            return Err(ItofinError::new_err(
                "bond or index fixing days exceed supported date range",
            ));
        }
        let gearings = gearings.unwrap_or_default();
        let spreads = spreads.unwrap_or_default();
        let caps: Vec<f64> = caps
            .unwrap_or_default()
            .into_iter()
            .map(|cap| cap.unwrap_or_else(f64::null))
            .collect();
        let floors: Vec<f64> = floors
            .unwrap_or_default()
            .into_iter()
            .map(|floor| floor.unwrap_or_else(f64::null))
            .collect();
        for &value in gearings.iter().chain(&spreads).chain(&caps).chain(&floors) {
            finite(value)?;
        }
        if issue_date.is_some_and(|date| date.inner() == Date::null()) {
            return Err(ItofinError::new_err("null issue date"));
        }
        let calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        let ex_calendar = ex_coupon_calendar.cloned().unwrap_or_else(|| {
            PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new())
        });
        for &date in schedule.dates().iter().skip(1) {
            let payment = adjust(&calendar, date, payment_convention.inner())?;
            if let Some(period) = ex_coupon_period
                .map(PyPeriod::inner)
                .filter(|period| period.length() != 0)
            {
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
                    &calendar,
                    schedule.date(1),
                    -i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    false,
                )?;
            }
            if !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(tenor.length()),
                    tenor.units(),
                    schedule.business_day_convention(),
                    false,
                )?;
            }
        }
        let dates = schedule
            .dates()
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect::<Vec<_>>();
        let floating = (0..dates.len())
            .map(|i| {
                gearings
                    .get(i)
                    .or_else(|| gearings.last())
                    .copied()
                    .unwrap_or(1.0)
                    != 0.0
            })
            .collect();
        let optionlets = (0..dates.len())
            .map(|i| {
                caps.get(i)
                    .or_else(|| caps.last())
                    .is_some_and(|cap| !cap.is_null())
                    || floors
                        .get(i)
                        .or_else(|| floors.last())
                        .is_some_and(|floor| !floor.is_null())
            })
            .collect();
        let guard = shared(FloatingGuard {
            index: Shared::clone(&index_inner),
            calendars: index.retained_calendars(),
            dates,
            floating,
            optionlets,
            fixing_days: effective_fixing_days,
            convention: fixing_convention.inner(),
            moving: RefCell::new(None),
        });
        guard.check_inner(None, true)?;
        let bond = FloatingRateBond::new(
            settlement_days,
            face_amount,
            schedule,
            index_inner,
            accrual_day_counter.inner(),
            payment_convention.inner(),
            fixing_days,
            gearings,
            spreads,
            caps,
            floors,
            redemption,
            issue_date.map(PyDate::inner),
            ex_coupon_period.map(PyPeriod::inner),
            ex_calendar.inner(),
            ex_coupon_convention.inner(),
            ex_coupon_end_of_month,
            fixing_convention.inner(),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        let coupons = bond.ibor_coupons().to_vec();
        let retained_guard = Shared::clone(&guard);
        let base = PyBond::from_floating(
            bond.into_bond(),
            calendar,
            shared(move |position| retained_guard.check(position)),
        );
        Ok(PyClassInitializer::from(base).add_subclass(Self { guard, coupons }))
    }
    /// Return the same retained index, including its original calendar bounds.
    fn index(&self) -> PyIborIndex {
        PyIborIndex::with_calendars(
            Shared::clone(&self.guard.index),
            self.guard.calendars.clone(),
        )
    }
    /// Install or replace one retained pricer on every nonzero-gearing coupon.
    fn set_coupon_pricer(&self, pricer: &PyBlackIborCouponPricer) -> PyResult<()> {
        if let Some(moving) = &pricer.moving
            && !std::ptr::eq(&*moving.settings, Index::settings(&*self.guard.index))
        {
            return Err(ItofinError::new_err(
                "bond and moving optionlet surface must share settings",
            ));
        }
        for coupon in self.coupons.iter().flatten() {
            coupon.set_pricer(SharedMut::clone(&pricer.inner));
        }
        *self.guard.moving.borrow_mut() = pricer.moving.clone();
        Ok(())
    }
}

/// Plain ibor amortization with one positive balance per coupon period and par principal.
/// Settlement, coupon and index fixing lags are bounded by the supported Date span.
#[gen_stub_pyclass]
#[pyclass(name="AmortizingFloatingRateBond", extends=PyBond, unsendable, module="itofin.instruments")]
pub struct PyAmortizingFloatingRateBond {
    floating: PyFloatingRateBond,
}
#[gen_stub_pymethods]
#[pymethods]
impl PyAmortizingFloatingRateBond {
    #[new]
    #[gen_stub(override_return_type(type_repr = "AmortizingFloatingRateBond"))]
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature=(settlement_days, notionals, schedule, index, accrual_day_counter, settings, *, payment_convention=PyBusinessDayConvention::Following, fixing_days=None, spreads=None, issue_date=None, ex_coupon_period=None, ex_coupon_calendar=None, ex_coupon_convention=PyBusinessDayConvention::Unadjusted, ex_coupon_end_of_month=false, fixing_convention=PyBusinessDayConvention::Preceding))]
    fn new(
        settlement_days: u32,
        notionals: Vec<f64>,
        schedule: &PySchedule,
        index: &PyIborIndex,
        accrual_day_counter: &PyDayCounter,
        settings: &PySettings,
        payment_convention: PyBusinessDayConvention,
        fixing_days: Option<u32>,
        spreads: Option<Vec<f64>>,
        issue_date: Option<&PyDate>,
        ex_coupon_period: Option<&PyPeriod>,
        ex_coupon_calendar: Option<&PyCalendar>,
        ex_coupon_convention: PyBusinessDayConvention,
        ex_coupon_end_of_month: bool,
        fixing_convention: PyBusinessDayConvention,
    ) -> PyResult<PyClassInitializer<Self>> {
        let calendar = schedule.retained_calendar();
        let schedule = schedule.inner();
        if schedule.len() < 2 || notionals.len() != schedule.len() - 1 {
            return Err(ItofinError::new_err(
                "one notional per schedule period required",
            ));
        }
        for &notional in &notionals {
            positive(notional)?;
        }
        if notionals.windows(2).any(|n| n[1] > n[0]) {
            return Err(ItofinError::new_err("notionals must be nonincreasing"));
        }
        let spreads = spreads.unwrap_or_default();
        if spreads.len() > notionals.len() {
            return Err(ItofinError::new_err(
                "spread vector exceeds schedule periods",
            ));
        }
        for &spread in &spreads {
            finite(spread)?;
        }
        let index_inner = index.inner();
        let effective_fixing_days = fixing_days.unwrap_or_else(|| index_inner.fixing_days());
        let date_span = (Date::max_date() - Date::min_date()) as u32;
        if settlement_days > date_span
            || effective_fixing_days > date_span
            || index_inner.fixing_days() > date_span
        {
            return Err(ItofinError::new_err(
                "bond or index fixing days exceed supported date range",
            ));
        }
        if issue_date.is_some_and(|d| d.inner() == Date::null())
            || ex_coupon_period.is_some_and(|p| p.inner().length() < 0)
        {
            return Err(ItofinError::new_err(
                "invalid issue date or ex-coupon period",
            ));
        }
        let ex_calendar = ex_coupon_calendar.cloned().unwrap_or_else(|| {
            PyCalendar::from_inner(libitofin::time::calendars::NullCalendar::new())
        });
        for &date in schedule.dates().iter().skip(1) {
            let payment = adjust(&calendar, date, payment_convention.inner())?;
            if let Some(period) = ex_coupon_period
                .map(PyPeriod::inner)
                .filter(|p| p.length() != 0)
            {
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
            let p = schedule.tenor();
            if !schedule.is_regular_at(1) {
                advance(
                    &calendar,
                    schedule.date(1),
                    -i64::from(p.length()),
                    p.units(),
                    schedule.business_day_convention(),
                    false,
                )?;
            }
            if !schedule.is_regular_at(schedule.len() - 1) {
                advance(
                    &calendar,
                    schedule.date(schedule.len() - 2),
                    i64::from(p.length()),
                    p.units(),
                    schedule.business_day_convention(),
                    false,
                )?;
            }
        }
        let dates: Vec<_> = schedule.dates().windows(2).map(|d| (d[0], d[1])).collect();
        let guard = shared(FloatingGuard {
            index: Shared::clone(&index_inner),
            calendars: index.retained_calendars(),
            floating: vec![true; dates.len()],
            optionlets: vec![false; dates.len()],
            dates,
            fixing_days: effective_fixing_days,
            convention: fixing_convention.inner(),
            moving: RefCell::new(None),
        });
        guard.check_inner(None, true)?;
        let bond = FloatingRateBond::new_amortizing(
            settlement_days,
            notionals,
            schedule,
            index_inner,
            accrual_day_counter.inner(),
            payment_convention.inner(),
            fixing_days,
            spreads,
            issue_date.map(PyDate::inner),
            ex_coupon_period.map(PyPeriod::inner),
            ex_calendar.inner(),
            ex_coupon_convention.inner(),
            ex_coupon_end_of_month,
            fixing_convention.inner(),
            settings.inner(),
        )
        .map_err(PyQlError::from)?;
        let coupons = bond.ibor_coupons().to_vec();
        let by_flow: std::collections::HashMap<_, _> = coupons
            .iter()
            .enumerate()
            .filter_map(|(i, coupon)| coupon.as_ref().map(|c| (Shared::as_ptr(c) as *const (), i)))
            .collect();
        let positions: Vec<_> = bond
            .bond()
            .cashflows()
            .iter()
            .map(|flow| by_flow.get(&(Shared::as_ptr(flow) as *const ())).copied())
            .collect();
        let retained = Shared::clone(&guard);
        let base = PyBond::from_floating(
            bond.into_bond(),
            calendar,
            shared(move |position| match position {
                None => retained.check(None),
                Some(i) => match positions.get(i).copied().flatten() {
                    Some(coupon) => retained.check(Some(coupon)),
                    None => Ok(()),
                },
            }),
        );
        Ok(PyClassInitializer::from(base).add_subclass(Self {
            floating: PyFloatingRateBond { guard, coupons },
        }))
    }
    /// Return the retained live ibor index with its original calendar bounds.
    fn index(&self) -> PyIborIndex {
        self.floating.index()
    }
    /// Replace the retained plain coupon pricer without changing principal flows.
    fn set_coupon_pricer(&self, pricer: &PyBlackIborCouponPricer) -> PyResult<()> {
        self.floating.set_coupon_pricer(pricer)
    }
}
