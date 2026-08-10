//! The year-on-year inflation leg builder.
//!
//! Port of the `yoyInflationLeg` half of `ql/cashflows/yoyinflationcoupon.hpp`
//! (`:97-132`) and `.cpp` (`:67-232`): a fluent builder turning a [`Schedule`]
//! plus notionals, fixing days, gearings and spreads into a sequence of
//! [`YoYInflationCoupon`]s over a [`YoYInflationIndex`], mirroring
//! [`IborLeg`](super::IborLeg). The first and last periods may be short or long,
//! in which case the coupon accrues against a reference period one tenor away
//! from the stub, so a schedule-aware day counter still sees a regular period
//! and the observation lag still runs off a full year-on-year window.
//!
//! The leg carries two calendars, which are not interchangeable: the payment
//! dates are rolled on the builder's own `payment_calendar` under
//! [`with_payment_adjustment`](YoYInflationLeg::with_payment_adjustment)
//! (`.cpp:176`), while the stub reference dates are rolled on the *schedule's*
//! calendar under the schedule's own convention (`.cpp:177-184`).
//!
//! ## Divergences from QuantLib
//!
//! C++ ends the builder with `operator Leg()`. The port splits that into
//! [`YoYInflationLeg::coupons`], which keeps the concrete
//! [`YoYInflationCoupon`] type, and [`YoYInflationLeg::build`], which erases it
//! into a [`Leg`]; C++ recovers the concrete type with `dynamic_pointer_cast`,
//! which the port has no counterpart for.
//!
//! `operator Leg()` attaches the default swaplet pricer under the guard
//! `caps_.empty() && floors_.empty()` (`.cpp:228-229`). With the cap and floor
//! fields deferred that guard is vacuously true, so the port attaches
//! unconditionally in [`coupons`](YoYInflationLeg::coupons) rather than carrying
//! always-empty vectors to evaluate it against.
//!
//! The stub reference date uses `calendar.advance_by_period(end, -tenor, bdc)`
//! where C++ writes `calendar.adjust(end - tenor, bdc)` (`.cpp:179`), as
//! [`IborLeg`](super::IborLeg) does. The two agree for a tenor in months or
//! years and differ only for one in days, which advances over business days.
//!
//! ## Deferred (visible)
//!
//! Caps and floors (`withCaps`/`withFloors`, `hpp:112-115`) and the
//! `CappedFlooredYoYInflationCoupon` branch of the loop (`.cpp:207-222`) belong
//! to `#838`. Their builder methods are omitted entirely rather than accepted
//! and ignored, so a capped leg cannot be silently priced as a swaplet one.
//!
//! The zero-gearing collapse to a `FixedRateCoupon` (`.cpp:185-192`) is also
//! deferred, but by *accepting* the gearing rather than by rejecting it - the
//! opposite of [`IborLeg`](super::IborLeg), whose coupon refuses a zero gearing
//! outright. A zero-geared [`YoYInflationCoupon`] pays
//! `nominal * accrual * (0 * fixing + spread)`, which is what the C++
//! `FixedRateCoupon` pays at `effectiveFixedRate = spread` under simple
//! compounding, so the two agree on every amount the port can produce. They
//! differ in what they refuse: the C++ fixed coupon reads no index at all,
//! whereas the port's pricer still resolves the fixing and propagates a missing
//! one as an error.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::yoyinflationcoupon::{
    SwapletYoYInflationCouponPricer, YoYInflationCoupon, YoYInflationCouponPricer,
};
use crate::errors::QlResult;
use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::daycounter::DayCounter;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Real, Spread};
use crate::{fail, require};

/// Builds a sequence of [`YoYInflationCoupon`]s from a [`Schedule`].
#[must_use]
pub struct YoYInflationLeg {
    schedule: Schedule,
    payment_calendar: Calendar,
    yoy_index: Shared<YoYInflationIndex>,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    notionals: Vec<Real>,
    payment_day_counter: Option<DayCounter>,
    payment_adjustment: BusinessDayConvention,
    fixing_days: Vec<Natural>,
    gearings: Vec<Real>,
    spreads: Vec<Spread>,
}

impl YoYInflationLeg {
    /// A leg over `schedule` paying `yoy_index` observed `observation_lag` back
    /// under `interpolation`, paying on `payment_calendar` with the
    /// `ModifiedFollowing` convention (`.cpp:67-73`, the default of
    /// `hpp:126`).
    pub fn new(
        schedule: Schedule,
        payment_calendar: Calendar,
        yoy_index: Shared<YoYInflationIndex>,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
    ) -> YoYInflationLeg {
        YoYInflationLeg {
            schedule,
            payment_calendar,
            yoy_index,
            observation_lag,
            interpolation,
            notionals: Vec::new(),
            payment_day_counter: None,
            payment_adjustment: BusinessDayConvention::ModifiedFollowing,
            fixing_days: Vec::new(),
            gearings: Vec::new(),
            spreads: Vec::new(),
        }
    }

    /// One notional for every coupon.
    pub fn with_notional(self, notional: Real) -> YoYInflationLeg {
        self.with_notionals(vec![notional])
    }

    /// A notional per coupon; the last one carries over to any coupon beyond the
    /// end of the list.
    pub fn with_notionals(mut self, notionals: Vec<Real>) -> YoYInflationLeg {
        self.notionals = notionals;
        self
    }

    /// The day counter the coupons accrue with. The leg has no default: a
    /// coupon cannot be built without one.
    pub fn with_payment_day_counter(mut self, day_counter: DayCounter) -> YoYInflationLeg {
        self.payment_day_counter = Some(day_counter);
        self
    }

    /// The convention the payment dates are adjusted with.
    pub fn with_payment_adjustment(mut self, convention: BusinessDayConvention) -> YoYInflationLeg {
        self.payment_adjustment = convention;
        self
    }

    /// One fixing-days count for every coupon.
    pub fn with_fixing_days(self, fixing_days: Natural) -> YoYInflationLeg {
        self.with_fixing_days_per_coupon(vec![fixing_days])
    }

    /// A fixing-days count per coupon; the last one carries over.
    pub fn with_fixing_days_per_coupon(mut self, fixing_days: Vec<Natural>) -> YoYInflationLeg {
        self.fixing_days = fixing_days;
        self
    }

    /// One gearing for every coupon.
    pub fn with_gearing(self, gearing: Real) -> YoYInflationLeg {
        self.with_gearings(vec![gearing])
    }

    /// A gearing per coupon; the last one carries over.
    pub fn with_gearings(mut self, gearings: Vec<Real>) -> YoYInflationLeg {
        self.gearings = gearings;
        self
    }

    /// One spread for every coupon.
    pub fn with_spread(self, spread: Spread) -> YoYInflationLeg {
        self.with_spreads(vec![spread])
    }

    /// A spread per coupon; the last one carries over.
    pub fn with_spreads(mut self, spreads: Vec<Spread>) -> YoYInflationLeg {
        self.spreads = spreads;
        self
    }

    /// The coupons the leg is made of, each carrying the default
    /// [`SwapletYoYInflationCouponPricer`] (`.cpp:228-229`).
    ///
    /// That pricer holds no nominal curve, so the coupons answer
    /// [`rate`](crate::cashflows::Coupon::rate) and
    /// [`amount`](crate::cashflows::Coupon::amount) but refuse a
    /// [`swaplet_price`](YoYInflationCouponPricer::swaplet_price) until a
    /// discounting pricer replaces it.
    ///
    /// # Errors
    ///
    /// Errors if no payment day counter or no notional was given, if the
    /// schedule holds fewer than two dates, or if more notionals, gearings or
    /// spreads were given than the schedule has periods.
    pub fn coupons(&self) -> QlResult<Vec<Shared<YoYInflationCoupon>>> {
        let Some(payment_day_counter) = &self.payment_day_counter else {
            fail!("no payment daycounter given");
        };
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
            let payment_date = self.payment_calendar.adjust(end, self.payment_adjustment);
            let coupon = YoYInflationCoupon::new(
                payment_date,
                broadcast(&self.notionals, i, 1.0),
                start,
                end,
                broadcast(&self.fixing_days, i, 0),
                Shared::clone(&self.yoy_index),
                self.observation_lag,
                self.interpolation,
                payment_day_counter.clone(),
                broadcast(&self.gearings, i, 1.0),
                broadcast(&self.spreads, i, 0.0),
                Some(reference_start),
                Some(reference_end),
            );
            coupon.set_pricer(default_pricer());
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

/// The default coupon pricer `operator Leg()` attaches (`.cpp:229`).
fn default_pricer() -> SharedMut<dyn YoYInflationCouponPricer> {
    shared_mut(SwapletYoYInflationCouponPricer::new()) as SharedMut<dyn YoYInflationCouponPricer>
}

/// The `index`-th value, the last one when the list is shorter, or `default`
/// when the list is empty (`detail::get`).
fn broadcast<T: Clone>(values: &[T], index: usize, default: T) -> T {
    match values.last() {
        None => default,
        Some(last) => values.get(index).unwrap_or(last).clone(),
    }
}
