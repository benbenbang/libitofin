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
    ///
    /// A zero-length `period` is honoured, putting each coupon's ex-coupon date
    /// on its payment date. C++ reads a default-constructed `Period()` as "no
    /// ex-coupon date" instead (`fixedratecoupon.cpp:191`), a sentinel the
    /// absence of this call already expresses here.
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
    /// fewer than two dates, if a stub period needs the schedule's end-of-month
    /// flag and the schedule does not carry one, or if a period day-counter
    /// override does not satisfy the [`InterestRate::new`] precondition.
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
                self.schedule_end_of_month()?,
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
                    self.schedule_end_of_month()?,
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

    /// The schedule's end-of-month flag, as an error when it carries none.
    ///
    /// Only a stub period reads it. C++ raises here too, through the
    /// `QL_REQUIRE` in `Schedule::endOfMonth`; the port surfaces it as an error
    /// rather than a panic, per D4.
    fn schedule_end_of_month(&self) -> QlResult<bool> {
        require!(
            self.schedule.has_end_of_month(),
            "schedule carries no end-of-month flag, which its stub period needs"
        );
        Ok(self.schedule.end_of_month())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::coupon::Coupon;
    use crate::time::calendars::target::Target;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::timeunit::TimeUnit;

    fn simple(rate: Rate, day_counter: DayCounter) -> InterestRate {
        InterestRate::new(rate, day_counter, Compounding::Simple, Frequency::Annual).unwrap()
    }

    /// A zero-length ex-coupon period is honoured rather than read as the
    /// "no ex-coupon" sentinel `Period()` of `fixedratecoupon.cpp:191`, so the
    /// ex-coupon date lands on the payment date and the coupon trades ex-coupon
    /// there. C++ would leave the date null and accrue the full amount.
    #[test]
    fn a_zero_length_ex_coupon_period_puts_the_date_on_the_payment_date() {
        let schedule = crate::time::schedule::MakeSchedule::new()
            .from(Date::new(15, Month::January, 2026))
            .to(Date::new(15, Month::July, 2026))
            .with_frequency(Frequency::Semiannual)
            .with_convention(BusinessDayConvention::Unadjusted)
            .build();
        let coupons = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_interest_rate(simple(0.03, Actual360::new()))
            .with_ex_coupon_period(
                Period::new(0, TimeUnit::Days),
                NullCalendar::new(),
                BusinessDayConvention::Unadjusted,
                false,
            )
            .coupons()
            .unwrap();

        let payment_date = crate::event::Event::date(coupons[0].as_ref());
        assert_eq!(coupons[0].ex_coupon_date(), Some(payment_date));
        assert!(coupons[0].trades_ex_coupon_on(payment_date));
        assert_eq!(coupons[0].accrued_amount(payment_date).unwrap(), 0.0);
    }

    /// A step-up leg: the `i`-th coupon reads the `i`-th notional and the `i`-th
    /// rate, and the last element of either list carries over to the coupons
    /// beyond its end. Every other test gives one notional and one rate, where a
    /// coupon indexing the list wrongly still reads the same value.
    #[test]
    fn each_coupon_reads_its_own_notional_and_rate() {
        let schedule = crate::time::schedule::MakeSchedule::new()
            .from(Date::new(15, Month::January, 2026))
            .to(Date::new(15, Month::January, 2028))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(NullCalendar::new())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        let rates = [0.01, 0.02, 0.03, 0.04];
        let coupons = FixedRateLeg::new(schedule)
            .with_notionals(vec![100.0, 200.0, 300.0])
            .with_interest_rates(
                rates
                    .iter()
                    .map(|&rate| simple(rate, Actual360::new()))
                    .collect(),
            )
            .coupons()
            .unwrap();

        assert_eq!(coupons.len(), 4);
        let borne: Vec<Rate> = coupons.iter().map(|c| c.rate().unwrap()).collect();
        let nominals: Vec<Real> = coupons.iter().map(|c| c.nominal()).collect();
        assert_eq!(borne, rates);
        assert_eq!(nominals, vec![100.0, 200.0, 300.0, 300.0]);
    }

    /// A stub period reads the schedule's end-of-month flag, which a schedule
    /// carrying a tenor and regularity flags may still lack.
    #[test]
    fn a_stub_without_an_end_of_month_flag_is_an_error_not_a_panic() {
        let schedule = Schedule::with_metadata(
            vec![
                Date::new(15, Month::January, 2026),
                Date::new(1, Month::April, 2026),
                Date::new(1, Month::October, 2026),
            ],
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            None,
            Some(Period::new(6, TimeUnit::Months)),
            None,
            None,
            vec![false, true],
        );

        assert!(
            FixedRateLeg::new(schedule)
                .with_notional(100.0)
                .with_interest_rate(simple(0.03, Actual360::new()))
                .coupons()
                .is_err()
        );
    }

    /// `cashflows.cpp::testDefaultSettlementDate`, which reads the leg's accrual
    /// at the evaluation date through `CashFlows::accruedPeriod` and friends.
    /// Those free functions are not ported yet, so the accrual is read off the
    /// single coupon the schedule produces, which is what they would select.
    #[test]
    fn a_leg_spanning_the_evaluation_date_has_a_running_accrual() {
        let today = Date::new(7, Month::July, 2026);
        let schedule = crate::time::schedule::MakeSchedule::new()
            .from(Date::new(7, Month::May, 2026))
            .to(Date::new(7, Month::November, 2026))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        let coupons = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_interest_rate(simple(0.03, Actual360::new()))
            .with_payment_calendar(Target::new())
            .with_payment_adjustment(BusinessDayConvention::Following)
            .coupons()
            .unwrap();

        assert_eq!(coupons.len(), 1);
        assert!(coupons[0].accrued_period(today) > 0.0);
        assert!(coupons[0].accrued_days(today) > 0);
        assert!(coupons[0].accrued_amount(today).unwrap() > 0.0);
    }

    /// The `FixedRateLeg` half of `cashflows.cpp::testExCouponDates` (`l1`, `l5`
    /// and `l7`); the `IborLeg` half is deferred to the floating-coupon epic.
    #[test]
    fn the_ex_coupon_dates_are_measured_back_from_each_payment_date() {
        let today = Date::new(15, Month::June, 2026);
        let schedule = crate::time::schedule::MakeSchedule::new()
            .from(today)
            .to(Date::new(15, Month::June, 2031))
            .with_frequency(Frequency::Monthly)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::Following)
            .build();
        let leg = |schedule| {
            FixedRateLeg::new(schedule)
                .with_notional(100.0)
                .with_interest_rate(simple(0.03, Actual360::new()))
        };

        for coupon in leg(schedule.clone()).coupons().unwrap() {
            assert_eq!(coupon.ex_coupon_date(), None);
        }

        let calendar_days = leg(schedule.clone())
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

        let business_days = leg(schedule)
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

    /// `cashflows.cpp::testIrregularFirstCouponReferenceDatesAtEndOfMonth`.
    #[test]
    fn an_irregular_first_coupon_references_the_end_of_the_month() {
        let schedule = crate::time::schedule::MakeSchedule::new()
            .from(Date::new(17, Month::January, 2017))
            .to(Date::new(28, Month::February, 2018))
            .with_frequency(Frequency::Semiannual)
            .with_convention(BusinessDayConvention::Unadjusted)
            .end_of_month(true)
            .backwards()
            .build();
        let coupons = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_interest_rate(simple(0.01, Actual360::new()))
            .coupons()
            .unwrap();

        assert_eq!(
            coupons[0].reference_period_start(),
            Date::new(31, Month::August, 2016)
        );
    }

    /// `cashflows.cpp::testIrregularFirstCouponReferenceDatesAtEndOfCalendarMonth`,
    /// the one ported case that pins `amount()` against a C++ number.
    #[test]
    fn an_irregular_first_coupon_references_the_end_of_the_calendar_month() {
        let schedule = crate::time::schedule::MakeSchedule::new()
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .from(Date::new(30, Month::September, 2017))
            .to(Date::new(30, Month::September, 2022))
            .with_tenor(Period::new(6, TimeUnit::Months))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_first_date(Date::new(31, Month::March, 2018))
            .with_next_to_last_date(Date::new(31, Month::March, 2022))
            .end_of_month(true)
            .backwards()
            .build();
        let coupons = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_interest_rate(simple(
                0.01875,
                ActualActual::with_convention(Convention::ISMA),
            ))
            .coupons()
            .unwrap();

        assert_eq!(
            coupons[0].reference_period_start(),
            Date::new(30, Month::September, 2017)
        );
        assert!((CashFlow::amount(coupons[0].as_ref()).unwrap() - 0.9375).abs() < 1e-4);
    }

    /// `cashflows.cpp::testIrregularLastCouponReferenceDatesAtEndOfMonth`.
    #[test]
    fn an_irregular_last_coupon_references_the_end_of_the_month() {
        let schedule = crate::time::schedule::MakeSchedule::new()
            .from(Date::new(17, Month::January, 2017))
            .to(Date::new(15, Month::September, 2018))
            .with_next_to_last_date(Date::new(28, Month::February, 2018))
            .with_frequency(Frequency::Semiannual)
            .with_convention(BusinessDayConvention::Unadjusted)
            .end_of_month(true)
            .backwards()
            .build();
        let coupons = FixedRateLeg::new(schedule)
            .with_notional(100.0)
            .with_interest_rate(simple(0.01, Actual360::new()))
            .coupons()
            .unwrap();

        assert_eq!(
            coupons.last().unwrap().reference_period_end(),
            Date::new(31, Month::August, 2018)
        );
    }

    /// The `FixedRateLeg` half of `cashflows.cpp::testPartialScheduleLegConstruction`;
    /// the `IborLeg` half is deferred to the floating-coupon epic. A schedule
    /// stripped of its meta information cannot reconstruct the stubs' reference
    /// periods, so those fall back to the schedule periods themselves.
    #[test]
    fn a_date_based_schedule_reconstructs_the_reference_periods_only_with_its_metadata() {
        let schedule = crate::time::schedule::MakeSchedule::new()
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
            FixedRateLeg::new(schedule)
                .with_notional(100.0)
                .with_interest_rate(simple(
                    0.01,
                    ActualActual::with_convention(Convention::ISMA),
                ))
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
}
