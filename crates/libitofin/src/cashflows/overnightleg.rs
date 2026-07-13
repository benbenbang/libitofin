//! The overnight leg builder.
//!
//! Port of the `OvernightLeg` half of `ql/cashflows/overnightindexedcoupon.{hpp,cpp}`
//! (the builder shares the coupon's header at `overnightindexedcoupon.hpp:215`, not
//! a `cashflowvectors`-style file). A fluent builder turning a [`Schedule`] plus
//! notionals, gearings and spreads into a sequence of
//! [`OvernightIndexedCoupon`]s over an [`OvernightIndex`], the overnight analogue
//! of [`IborLeg`](super::IborLeg). The first and last periods may be a short or
//! long stub, in which case the coupon accrues against a reference period one
//! tenor away from the stub so a schedule-aware day counter still sees a regular
//! period.
//!
//! This port reproduces the `operator Leg()` loop of `overnightindexedcoupon.cpp:564`
//! rather than [`IborLeg`](super::IborLeg)'s `FloatingLeg` template: the two build
//! loops differ, and the stub reference dates here are adjusted with the builder's
//! own payment convention (`paymentAdjustment_`), not the schedule's convention.
//!
//! ## The pricer: no `withCouponPricer`
//!
//! Unlike [`IborLeg`](super::IborLeg), C++ `OvernightLeg` carries a pricer builder,
//! `withCouponPricer(const ext::shared_ptr<OvernightIndexedCouponPricer>&)`
//! (`overnightindexedcoupon.hpp:245`), applied at `operator Leg()`
//! (`overnightindexedcoupon.cpp:668`) only when one was supplied. This port omits
//! that builder method, for reasons rooted in the ported surface:
//!
//! - Its only caller in the test suite is the caps/floors path
//!   (`overnightindexedcoupon.cpp:246-252` of the test fixture), which builds a
//!   `CappedFlooredOvernightIndexedCoupon` through a `Black...` pricer. Both the
//!   capped coupon and those pricers are deferred (see below), so no ported call
//!   ever supplies a pricer.
//! - [`OvernightIndexedCoupon`] installs its own
//!   [`CompoundingOvernightIndexedCouponPricer`] in its constructor and holds a
//!   reference to it in a private field that its rate-bearing inspectors
//!   (`accrued_amount`, `effective_spread`) read directly. Overriding that pricer
//!   after construction would desync that field from the embedded
//!   [`FloatingRateCoupon`](super::floatingratecoupon::FloatingRateCoupon)'s
//!   pricer, so the coupon exposes no override hook and none can be added without
//!   touching the coupon (a separate ticket's file).
//!
//! When the caps/floors coupon and its Black pricers are ported, `withCouponPricer`
//! lands with them, applied only when supplied so the constructor default stands
//! otherwise.
//!
//! ## Divergences from QuantLib
//!
//! C++ ends the builder with `operator Leg()`. The port splits that into
//! [`OvernightLeg::coupons`], which keeps the concrete [`OvernightIndexedCoupon`]
//! type, and [`OvernightLeg::build`], which erases it into a [`Leg`]. Each coupon
//! carries the compounding pricer its own constructor installed; the leg attaches
//! none.
//!
//! ## Deferred (later sub-tickets of #69)
//!
//! Caps and floors (`withCaps`/`withFloors`/`withNakedOption`/`withDailyCapFloor`,
//! which build a `CappedFlooredOvernightIndexedCoupon`), and with them the
//! in-advance default (`inArrears_` defaults to `true` in C++, and the reference
//! computation dates it drives), the lookback, lockout and observation-shift
//! knobs, telescopic value dates, rounding precision, the last-recent-period knob
//! and explicit payment dates. Their builder methods are omitted entirely rather
//! than accepted and ignored, since [`OvernightIndexedCoupon`] does not accept the
//! corresponding constructor arguments. A zero gearing, which C++ collapses to a
//! `FixedRateCoupon`, is likewise not special-cased: the port's coupon rejects it,
//! so `with_gearing(0.0)` surfaces that error rather than a silent fixed coupon.
//! [`RateAveraging::Simple`](super::rateaveraging::RateAveraging::Simple) may be
//! set, but the coupon refuses it at construction, so the error surfaces at
//! [`build`](OvernightLeg::build).

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::overnightindexedcoupon::OvernightIndexedCoupon;
use crate::cashflows::rateaveraging::RateAveraging;
use crate::errors::QlResult;
use crate::indexes::iborindex::OvernightIndex;
use crate::require;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::daycounter::DayCounter;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Real, Spread};

/// Builds a sequence of [`OvernightIndexedCoupon`]s from a [`Schedule`].
#[must_use]
pub struct OvernightLeg {
    schedule: Schedule,
    index: Shared<OvernightIndex>,
    notionals: Vec<Real>,
    payment_day_counter: Option<DayCounter>,
    payment_adjustment: BusinessDayConvention,
    payment_lag: Integer,
    payment_calendar: Calendar,
    gearings: Vec<Real>,
    spreads: Vec<Spread>,
    averaging_method: RateAveraging,
    compound_spread_daily: bool,
}

impl OvernightLeg {
    /// A leg over `schedule` paying `index`, on the schedule's own calendar with
    /// the `Following` convention, no payment lag, and compound averaging.
    pub fn new(schedule: Schedule, index: Shared<OvernightIndex>) -> OvernightLeg {
        let payment_calendar = schedule.calendar().clone();
        OvernightLeg {
            schedule,
            index,
            notionals: Vec::new(),
            payment_day_counter: None,
            payment_adjustment: BusinessDayConvention::Following,
            payment_lag: 0,
            payment_calendar,
            gearings: Vec::new(),
            spreads: Vec::new(),
            averaging_method: RateAveraging::Compound,
            compound_spread_daily: false,
        }
    }

    /// One notional for every coupon.
    pub fn with_notional(self, notional: Real) -> OvernightLeg {
        self.with_notionals(vec![notional])
    }

    /// A notional per coupon; the last one carries over to any coupon beyond the
    /// end of the list.
    pub fn with_notionals(mut self, notionals: Vec<Real>) -> OvernightLeg {
        self.notionals = notionals;
        self
    }

    /// The day counter the coupons accrue with, overriding the index's.
    pub fn with_payment_day_counter(mut self, day_counter: DayCounter) -> OvernightLeg {
        self.payment_day_counter = Some(day_counter);
        self
    }

    /// The convention the payment dates and stub reference dates are adjusted with.
    pub fn with_payment_adjustment(mut self, convention: BusinessDayConvention) -> OvernightLeg {
        self.payment_adjustment = convention;
        self
    }

    /// The number of business days between a coupon's accrual end and its
    /// payment.
    pub fn with_payment_lag(mut self, lag: Integer) -> OvernightLeg {
        self.payment_lag = lag;
        self
    }

    /// The calendar the payment dates are adjusted on, overriding the schedule's.
    pub fn with_payment_calendar(mut self, calendar: Calendar) -> OvernightLeg {
        self.payment_calendar = calendar;
        self
    }

    /// One gearing for every coupon.
    pub fn with_gearing(self, gearing: Real) -> OvernightLeg {
        self.with_gearings(vec![gearing])
    }

    /// A gearing per coupon; the last one carries over.
    pub fn with_gearings(mut self, gearings: Vec<Real>) -> OvernightLeg {
        self.gearings = gearings;
        self
    }

    /// One spread for every coupon.
    pub fn with_spread(self, spread: Spread) -> OvernightLeg {
        self.with_spreads(vec![spread])
    }

    /// A spread per coupon; the last one carries over.
    pub fn with_spreads(mut self, spreads: Vec<Spread>) -> OvernightLeg {
        self.spreads = spreads;
        self
    }

    /// The averaging method. Only
    /// [`RateAveraging::Compound`](super::rateaveraging::RateAveraging::Compound)
    /// is supported by the coupon; setting
    /// [`Simple`](super::rateaveraging::RateAveraging::Simple) surfaces an error at
    /// [`build`](Self::build).
    pub fn with_averaging_method(mut self, averaging_method: RateAveraging) -> OvernightLeg {
        self.averaging_method = averaging_method;
        self
    }

    /// Whether the spread is compounded with each daily fixing rather than added
    /// after compounding (`compoundingSpreadDaily`).
    pub fn with_compound_spread_daily(mut self, compound_spread_daily: bool) -> OvernightLeg {
        self.compound_spread_daily = compound_spread_daily;
        self
    }

    /// The coupons the leg is made of, each carrying the compounding pricer its own
    /// constructor installed.
    ///
    /// # Errors
    ///
    /// Errors if no notional was given, if the schedule holds fewer than two dates,
    /// if more notionals, gearings or spreads were given than the schedule has
    /// periods, or if a coupon fails its [`OvernightIndexedCoupon::new`]
    /// preconditions (a zero gearing and simple averaging among them).
    pub fn coupons(&self) -> QlResult<Vec<Shared<OvernightIndexedCoupon>>> {
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
                reference_start = calendar.advance_by_period(
                    end,
                    -self.schedule.tenor(),
                    self.payment_adjustment,
                    false,
                );
            }
            if i == periods - 1 && stub(i + 1) {
                reference_end = calendar.advance_by_period(
                    start,
                    self.schedule.tenor(),
                    self.payment_adjustment,
                    false,
                );
            }
            let payment_date = self.payment_calendar.advance(
                end,
                self.payment_lag,
                TimeUnit::Days,
                self.payment_adjustment,
                false,
            );
            let coupon = OvernightIndexedCoupon::new(
                payment_date,
                broadcast(&self.notionals, i, 1.0),
                start,
                end,
                self.index.clone(),
                broadcast(&self.gearings, i, 1.0),
                broadcast(&self.spreads, i, 0.0),
                Some(reference_start),
                Some(reference_end),
                self.payment_day_counter.clone(),
                self.averaging_method,
                self.compound_spread_daily,
                None,
            )?;
            coupons.push(shared(coupon));
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

/// The `index`-th value, the last one when the list is shorter, or `default`
/// when the list is empty (`detail::get`).
fn broadcast<T: Clone>(values: &[T], index: usize, default: T) -> T {
    match values.last() {
        None => default,
        Some(last) => values.get(index).unwrap_or(last).clone(),
    }
}
