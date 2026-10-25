//! Plain explicit-base CPI leg with terminal indexed principal.
//!
//! Adapted from QuantLib `ql/cashflows/cpicoupon.cpp`, revision
//! `9863b578af0caa4cecabf697196533e84a8308b6`.
//! Copyright (C) 2009 Chris Kenyon; Copyright (C) 2022 Quaternion Risk Management Ltd.
//! See `THIRD_PARTY_NOTICES.md` and `QUANTLIB_LICENSE.txt`.

use super::cpicoupon::{check_base_cpi, checked_cpi_offset, checked_cpi_shift, finite_cpi};
use super::cpidates::{checked_cpi_adjust, checked_cpi_day};
use super::{CpiCashFlow, CpiCoupon, CpiCouponPricer, FixedRateCoupon};
use crate::cashflow::{CashFlow, Leg};
use crate::errors::QlResult;
use crate::indexes::inflationindex::{CpiInterpolationType, ZeroInflationIndex};
use crate::require;
use crate::shared::{Shared, shared};
use crate::time::{
    businessdayconvention::BusinessDayConvention,
    calendar::Calendar,
    daycounter::DayCounter,
    daycounters::thirty360::{Convention, Thirty360},
    period::Period,
    schedule::Schedule,
};

/// CPI coupons followed by an indexed principal or inflation-growth payment.
#[derive(Clone)]
#[must_use]
pub struct CpiLeg {
    schedule: Schedule,
    index: Shared<ZeroInflationIndex>,
    base_cpi: f64,
    lag: Period,
    interpolation: CpiInterpolationType,
    notionals: Vec<f64>,
    rates: Vec<f64>,
    day_counter: DayCounter,
    payment_calendar: Calendar,
    adjustment: BusinessDayConvention,
    growth_only: bool,
    ex_coupon: Option<(Period, Calendar, BusinessDayConvention, bool)>,
}
impl CpiLeg {
    /// Create an explicit-base CPI leg builder; terminal principal defaults to growth only.
    pub fn new(
        schedule: Schedule,
        index: Shared<ZeroInflationIndex>,
        base_cpi: f64,
        observation_lag: Period,
    ) -> Self {
        let payment_calendar = schedule.calendar().clone();
        Self {
            schedule,
            index,
            base_cpi,
            lag: observation_lag,
            interpolation: CpiInterpolationType::Flat,
            notionals: Vec::new(),
            rates: Vec::new(),
            day_counter: Thirty360::with_convention(Convention::BondBasis),
            payment_calendar,
            adjustment: BusinessDayConvention::ModifiedFollowing,
            growth_only: true,
            ex_coupon: None,
        }
    }
    /// Use one notional for all coupons and the terminal indexed payment.
    pub fn with_notional(self, value: f64) -> Self {
        self.with_notionals(vec![value])
    }
    /// Set per-flow notionals, carrying the last value forward.
    pub fn with_notionals(mut self, values: Vec<f64>) -> Self {
        self.notionals = values;
        self
    }
    /// Use one fixed rate for every coupon.
    pub fn with_fixed_rate(self, value: f64) -> Self {
        self.with_fixed_rates(vec![value])
    }
    /// Set per-coupon fixed rates, carrying the last value forward.
    pub fn with_fixed_rates(mut self, values: Vec<f64>) -> Self {
        self.rates = values;
        self
    }
    /// Set the coupon accrual day counter.
    pub fn with_payment_day_counter(mut self, value: DayCounter) -> Self {
        self.day_counter = value;
        self
    }
    /// Set the payment date adjustment convention.
    pub fn with_payment_adjustment(mut self, value: BusinessDayConvention) -> Self {
        self.adjustment = value;
        self
    }
    /// Set the retained calendar used to adjust payment dates.
    pub fn with_payment_calendar(mut self, value: Calendar) -> Self {
        self.payment_calendar = value;
        self
    }
    /// Set Flat or Linear CPI observation interpolation.
    pub fn with_observation_interpolation(mut self, value: CpiInterpolationType) -> Self {
        self.interpolation = value;
        self
    }
    /// Choose growth-only versus full indexed terminal principal.
    pub fn with_subtract_inflation_nominal(mut self, value: bool) -> Self {
        self.growth_only = value;
        self
    }
    /// Set the ex-coupon convention; a zero period disables ex-coupon dates.
    pub fn with_ex_coupon_period(
        mut self,
        period: Period,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
    ) -> Self {
        self.ex_coupon = if period.length() == 0 {
            None
        } else {
            Some((period, calendar, convention, end_of_month))
        };
        self
    }
    /// Materialize the coupons and terminal indexed principal with live retained indices.
    ///
    /// # Errors
    /// Returns an error for invalid inputs, unsupported dates, unavailable fixings
    /// or curves, and non-finite calculated values.
    pub fn build(&self) -> QlResult<Leg> {
        check_base_cpi(self.base_cpi)?;
        require!(!self.schedule.is_empty(), "empty CPI schedule");
        require!(!self.notionals.is_empty(), "no CPI notional given");
        let periods = self.schedule.len() - 1;
        require!(
            periods == 0 || !self.rates.is_empty(),
            "no CPI fixed rates given"
        );
        require!(
            self.notionals.len() <= periods + 1 && self.rates.len() <= periods,
            "too many CPI leg parameters"
        );
        for v in self.notionals.iter().chain(self.rates.iter()) {
            finite_cpi(*v)?;
        }
        for date in self.schedule.dates() {
            checked_cpi_shift(*date, self.lag)?;
        }
        let mut flows = Vec::with_capacity(periods + 1);
        let last = |v: &[f64], i: usize| v[i.min(v.len() - 1)];
        let pricer = shared(CpiCouponPricer::default());
        for i in 0..periods {
            let start = self.schedule.date(i);
            let end = self.schedule.date(i + 1);
            require!(start < end, "CPI schedule dates must increase");
            let mut ref_start = start;
            let mut ref_end = end;
            if self.schedule.has_is_regular()
                && self.schedule.has_tenor()
                && !self.schedule.is_regular_at(i + 1)
            {
                if i == 0 {
                    ref_start = checked_cpi_adjust(
                        self.schedule.calendar(),
                        checked_cpi_shift(end, self.schedule.tenor())?,
                        self.schedule.business_day_convention(),
                    )?;
                }
                if i + 1 == periods {
                    let tenor = self.schedule.tenor();
                    ref_end = checked_cpi_adjust(
                        self.schedule.calendar(),
                        checked_cpi_offset(start, tenor, true)?,
                        self.schedule.business_day_convention(),
                    )?;
                }
            }
            let payment = checked_cpi_adjust(&self.payment_calendar, end, self.adjustment)?;
            let ex_coupon = if let Some((period, cal, conv, eom)) = &self.ex_coupon {
                checked_cpi_shift(payment, *period)?;
                Some(ex_coupon_date(cal, payment, *period, *conv, *eom)?)
            } else {
                None
            };
            let nominal = last(&self.notionals, i);
            let rate = last(&self.rates, i);
            let flow: Shared<dyn CashFlow> = if rate == 0.0 {
                shared(FixedRateCoupon::from_rate(
                    payment,
                    nominal,
                    rate,
                    self.day_counter.clone(),
                    start,
                    end,
                    Some(ref_start),
                    Some(ref_end),
                    ex_coupon,
                ))
            } else {
                let coupon = CpiCoupon::new(
                    self.base_cpi,
                    payment,
                    nominal,
                    start,
                    end,
                    self.index.clone(),
                    self.lag,
                    self.interpolation,
                    self.day_counter.clone(),
                    rate,
                    Some(ref_start),
                    Some(ref_end),
                    ex_coupon,
                )?;
                coupon.set_pricer(pricer.clone());
                shared(coupon)
            };
            flows.push(flow);
        }
        let observation = self.schedule.date(periods);
        let payment = checked_cpi_adjust(&self.payment_calendar, observation, self.adjustment)?;
        flows.push(shared(CpiCashFlow::new(
            last(&self.notionals, periods),
            self.index.clone(),
            self.base_cpi,
            observation,
            self.lag,
            self.interpolation,
            payment,
            self.growth_only,
        )?) as Shared<dyn CashFlow>);
        Ok(flows)
    }
}

fn ex_coupon_date(
    calendar: &Calendar,
    payment: crate::time::date::Date,
    period: Period,
    convention: BusinessDayConvention,
    eom: bool,
) -> QlResult<crate::time::date::Date> {
    use crate::time::{date::Date, timeunit::TimeUnit};
    checked_cpi_shift(payment, period)?;
    if period.units() == TimeUnit::Days && period.length() != 0 {
        let mut date = payment;
        for _ in 0..period.length() {
            date = checked_cpi_day(date, -1)?;
            while calendar.is_holiday(date) {
                date = checked_cpi_day(date, -1)?;
            }
        }
        return Ok(date);
    }
    let shifted = checked_cpi_shift(payment, period)?;
    if eom && matches!(period.units(), TimeUnit::Months | TimeUnit::Years) {
        let end = if convention == BusinessDayConvention::Unadjusted {
            Date::end_of_month(payment)
        } else {
            checked_cpi_adjust(
                calendar,
                Date::end_of_month(payment),
                BusinessDayConvention::Preceding,
            )?
        };
        if payment >= end {
            return if convention == BusinessDayConvention::Unadjusted {
                Ok(Date::end_of_month(shifted))
            } else {
                checked_cpi_adjust(
                    calendar,
                    Date::end_of_month(shifted),
                    BusinessDayConvention::Preceding,
                )
            };
        }
    }
    checked_cpi_adjust(calendar, shifted, convention)
}
