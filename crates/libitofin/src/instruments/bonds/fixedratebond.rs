//! The fixed-coupon bond.
//!
//! Port of `ql/instruments/bonds/fixedratebond.{hpp,cpp}`: the concrete
//! [`FixedRateBond`], a [`Bond`] whose cash flows are a
//! [`FixedRateLeg`](crate::cashflows::FixedRateLeg) plus a single redemption.
//! Its constructor builds `cashflows_` as
//! `FixedRateLeg(schedule).withNotionals(faceAmount).withCouponRates(coupons,
//! accrualDayCounter)...` and then appends the redemption with
//! [`Bond::add_redemptions_to_cashflows`] (`fixedratebond.cpp:54-65`). The
//! maturity is set explicitly to the schedule's end date
//! (`fixedratebond.cpp:52`), matching C++, rather than left to the base's
//! last-coupon fall back.
//!
//! Deviations, all by existing design decisions:
//! - C++ inherits from `Bond`; the port composes it. The base is reachable
//!   through [`bond`](FixedRateBond::bond) / [`bond_mut`](FixedRateBond::bond_mut)
//!   for the settlement, price and accrual accessors, which are unchanged from
//!   the base.
//! - The `Date()` / `DayCounter()` / `Period()` C++ sentinels for an unset issue
//!   date, first-period day counter and ex-coupon period become [`Option`] (D4).

use super::super::bond::Bond;
use crate::cashflows::FixedRateLeg;
use crate::errors::QlResult;
use crate::interestrate::Compounding;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::types::{Natural, Rate, Real};

/// A bond paying a fixed coupon rate on a schedule.
///
/// Wraps the [`Bond`] base with the coupon frequency and day counters the C++
/// `FixedRateBond` exposes.
pub struct FixedRateBond {
    bond: Bond,
    frequency: Frequency,
    day_counter: DayCounter,
    first_period_day_counter: Option<DayCounter>,
}

impl FixedRateBond {
    /// Builds a fixed-rate bond with simple annual-compounding coupon rates.
    ///
    /// Mirrors the C++ constructor: the payment calendar defaults to the
    /// schedule's when `payment_calendar` is `None`, the coupon leg is built
    /// with the given rates against `accrual_day_counter`, and a single
    /// redemption scaled by `redemption` (in base 100) is appended.
    ///
    /// # Errors
    ///
    /// Propagates the [`FixedRateLeg`] and [`Bond`] preconditions, and fails if
    /// no cash flow or more than one redemption is produced.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        settlement_days: Natural,
        face_amount: Real,
        schedule: Schedule,
        coupons: Vec<Rate>,
        accrual_day_counter: DayCounter,
        payment_convention: BusinessDayConvention,
        redemption: Real,
        issue_date: Option<Date>,
        payment_calendar: Option<Calendar>,
        ex_coupon_period: Option<Period>,
        ex_coupon_calendar: Calendar,
        ex_coupon_convention: BusinessDayConvention,
        ex_coupon_end_of_month: bool,
        first_period_day_counter: Option<DayCounter>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<FixedRateBond> {
        let calendar = payment_calendar.unwrap_or_else(|| schedule.calendar().clone());
        let maturity = schedule.end_date();
        let frequency = if schedule.has_tenor() {
            schedule.tenor().frequency()
        } else {
            Frequency::NoFrequency
        };

        let mut leg = FixedRateLeg::new(schedule)
            .with_notional(face_amount)
            .with_coupon_rates(
                coupons,
                accrual_day_counter.clone(),
                Compounding::Simple,
                Frequency::Annual,
            )?
            .with_payment_calendar(calendar.clone())
            .with_payment_adjustment(payment_convention);
        if let Some(day_counter) = &first_period_day_counter {
            leg = leg.with_first_period_day_counter(day_counter.clone());
        }
        if let Some(period) = ex_coupon_period {
            leg = leg.with_ex_coupon_period(
                period,
                ex_coupon_calendar,
                ex_coupon_convention,
                ex_coupon_end_of_month,
            );
        }
        let cashflows = leg.build()?;

        let mut bond = Bond::new(settlement_days, calendar, issue_date, cashflows, settings)?;
        bond.add_redemptions_to_cashflows(&[redemption])?;
        bond.set_maturity_date(maturity);

        require!(!bond.cashflows().is_empty(), "bond with no cashflows!");
        require!(
            bond.redemptions().len() == 1,
            "multiple redemptions created"
        );

        Ok(FixedRateBond {
            bond,
            frequency,
            day_counter: accrual_day_counter,
            first_period_day_counter,
        })
    }

    /// The coupon frequency, taken from the schedule tenor.
    pub fn frequency(&self) -> Frequency {
        self.frequency
    }

    /// The accrual day counter the coupons use.
    pub fn day_counter(&self) -> &DayCounter {
        &self.day_counter
    }

    /// The first period's day counter, when one was given.
    pub fn first_period_day_counter(&self) -> Option<&DayCounter> {
        self.first_period_day_counter.as_ref()
    }

    /// The underlying [`Bond`] base, for its settlement, price and accrual
    /// accessors.
    pub fn bond(&self) -> &Bond {
        &self.bond
    }

    /// Mutable access to the underlying [`Bond`] base, for attaching a pricing
    /// engine and reading the lazily calculated prices.
    pub fn bond_mut(&mut self) -> &mut Bond {
        &mut self.bond
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::shared;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::calendars::unitedstates::{Market, UnitedStates};
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actualactual::{ActualActual, Convention};
    use crate::time::schedule::MakeSchedule;

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(30, Month::November, 2004));
        settings
    }

    fn annual_schedule() -> Schedule {
        MakeSchedule::new()
            .from(Date::new(30, Month::November, 2004))
            .to(Date::new(30, Month::November, 2006))
            .with_frequency(Frequency::Annual)
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build()
    }

    fn plain_bond() -> FixedRateBond {
        FixedRateBond::new(
            3,
            100.0,
            annual_schedule(),
            vec![0.025],
            Actual360::new(),
            BusinessDayConvention::ModifiedFollowing,
            100.0,
            Some(Date::new(30, Month::November, 2004)),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            None,
            settings_today(),
        )
        .unwrap()
    }

    /// The constructor wires the fixed-rate leg onto the base and appends one
    /// full redemption; the accessors report the schedule frequency and the
    /// accrual day counter.
    #[test]
    fn it_builds_two_coupons_and_a_full_redemption() {
        let bond = plain_bond();

        assert_eq!(
            bond.bond().cashflows().len(),
            3,
            "two annual coupons plus the redemption"
        );
        assert_eq!(bond.bond().redemptions().len(), 1);
        assert_eq!(bond.bond().redemptions()[0].amount().unwrap(), 100.0);
        assert_eq!(
            bond.bond().redemptions()[0].date(),
            Date::new(30, Month::November, 2006)
        );
        assert_eq!(bond.bond().notionals(), &[100.0, 0.0]);
        assert_eq!(bond.frequency(), Frequency::Annual);
        assert_eq!(bond.day_counter().name(), Actual360::new().name());
        assert!(bond.first_period_day_counter().is_none());
        assert_eq!(
            bond.bond().issue_date(),
            Some(Date::new(30, Month::November, 2004))
        );
    }

    /// A redemption below 100 scales the redeemed amount down.
    #[test]
    fn a_below_par_redemption_scales_the_amount() {
        let bond = FixedRateBond::new(
            3,
            100.0,
            annual_schedule(),
            vec![0.025],
            Actual360::new(),
            BusinessDayConvention::ModifiedFollowing,
            98.0,
            None,
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            None,
            settings_today(),
        )
        .unwrap();

        assert_eq!(bond.bond().redemptions().len(), 1);
        assert_eq!(bond.bond().redemptions()[0].amount().unwrap(), 98.0);
    }

    /// The maturity is the schedule's end date, not the last coupon's payment
    /// date. On the `bonds.cpp` testCachedFixed schedule the end date
    /// 2008-11-30 (a Sunday, unadjusted termination) differs from the last
    /// coupon payment 2008-11-28 (`ModifiedFollowing`), so the explicit set
    /// must win over the base's last-coupon fall back, matching C++
    /// `Bond::maturityDate()`.
    #[test]
    fn the_maturity_is_the_schedule_end_not_the_last_payment() {
        let schedule = MakeSchedule::new()
            .from(Date::new(30, Month::November, 2004))
            .to(Date::new(30, Month::November, 2008))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(UnitedStates::new(Market::GovernmentBond))
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        let bond = FixedRateBond::new(
            1,
            100.0,
            schedule,
            vec![0.02875],
            ActualActual::with_convention(Convention::ISMA),
            BusinessDayConvention::ModifiedFollowing,
            100.0,
            Some(Date::new(30, Month::November, 2004)),
            None,
            None,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            None,
            settings_today(),
        )
        .unwrap();

        let maturity = bond.bond().maturity_date().unwrap();
        let last_payment = bond.bond().redemptions()[0].date();
        assert_eq!(maturity, Date::new(30, Month::November, 2008));
        assert_eq!(last_payment, Date::new(28, Month::November, 2008));
        assert_ne!(
            maturity, last_payment,
            "the explicit schedule-end maturity wins over the derived last-payment date"
        );
    }
}
