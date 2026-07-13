//! The ibor leg builder.
//!
//! Port of the `IborLeg` half of `ql/cashflows/iborcoupon.{hpp,cpp}`, the
//! floating analogue of [`FixedRateLeg`](super::FixedRateLeg): a fluent builder
//! turning a [`Schedule`] plus notionals, fixing days, gearings and spreads into
//! a sequence of [`IborCoupon`]s over an [`IborIndex`]. The first and last
//! periods may be short or long, in which case the coupon accrues against a
//! reference period one tenor away from the stub, so a schedule-aware day counter
//! still sees a regular period.
//!
//! The C++ class delegates to the generic `FloatingLeg` template in
//! `cashflowvectors.hpp`; this port reproduces that template's loop rather than
//! the [`FixedRateLeg`](super::FixedRateLeg) operator's, so the stub reference
//! dates match `IborLeg`, not the fixed leg. The two agree on every multi-coupon
//! schedule and differ only when a single-coupon schedule is irregular at both
//! ends, where the template adjusts both reference bounds and the fixed operator
//! only the first.
//!
//! ## Divergences from QuantLib
//!
//! C++ ends the builder with `operator Leg()`. The port splits that into
//! [`IborLeg::coupons`], which keeps the concrete [`IborCoupon`] type, and
//! [`IborLeg::build`], which erases it into a [`Leg`]; C++ recovers the concrete
//! type with `dynamic_pointer_cast`, which the port has no counterpart for. The
//! default [`BlackIborCouponPricer`] that `operator Leg()` attaches when no
//! caps, floors or in-arrears feature is present is attached in
//! [`coupons`](IborLeg::coupons); since those features are deferred the condition
//! always holds. C++ attaches it through the free `setCouponPricer(leg, pricer)`,
//! which downcasts each flow; the port's [`set_coupon_pricer`] takes the concrete
//! coupons instead, the erased [`Leg`] carrying no downcast.
//!
//! The stub reference date uses `calendar.adjust(end - tenor, bdc)` as the
//! `FloatingLeg` template does, ignoring the schedule's end-of-month flag; the
//! fixed leg passes that flag through. The two agree whenever the flag is unset
//! or `false`.
//!
//! ## Deferred (later sub-tickets of #69)
//!
//! Caps and floors (`withCaps`/`withFloors`, which yield `CappedFlooredIborCoupon`
//! through a `BlackIborCouponPricer` optionlet path), the in-arrears convexity
//! adjustment (`inArrears`), zero and indexed-coupon modes, digital and CMS
//! coupons and the overnight-indexed leg. Their builder methods are omitted
//! entirely rather than accepted and ignored. A zero gearing, which the template
//! collapses to a `FixedRateCoupon`, is likewise not special-cased: the port's
//! [`IborCoupon`] rejects it, so `with_gearing(0.0)` surfaces that error rather
//! than a silent fixed coupon.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::couponpricer::{BlackIborCouponPricer, FloatingRateCouponPricer};
use crate::cashflows::iborcoupon::IborCoupon;
use crate::errors::QlResult;
use crate::indexes::iborindex::IborIndex;
use crate::indexes::interestrateindex::InterestRateIndex;
use crate::require;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::calendars::nullcalendar::NullCalendar;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Real, Spread};

/// Builds a sequence of [`IborCoupon`]s from a [`Schedule`].
#[must_use]
pub struct IborLeg {
    schedule: Schedule,
    index: Shared<IborIndex>,
    notionals: Vec<Real>,
    payment_day_counter: Option<DayCounter>,
    payment_adjustment: BusinessDayConvention,
    payment_lag: Integer,
    payment_calendar: Calendar,
    fixing_days: Vec<Natural>,
    gearings: Vec<Real>,
    spreads: Vec<Spread>,
    fixing_convention: BusinessDayConvention,
    ex_coupon_period: Option<Period>,
    ex_coupon_calendar: Calendar,
    ex_coupon_adjustment: BusinessDayConvention,
    ex_coupon_end_of_month: bool,
}

impl IborLeg {
    /// A leg over `schedule` paying `index`, on the schedule's own calendar with
    /// the `Following` convention, no payment lag, and the `Preceding` fixing
    /// convention.
    pub fn new(schedule: Schedule, index: Shared<IborIndex>) -> IborLeg {
        let payment_calendar = schedule.calendar().clone();
        IborLeg {
            schedule,
            index,
            notionals: Vec::new(),
            payment_day_counter: None,
            payment_adjustment: BusinessDayConvention::Following,
            payment_lag: 0,
            payment_calendar,
            fixing_days: Vec::new(),
            gearings: Vec::new(),
            spreads: Vec::new(),
            fixing_convention: BusinessDayConvention::Preceding,
            ex_coupon_period: None,
            ex_coupon_calendar: NullCalendar::new(),
            ex_coupon_adjustment: BusinessDayConvention::Following,
            ex_coupon_end_of_month: false,
        }
    }

    /// One notional for every coupon.
    pub fn with_notional(self, notional: Real) -> IborLeg {
        self.with_notionals(vec![notional])
    }

    /// A notional per coupon; the last one carries over to any coupon beyond the
    /// end of the list.
    pub fn with_notionals(mut self, notionals: Vec<Real>) -> IborLeg {
        self.notionals = notionals;
        self
    }

    /// The day counter the coupons accrue with, overriding the index's.
    pub fn with_payment_day_counter(mut self, day_counter: DayCounter) -> IborLeg {
        self.payment_day_counter = Some(day_counter);
        self
    }

    /// The convention the payment dates are adjusted with.
    pub fn with_payment_adjustment(mut self, convention: BusinessDayConvention) -> IborLeg {
        self.payment_adjustment = convention;
        self
    }

    /// The number of business days between a coupon's accrual end and its
    /// payment.
    pub fn with_payment_lag(mut self, lag: Integer) -> IborLeg {
        self.payment_lag = lag;
        self
    }

    /// The calendar the payment dates are adjusted on, overriding the schedule's.
    pub fn with_payment_calendar(mut self, calendar: Calendar) -> IborLeg {
        self.payment_calendar = calendar;
        self
    }

    /// One fixing-days count for every coupon, overriding the index's.
    pub fn with_fixing_days(self, fixing_days: Natural) -> IborLeg {
        self.with_fixing_days_per_coupon(vec![fixing_days])
    }

    /// A fixing-days count per coupon; the last one carries over.
    pub fn with_fixing_days_per_coupon(mut self, fixing_days: Vec<Natural>) -> IborLeg {
        self.fixing_days = fixing_days;
        self
    }

    /// One gearing for every coupon.
    pub fn with_gearing(self, gearing: Real) -> IborLeg {
        self.with_gearings(vec![gearing])
    }

    /// A gearing per coupon; the last one carries over.
    pub fn with_gearings(mut self, gearings: Vec<Real>) -> IborLeg {
        self.gearings = gearings;
        self
    }

    /// One spread for every coupon.
    pub fn with_spread(self, spread: Spread) -> IborLeg {
        self.with_spreads(vec![spread])
    }

    /// A spread per coupon; the last one carries over.
    pub fn with_spreads(mut self, spreads: Vec<Spread>) -> IborLeg {
        self.spreads = spreads;
        self
    }

    /// The convention the fixing dates are computed with.
    pub fn with_fixing_convention(mut self, convention: BusinessDayConvention) -> IborLeg {
        self.fixing_convention = convention;
        self
    }

    /// The ex-coupon period, measured back from each payment date.
    pub fn with_ex_coupon_period(
        mut self,
        period: Period,
        calendar: Calendar,
        convention: BusinessDayConvention,
        end_of_month: bool,
    ) -> IborLeg {
        self.ex_coupon_period = Some(period);
        self.ex_coupon_calendar = calendar;
        self.ex_coupon_adjustment = convention;
        self.ex_coupon_end_of_month = end_of_month;
        self
    }

    /// The coupons the leg is made of, each carrying the default
    /// [`BlackIborCouponPricer`].
    ///
    /// # Errors
    ///
    /// Errors if no notional was given, if the schedule holds fewer than two
    /// dates, if more notionals, gearings or spreads were given than the schedule
    /// has periods, or if a coupon fails its [`IborCoupon::new`] preconditions (a
    /// zero gearing among them).
    pub fn coupons(&self) -> QlResult<Vec<Shared<IborCoupon>>> {
        require!(!self.notionals.is_empty(), "no notional given");
        let size = self.schedule.len();
        require!(size >= 2, "schedule with {size} date(s) spans no period");
        let periods = size - 1;
        require!(
            self.notionals.len() <= periods,
            "too many notionals ({}), only {periods} required",
            self.notionals.len()
        );
        require!(
            self.gearings.len() <= periods,
            "too many gearings ({}), only {periods} required",
            self.gearings.len()
        );
        require!(
            self.spreads.len() <= periods,
            "too many spreads ({}), only {periods} required",
            self.spreads.len()
        );

        let calendar = self.schedule.calendar();
        let convention = self.schedule.business_day_convention();
        let stub = |period: usize| {
            self.schedule.has_tenor()
                && self.schedule.has_is_regular()
                && !self.schedule.is_regular_at(period)
        };

        let mut coupons = Vec::with_capacity(periods);
        for i in 0..periods {
            let start = self.schedule.date(i);
            let end = self.schedule.date(i + 1);
            let mut reference_start = start;
            let mut reference_end = end;
            if i == 0 && stub(1) {
                reference_start =
                    calendar.advance_by_period(end, -self.schedule.tenor(), convention, false);
            }
            if i == periods - 1 && stub(i + 1) {
                reference_end =
                    calendar.advance_by_period(start, self.schedule.tenor(), convention, false);
            }
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
            let fixing_days = if self.fixing_days.is_empty() {
                None
            } else {
                Some(broadcast(&self.fixing_days, i, self.index.fixing_days()))
            };
            let coupon = IborCoupon::new(
                payment_date,
                broadcast(&self.notionals, i, 1.0),
                start,
                end,
                fixing_days,
                self.index.clone(),
                broadcast(&self.gearings, i, 1.0),
                broadcast(&self.spreads, i, 0.0),
                Some(reference_start),
                Some(reference_end),
                self.payment_day_counter.clone(),
                false,
                ex_coupon_date,
                self.fixing_convention,
            )?;
            let coupon = shared(coupon);
            coupon.set_pricer(default_pricer());
            coupons.push(coupon);
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
}

/// Attaches `pricer` to every coupon, overriding the default the builder set.
///
/// The free `setCouponPricer(Leg&, pricer)` of `couponpricer.cpp`, taking the
/// concrete coupons rather than an erased [`Leg`] since the port cannot downcast
/// a [`CashFlow`](crate::cashflow::CashFlow) back to a coupon.
pub fn set_coupon_pricer(
    coupons: &[Shared<IborCoupon>],
    pricer: SharedMut<dyn FloatingRateCouponPricer>,
) {
    for coupon in coupons {
        coupon.set_pricer(pricer.clone());
    }
}

/// The default coupon pricer `operator Leg()` attaches.
fn default_pricer() -> SharedMut<dyn FloatingRateCouponPricer> {
    shared_mut(BlackIborCouponPricer::new()) as SharedMut<dyn FloatingRateCouponPricer>
}

/// The `index`-th value, the last one when the list is shorter, or `default`
/// when the list is empty (`detail::get`).
fn broadcast<T: Clone>(values: &[T], index: usize, default: T) -> T {
    match values.last() {
        None => default,
        Some(last) => values.get(index).unwrap_or(last).clone(),
    }
}
