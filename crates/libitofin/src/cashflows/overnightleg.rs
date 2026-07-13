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

#[cfg(test)]
mod tests {
    //! The `OvernightLeg` cases of `test-suite/overnightindexedcoupon.cpp` that the
    //! ported (plain compound) scope can express:
    //! `testOvernightLegBasicFunctionality` (:920),
    //! `testOvernightLegWithGearingsAndSpreads` (:999) and
    //! `testOvernightLegErrorConditions` (:1094).
    //!
    //! Two of them read surfaces this stack has not ported, so they are reproduced
    //! through equivalent ported observables rather than copied assertion for
    //! assertion:
    //!
    //! - The basic case asserts `lockoutDays() == 0` and
    //!   `applyObservationShift() == false`; [`OvernightIndexedCoupon`] ships
    //!   neither inspector (both knobs are deferred and constant), so those two
    //!   assertions are dropped and the structural rest kept.
    //! - The gearings/spreads case asserts `gearing()` and `spread()` per coupon;
    //!   the coupon exposes neither accessor, so the broadcast is proved instead
    //!   through `rate()`: against a plain leg, a per-coupon gearing scales the rate
    //!   and a per-coupon spread shifts it, both exactly, over an all-forecast leg.
    //!
    //! `testOvernightLegNPV` (:1022) is not reproduced here: its fixture uses
    //! `makeLeg(Null, 3, false, true, ...)`, i.e. three lockout days and telescopic
    //! value dates (both deferred), over an `InterpolatedZeroCurve<Cubic>` (the
    //! stack's zero curve is linear-only), so its published number cannot be
    //! reproduced with the ported surface. The leg's discount-and-sum over
    //! already-oracle'd coupon amounts lands with the OIS swap (#8.10), as the
    //! issue body notes.

    use super::*;
    use crate::cashflows::coupon::Coupon;
    use crate::handle::RelinkableHandle;
    use crate::indexes::ibor::Sofr;
    use crate::interestrate::Compounding;
    use crate::settings::Settings;
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::types::Rate;

    const NOTIONAL: Real = 1_000_000.0;

    /// The C++ `CommonVarsONLeg` reduced to the ported surface: an evaluation date,
    /// a SOFR index over a relinkable forecast curve, and a quarterly schedule from
    /// 1 July 2025 to 1 July 2026 (four regular periods, no stub). The fixing
    /// history is omitted: every test here builds an all-forecast leg.
    fn common_vars(
        today: Date,
    ) -> (
        Shared<Settings<Date>>,
        RelinkableHandle<dyn YieldTermStructure>,
        Shared<OvernightIndex>,
        Schedule,
    ) {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let curve: RelinkableHandle<dyn YieldTermStructure> = RelinkableHandle::empty();
        let sofr = shared(Sofr::new(curve.handle(), settings.clone()));
        let schedule = MakeSchedule::new()
            .from(Date::new(1, Month::July, 2025))
            .to(Date::new(1, Month::July, 2026))
            .with_frequency(Frequency::Quarterly)
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .forwards()
            .build();
        (settings, curve, sofr, schedule)
    }

    /// The C++ `flatRate(rate, Actual360())`: a flat continuously-compounded
    /// forward curve anchored at the evaluation date.
    fn flat_rate(reference: Date, rate: Rate) -> Shared<dyn YieldTermStructure> {
        shared(FlatForward::with_rate(
            reference,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    }

    fn base_leg(schedule: Schedule, sofr: Shared<OvernightIndex>) -> OvernightLeg {
        OvernightLeg::new(schedule, sofr)
            .with_notional(NOTIONAL)
            .with_payment_day_counter(Actual360::new())
    }

    /// `testOvernightLegBasicFunctionality` (:920): the quarterly schedule builds
    /// four overnight coupons, each on the leg notional and compound-averaged.
    #[test]
    fn a_quarterly_schedule_builds_four_compound_coupons() {
        let (_settings, _curve, sofr, schedule) = common_vars(Date::new(1, Month::June, 2025));
        let leg = base_leg(schedule, sofr);

        assert_eq!(leg.build().unwrap().len(), 4);

        let coupons = leg.coupons().unwrap();
        assert_eq!(coupons.len(), 4);
        for coupon in coupons {
            assert_eq!(coupon.nominal(), NOTIONAL);
            assert_eq!(coupon.averaging_method(), RateAveraging::Compound);
        }
    }

    /// `testOvernightLegWithGearingsAndSpreads` (:999): four coupons, each carrying
    /// its own gearing and spread. The coupon exposes no `gearing()`/`spread()`, so
    /// the broadcast is proved through the rate against a plain (gearing 1, spread
    /// 0) leg: a gearing scales the compounded rate and a spread shifts it, both
    /// exactly, over an all-forecast leg.
    #[test]
    fn gearings_and_spreads_broadcast_to_every_coupon() {
        let (settings, curve, sofr, schedule) = common_vars(Date::new(1, Month::June, 2025));
        curve.link_to(flat_rate(settings.evaluation_date().unwrap(), 0.0434));

        let gearings = [1.0, 1.25, 2.0, 0.5];
        let spreads = [0.0001, 0.0001, 0.0002, 0.0002];

        let plain = base_leg(schedule.clone(), sofr.clone()).coupons().unwrap();
        let geared = base_leg(schedule.clone(), sofr.clone())
            .with_gearings(gearings.to_vec())
            .coupons()
            .unwrap();
        let spreaded = base_leg(schedule, sofr)
            .with_spreads(spreads.to_vec())
            .coupons()
            .unwrap();

        assert_eq!(plain.len(), 4);
        assert_eq!(geared.len(), 4);
        assert_eq!(spreaded.len(), 4);

        for i in 0..4 {
            let plain_rate = plain[i].rate().unwrap();
            assert!((geared[i].rate().unwrap() - gearings[i] * plain_rate).abs() < 1e-12);
            assert!((spreaded[i].rate().unwrap() - (plain_rate + spreads[i])).abs() < 1e-12);
        }
    }

    /// `testOvernightLegErrorConditions` (:1094) and the `operator Leg()` guard: a
    /// leg with no notional, one asking for simple averaging (the coupon ports only
    /// compound), and one with a zero gearing (the coupon rejects it rather than
    /// collapsing to a fixed coupon) all surface an error at build.
    #[test]
    fn invalid_legs_surface_errors_at_build() {
        let (_settings, _curve, sofr, schedule) = common_vars(Date::new(1, Month::June, 2025));

        assert!(
            OvernightLeg::new(schedule.clone(), sofr.clone())
                .build()
                .is_err()
        );
        assert!(
            base_leg(schedule.clone(), sofr.clone())
                .with_averaging_method(RateAveraging::Simple)
                .build()
                .is_err()
        );
        assert!(base_leg(schedule, sofr).with_gearing(0.0).build().is_err());
    }
}
