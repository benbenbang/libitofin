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
//! default [`BlackIborCouponPricer`] that `operator Leg()` attaches only when no
//! caps, floors or in-arrears feature is present is attached in
//! [`coupons`](IborLeg::coupons) under the same guard; with a cap or floor set
//! the coupons come from [`capped_floored_coupons`](IborLeg::capped_floored_coupons)
//! and the caller installs a volatility-carrying pricer instead. C++ attaches it
//! through the free `setCouponPricer(leg, pricer)`, which downcasts each flow;
//! the port's [`set_coupon_pricer`] takes the concrete coupons instead, the
//! erased [`Leg`] carrying no downcast.
//!
//! The stub reference date uses `calendar.adjust(end - tenor, bdc)` as the
//! `FloatingLeg` template does, ignoring the schedule's end-of-month flag; the
//! fixed leg passes that flag through. The two agree whenever the flag is unset
//! or `false`.
//!
//! ## Deferred (later sub-tickets of #69)
//!
//! The in-arrears convexity adjustment (`inArrears`), zero and indexed-coupon
//! modes, digital and CMS coupons and the overnight-indexed leg. Their builder
//! methods are omitted entirely rather than accepted and ignored. A zero
//! gearing, which the template collapses to a `FixedRateCoupon`, is likewise not
//! special-cased: the port's [`IborCoupon`] rejects it, so `with_gearing(0.0)`
//! surfaces that error rather than a silent fixed coupon.
//!
//! Caps and floors (`withCaps`/`withFloors`) are ported: they yield
//! [`CappedFlooredCoupon`](crate::cashflows::CappedFlooredCoupon)s over the
//! [`BlackIborCouponPricer`] optionlet path.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::capflooredcoupon::CappedFlooredCoupon;
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
use crate::types::{Integer, Natural, Rate, Real, Spread};

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
    caps: Vec<Rate>,
    floors: Vec<Rate>,
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
            caps: Vec::new(),
            floors: Vec::new(),
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

    /// A cap per coupon; the last one carries over. An empty list, the default,
    /// leaves the coupons uncapped.
    ///
    /// Setting a cap or a floor makes [`build`](Self::build) and
    /// [`capped_floored_coupons`](Self::capped_floored_coupons) produce
    /// [`CappedFlooredCoupon`]s, and stops the plain-path default pricer being
    /// attached: the caller installs a volatility-carrying pricer instead.
    pub fn with_caps(mut self, caps: Vec<Rate>) -> IborLeg {
        self.caps = caps;
        self
    }

    /// A floor per coupon; the last one carries over. An empty list, the
    /// default, leaves the coupons unfloored. See [`with_caps`](Self::with_caps).
    pub fn with_floors(mut self, floors: Vec<Rate>) -> IborLeg {
        self.floors = floors;
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

    /// The bare underlying coupons, no pricer attached: the loop shared by the
    /// plain [`coupons`](Self::coupons) and the capped
    /// [`capped_floored_coupons`](Self::capped_floored_coupons) paths.
    fn raw_coupons(&self) -> QlResult<Vec<Shared<IborCoupon>>> {
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
        require!(
            self.caps.len() <= periods,
            "too many caps ({}), only {periods} required",
            self.caps.len()
        );
        require!(
            self.floors.len() <= periods,
            "too many floors ({}), only {periods} required",
            self.floors.len()
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
            coupons.push(shared(coupon));
        }
        Ok(coupons)
    }

    /// The coupons the leg is made of.
    ///
    /// Each carries the default [`BlackIborCouponPricer`] on the plain path.
    /// With a cap or floor set the plain coupons are returned unpriced, the C++
    /// `operator Leg()` guard withholding the default pricer;
    /// [`capped_floored_coupons`](Self::capped_floored_coupons) is then the
    /// intended entry.
    ///
    /// # Errors
    ///
    /// Errors if no notional was given, if the schedule holds fewer than two
    /// dates, if more notionals, gearings, spreads, caps or floors were given
    /// than the schedule has periods, or if a coupon fails its
    /// [`IborCoupon::new`] preconditions (a zero gearing among them).
    pub fn coupons(&self) -> QlResult<Vec<Shared<IborCoupon>>> {
        let coupons = self.raw_coupons()?;
        if self.caps.is_empty() && self.floors.is_empty() {
            for coupon in &coupons {
                coupon.set_pricer(default_pricer());
            }
        }
        Ok(coupons)
    }

    /// The coupons wrapped in their per-coupon cap and floor, no pricer
    /// attached: the caller installs a volatility-carrying pricer through
    /// [`set_coupon_pricer`].
    ///
    /// # Errors
    ///
    /// As [`coupons`](Self::coupons), plus any [`CappedFlooredCoupon::new`]
    /// precondition (a cap below its floor).
    pub fn capped_floored_coupons(&self) -> QlResult<Vec<Shared<CappedFlooredCoupon>>> {
        let raw = self.raw_coupons()?;
        let mut coupons = Vec::with_capacity(raw.len());
        for (i, underlying) in raw.into_iter().enumerate() {
            let cap = pick(&self.caps, i);
            let floor = pick(&self.floors, i);
            coupons.push(shared(CappedFlooredCoupon::new(underlying, cap, floor)?));
        }
        Ok(coupons)
    }

    /// The coupons as a [`Leg`], with their concrete type erased.
    ///
    /// The plain path erases [`coupons`](Self::coupons); with a cap or floor set
    /// it erases [`capped_floored_coupons`](Self::capped_floored_coupons), whose
    /// coupons carry no pricer, so the caller must install one on the concrete
    /// coupons through [`set_coupon_pricer`] before erasing rather than through
    /// this method.
    ///
    /// # Errors
    ///
    /// As [`coupons`](Self::coupons) or
    /// [`capped_floored_coupons`](Self::capped_floored_coupons).
    pub fn build(&self) -> QlResult<Leg> {
        if self.caps.is_empty() && self.floors.is_empty() {
            Ok(self
                .coupons()?
                .into_iter()
                .map(|coupon| coupon as Shared<dyn CashFlow>)
                .collect())
        } else {
            Ok(self
                .capped_floored_coupons()?
                .into_iter()
                .map(|coupon| coupon as Shared<dyn CashFlow>)
                .collect())
        }
    }
}

/// The `index`-th rate as a cap/floor, or `None` when the list is empty
/// (an uncapped or unfloored coupon).
fn pick(rates: &[Rate], index: usize) -> Option<Rate> {
    if rates.is_empty() {
        None
    } else {
        Some(broadcast(rates, index, 0.0))
    }
}

/// A coupon a pricer can be attached to (an [`IborCoupon`] or a
/// [`CappedFlooredCoupon`]), so [`set_coupon_pricer`] spans both.
pub trait AttachPricer {
    /// Attaches `pricer` to the coupon.
    fn attach_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>);
}

impl AttachPricer for IborCoupon {
    fn attach_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>) {
        self.set_pricer(pricer);
    }
}

impl AttachPricer for CappedFlooredCoupon {
    fn attach_pricer(&self, pricer: SharedMut<dyn FloatingRateCouponPricer>) {
        self.set_pricer(pricer);
    }
}

/// Attaches `pricer` to every coupon, overriding the default the builder set.
///
/// The free `setCouponPricer(Leg&, pricer)` of `couponpricer.cpp`, taking the
/// concrete coupons rather than an erased [`Leg`] since the port cannot downcast
/// a [`CashFlow`](crate::cashflow::CashFlow) back to a coupon. Generic over the
/// coupon type so a plain or a capped/floored leg can be priced the same way.
pub fn set_coupon_pricer<C: AttachPricer>(
    coupons: &[Shared<C>],
    pricer: SharedMut<dyn FloatingRateCouponPricer>,
) {
    for coupon in coupons {
        coupon.attach_pricer(pricer.clone());
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

#[cfg(test)]
mod tests {
    //! The floating-leg cases of `cashflows.cpp`: `testNullFixingDays`, the
    //! `IborLeg` parts of `testExCouponDates` (`l2`, `l6`, `l8`) and of
    //! `testPartialScheduleLegConstruction` (`legf`, `legf2`, `legf3`). The
    //! oracles build `USDLibor` and `Euribor3M`; the port uses [`Euribor`]
    //! throughout, the assertions turning only on the schedule and the fixing-days
    //! default, neither of which the index currency or calendar touches.

    use super::*;
    use crate::cashflows::coupon::Coupon;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::settings::Settings;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::{MakeSchedule, Schedule};

    fn euribor3m() -> Shared<IborIndex> {
        let settings = shared(Settings::<Date>::new());
        shared(Euribor::three_months(
            Handle::<dyn YieldTermStructure>::empty(),
            settings,
        ))
    }

    fn monthly_schedule() -> Schedule {
        let today = Date::new(15, Month::June, 2026);
        MakeSchedule::new()
            .from(today)
            .to(Date::new(15, Month::June, 2031))
            .with_frequency(Frequency::Monthly)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Following)
            .build()
    }

    /// `testNullFixingDays`: a leg left without fixing days builds without error,
    /// its coupons falling back to the index's fixing days. C++ passes the
    /// `Null<Natural>` sentinel through the fixing-days vector; the port has no
    /// in-band null, so an unset builder is its analogue. A leg pinned to the
    /// index's own fixing days must then produce identical fixing dates.
    #[test]
    fn unset_fixing_days_fall_back_to_the_index() {
        let index = euribor3m();
        let schedule = monthly_schedule();
        let unset = IborLeg::new(schedule.clone(), index.clone())
            .with_notional(100.0)
            .coupons()
            .unwrap();
        let pinned = IborLeg::new(schedule, index.clone())
            .with_notional(100.0)
            .with_fixing_days(index.fixing_days())
            .coupons()
            .unwrap();

        assert!(!unset.is_empty());
        assert_eq!(unset.len(), pinned.len());
        for (u, p) in unset.iter().zip(pinned.iter()) {
            assert_eq!(u.fixing_date(), p.fixing_date());
        }
    }

    /// `testExCouponDates`, `l2`: an ibor leg with no ex-coupon period gives every
    /// coupon a null ex-coupon date.
    #[test]
    fn an_ibor_leg_without_an_ex_coupon_period_has_no_ex_coupon_dates() {
        let coupons = IborLeg::new(monthly_schedule(), euribor3m())
            .with_notional(100.0)
            .coupons()
            .unwrap();

        assert!(!coupons.is_empty());
        for coupon in coupons {
            assert_eq!(coupon.ex_coupon_date(), None);
        }
    }

    /// `testExCouponDates`, `l6` and `l8`: the ex-coupon date is measured back
    /// from each payment date, in calendar days off a [`NullCalendar`] and in
    /// business days off [`Target`].
    #[test]
    fn an_ibor_leg_measures_ex_coupon_dates_back_from_the_payment_date() {
        let leg = || IborLeg::new(monthly_schedule(), euribor3m()).with_notional(100.0);

        let calendar_days = leg()
            .with_ex_coupon_period(
                Period::new(2, TimeUnit::Days),
                NullCalendar::new(),
                BusinessDayConvention::Unadjusted,
                false,
            )
            .coupons()
            .unwrap();
        assert!(!calendar_days.is_empty());
        for coupon in calendar_days {
            assert_eq!(coupon.ex_coupon_date(), Some(coupon.accrual_end_date() - 2));
        }

        let business_days = leg()
            .with_ex_coupon_period(
                Period::new(2, TimeUnit::Days),
                Target::new(),
                BusinessDayConvention::Preceding,
                false,
            )
            .coupons()
            .unwrap();
        for coupon in business_days {
            let expected = Target::new().advance(
                coupon.accrual_end_date(),
                -2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            assert_eq!(coupon.ex_coupon_date(), Some(expected));
        }
    }

    /// The `IborLeg` half of `testPartialScheduleLegConstruction`: the first and
    /// last coupons' reference periods span a full tenor when the schedule keeps
    /// its metadata, and fall back to the schedule period when a date-based
    /// schedule has none.
    #[test]
    fn a_date_based_schedule_reconstructs_the_reference_periods_only_with_its_metadata() {
        let schedule = MakeSchedule::new()
            .from(Date::new(15, Month::September, 2017))
            .to(Date::new(30, Month::September, 2020))
            .with_next_to_last_date(Date::new(25, Month::September, 2020))
            .with_frequency(Frequency::Semiannual)
            .backwards()
            .build();
        let with_metadata = Schedule::with_metadata(
            schedule.dates().to_vec(),
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            Some(BusinessDayConvention::Unadjusted),
            Some(Period::new(6, TimeUnit::Months)),
            None,
            Some(schedule.end_of_month()),
            schedule.is_regular().to_vec(),
        );
        let without_metadata = Schedule::from_dates(schedule.dates().to_vec());
        let coupons = |schedule| {
            IborLeg::new(schedule, euribor3m())
                .with_notional(100.0)
                .with_payment_day_counter(ActualActual::with_convention(Convention::ISMA))
                .coupons()
                .unwrap()
        };

        for leg in [coupons(schedule), coupons(with_metadata)] {
            assert_eq!(
                leg[0].reference_period_start(),
                Date::new(25, Month::March, 2017)
            );
            assert_eq!(
                leg[0].reference_period_end(),
                Date::new(25, Month::September, 2017)
            );
            assert_eq!(
                leg.last().unwrap().reference_period_start(),
                Date::new(25, Month::September, 2020)
            );
            assert_eq!(
                leg.last().unwrap().reference_period_end(),
                Date::new(25, Month::March, 2021)
            );
        }

        let leg = coupons(without_metadata);
        assert_eq!(
            leg[0].reference_period_start(),
            Date::new(15, Month::September, 2017)
        );
        assert_eq!(
            leg[0].reference_period_end(),
            Date::new(25, Month::September, 2017)
        );
        assert_eq!(
            leg.last().unwrap().reference_period_start(),
            Date::new(25, Month::September, 2020)
        );
        assert_eq!(
            leg.last().unwrap().reference_period_end(),
            Date::new(30, Month::September, 2020)
        );
    }

    /// With a cap set the builder yields [`CappedFlooredCoupon`]s over one coupon
    /// per period, and the guard withholds the default pricer: the underlyings
    /// carry none until [`set_coupon_pricer`] installs a volatility-carrying one,
    /// so a rate query errors rather than pricing with a wrong pricer.
    #[test]
    fn a_capped_leg_withholds_the_default_pricer() {
        let periods = monthly_schedule().len() - 1;
        let leg = IborLeg::new(monthly_schedule(), euribor3m())
            .with_notional(100.0)
            .with_caps(vec![0.05]);

        let capped = leg.capped_floored_coupons().unwrap();
        assert_eq!(capped.len(), periods);
        assert!(capped[0].is_capped() && !capped[0].is_floored());
        assert!(capped[0].underlying().pricer().is_none());
        assert!(capped[0].rate().is_err());
    }
}
