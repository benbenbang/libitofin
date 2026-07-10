//! The fixed-rate leg builder.
//!
//! Port of the `FixedRateLeg` half of `ql/cashflows/fixedratecoupon.{hpp,cpp}`:
//! a fluent builder turning a [`Schedule`] plus notionals and coupon rates into
//! a sequence of [`FixedRateCoupon`]s. The first and last periods may be short
//! or long, in which case the coupon accrues against a reference period one
//! tenor away from the stub, so that a schedule-aware day counter still sees a
//! regular period.
//!
//! ## Divergences from QuantLib
//!
//! C++ ends the builder with `operator Leg()`. The port splits that into
//! [`FixedRateLeg::coupons`], which keeps the concrete type, and
//! [`FixedRateLeg::build`], which erases it into a [`Leg`]; C++ recovers the
//! concrete type with `dynamic_pointer_cast`, which the port has no counterpart
//! for. Both are fallible: `QL_REQUIRE(!couponRates_.empty())` and its notional
//! twin surface as [`QlResult`] per D4, together with the
//! [`InterestRate`] preconditions the day-counter overrides re-check.
//!
//! `withCouponRates` overloads on four shapes; Rust names them
//! [`with_coupon_rate`](FixedRateLeg::with_coupon_rate),
//! [`with_coupon_rates`](FixedRateLeg::with_coupon_rates),
//! [`with_interest_rate`](FixedRateLeg::with_interest_rate) and
//! [`with_interest_rates`](FixedRateLeg::with_interest_rates). The two that
//! take a bare rate build the [`InterestRate`] eagerly, as C++ does, and so are
//! fallible.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::fixedratecoupon::FixedRateCoupon;
use crate::errors::QlResult;
use crate::interestrate::{Compounding, InterestRate};
use crate::require;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Rate, Real};

/// Builds a sequence of [`FixedRateCoupon`]s from a [`Schedule`].
#[must_use]
pub struct FixedRateLeg {
    schedule: Schedule,
    notionals: Vec<Real>,
    coupon_rates: Vec<InterestRate>,
    first_period_day_counter: Option<DayCounter>,
    last_period_day_counter: Option<DayCounter>,
    payment_calendar: Calendar,
    payment_adjustment: BusinessDayConvention,
    payment_lag: Integer,
    ex_coupon_period: Option<Period>,
    ex_coupon_calendar: Calendar,
    ex_coupon_adjustment: BusinessDayConvention,
    ex_coupon_end_of_month: bool,
}

impl FixedRateLeg {
    /// A leg over `schedule`, paying on the schedule's own calendar with the
    /// `Following` convention and no payment lag.
    pub fn new(schedule: Schedule) -> FixedRateLeg {
        let payment_calendar = schedule.calendar().clone();
        FixedRateLeg {
            schedule,
            notionals: Vec::new(),
            coupon_rates: Vec::new(),
            first_period_day_counter: None,
            last_period_day_counter: None,
            payment_calendar,
            payment_adjustment: BusinessDayConvention::Following,
            payment_lag: 0,
            ex_coupon_period: None,
            ex_coupon_calendar: NullCalendar::new(),
            ex_coupon_adjustment: BusinessDayConvention::Following,
            ex_coupon_end_of_month: false,
        }
    }

    /// One notional for every coupon.
    pub fn with_notional(self, notional: Real) -> FixedRateLeg {
        self.with_notionals(vec![notional])
    }

    /// A notional per coupon; the last one carries over to any coupon beyond
    /// the end of the list.
    pub fn with_notionals(mut self, notionals: Vec<Real>) -> FixedRateLeg {
        self.notionals = notionals;
        self
    }

    /// One rate for every coupon, with its conventions.
    ///
    /// # Errors
    ///
    /// Propagates the [`InterestRate::new`] frequency precondition.
    pub fn with_coupon_rate(
        self,
        rate: Rate,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
    ) -> QlResult<FixedRateLeg> {
        self.with_coupon_rates(vec![rate], day_counter, compounding, frequency)
    }

    /// A rate per coupon, sharing conventions; the last one carries over.
    ///
    /// # Errors
    ///
    /// Propagates the [`InterestRate::new`] frequency precondition.
    pub fn with_coupon_rates(
        self,
        rates: Vec<Rate>,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
    ) -> QlResult<FixedRateLeg> {
        let coupon_rates = rates
            .into_iter()
            .map(|rate| InterestRate::new(rate, day_counter.clone(), compounding, frequency))
            .collect::<QlResult<Vec<_>>>()?;
        Ok(self.with_interest_rates(coupon_rates))
    }

    /// One [`InterestRate`] for every coupon.
    pub fn with_interest_rate(self, interest_rate: InterestRate) -> FixedRateLeg {
        self.with_interest_rates(vec![interest_rate])
    }

    /// An [`InterestRate`] per coupon; the last one carries over.
    pub fn with_interest_rates(mut self, interest_rates: Vec<InterestRate>) -> FixedRateLeg {
        self.coupon_rates = interest_rates;
        self
    }

    /// The convention the payment dates are adjusted with.
    pub fn with_payment_adjustment(mut self, convention: BusinessDayConvention) -> FixedRateLeg {
        self.payment_adjustment = convention;
        self
    }

    /// The calendar the payment dates are adjusted on, overriding the
    /// schedule's.
    pub fn with_payment_calendar(mut self, calendar: Calendar) -> FixedRateLeg {
        self.payment_calendar = calendar;
        self
    }

    /// The number of business days between a coupon's accrual end and its
    /// payment.
    pub fn with_payment_lag(mut self, lag: Integer) -> FixedRateLeg {
        self.payment_lag = lag;
        self
    }

    /// A day counter for the first coupon only, overriding the rate's.
    pub fn with_first_period_day_counter(mut self, day_counter: DayCounter) -> FixedRateLeg {
        self.first_period_day_counter = Some(day_counter);
        self
    }

    /// A day counter for the last coupon only, overriding the rate's.
    pub fn with_last_period_day_counter(mut self, day_counter: DayCounter) -> FixedRateLeg {
        self.last_period_day_counter = Some(day_counter);
        self
    }

    /// The ex-coupon period, measured back from each payment date.
    pub fn with_ex_coupon_period(
        mut self,
        period: Period,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
    ) -> FixedRateLeg {
        self.ex_coupon_period = Some(period);
        self.ex_coupon_calendar = calendar;
        self.ex_coupon_adjustment = convention;
        self.ex_coupon_end_of_month = end_of_month;
        self
    }

    /// The coupons the leg is made of.
    ///
    /// # Errors
    ///
    /// Errors if no coupon rate or no notional was given, if the schedule holds
    /// fewer than two dates, or if a period day-counter override does not
    /// satisfy the [`InterestRate::new`] precondition.
    pub fn coupons(&self) -> QlResult<Vec<Shared<FixedRateCoupon>>> {
        require!(!self.coupon_rates.is_empty(), "no coupon rates given");
        require!(!self.notionals.is_empty(), "no notional given");
        let size = self.schedule.len();
        require!(size >= 2, "schedule with {size} date(s) spans no period");

        let mut coupons = Vec::with_capacity(size - 1);

        let mut start = self.schedule.date(0);
        let mut end = self.schedule.date(1);
        let stub = self.schedule.has_tenor()
            && self.schedule.has_is_regular()
            && !self.schedule.is_regular_at(1);
        let reference_start = if stub {
            self.schedule.calendar().advance_by_period(
                end,
                -self.schedule.tenor(),
                self.schedule.business_day_convention(),
                self.schedule.end_of_month(),
            )
        } else {
            start
        };
        coupons.push(self.coupon(
            0,
            start,
            end,
            reference_start,
            end,
            self.first_period_day_counter.as_ref(),
        )?);

        for i in 2..size - 1 {
            start = end;
            end = self.schedule.date(i);
            coupons.push(self.coupon(i - 1, start, end, start, end, None)?);
        }

        if size > 2 {
            start = end;
            end = self.schedule.date(size - 1);
            let regular = (self.schedule.has_is_regular() && self.schedule.is_regular_at(size - 1))
                || !self.schedule.has_tenor();
            let reference_end = if regular {
                end
            } else {
                self.schedule.calendar().advance_by_period(
                    start,
                    self.schedule.tenor(),
                    self.schedule.business_day_convention(),
                    self.schedule.end_of_month(),
                )
            };
            coupons.push(self.coupon(
                size - 2,
                start,
                end,
                start,
                reference_end,
                self.last_period_day_counter.as_ref(),
            )?);
        }

        Ok(coupons)
    }

    /// The coupons as a [`Leg`], with their concrete type erased.
    ///
    /// # Errors
    ///
    /// As [`coupons`](Self::coupons).
    pub fn build(&self) -> QlResult<Leg> {
        Ok(self
            .coupons()?
            .into_iter()
            .map(|coupon| coupon as Shared<dyn CashFlow>)
            .collect())
    }

    fn coupon(
        &self,
        index: usize,
        start: Date,
        end: Date,
        reference_start: Date,
        reference_end: Date,
        day_counter: Option<&DayCounter>,
    ) -> QlResult<Shared<FixedRateCoupon>> {
        let payment_date = self.payment_calendar.advance(
            end,
            self.payment_lag,
            TimeUnit::Days,
            self.payment_adjustment,
            false,
        );
        let ex_coupon_date = self.ex_coupon_period.map(|period| {
            self.ex_coupon_calendar.advance_by_period(
                payment_date,
                -period,
                self.ex_coupon_adjustment,
                self.ex_coupon_end_of_month,
            )
        });
        let rate = at(&self.coupon_rates, index);
        let rate = match day_counter {
            Some(day_counter) => InterestRate::new(
                rate.rate(),
                day_counter.clone(),
                rate.compounding(),
                rate.frequency(),
            )?,
            None => rate,
        };
        Ok(shared(FixedRateCoupon::new(
            payment_date,
            at(&self.notionals, index),
            rate,
            start,
            end,
            Some(reference_start),
            Some(reference_end),
            ex_coupon_date,
        )))
    }
}

/// The `index`-th value, or the last one when the list is shorter.
///
/// # Panics
///
/// Panics on an empty list, which [`FixedRateLeg::coupons`] rules out first.
fn at<T: Clone>(values: &[T], index: usize) -> T {
    values
        .get(index)
        .unwrap_or_else(|| values.last().expect("non-empty by precondition"))
        .clone()
}
