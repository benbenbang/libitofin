//! Credit-default swap.
//!
//! Port of the running-spread half of
//! `ql/instruments/creditdefaultswap.{hpp,cpp}`: the contract's terms, the
//! premium leg it pays, and the cash-settled upfront and accrual-rebate flows
//! that frame it (`creditdefaultswap.cpp:39-60` and `:87-176`).
//!
//! A running-spread contract quotes no upfront, so [`upfront`] is always `None`
//! and the upfront payment is a zero-amount flow on the cash settlement date;
//! it exists because the engines read it unconditionally
//! (`creditdefaultswap.cpp:127-131`).
//!
//! ## Divergences from QuantLib
//!
//! - The C++ constructor's eight defaulted arguments (`creditdefaultswap.hpp:105-112`)
//!   become [`CdsTerms`], whose [`Default`] carries the C++ defaults.
//!   [`CreditDefaultSwap::new`] takes the eight leading arguments and defaults
//!   the rest; [`CreditDefaultSwap::with_terms`] takes them all.
//! - The empty-schedule check runs before the protection-start default, where
//!   C++ runs it after: `protectionStart == Date() ? schedule[0]` sits in the
//!   member-initialiser list (`creditdefaultswap.cpp:56`) and so reads
//!   `schedule[0]` before `init`'s `QL_REQUIRE(!schedule.empty())`
//!   (`creditdefaultswap.cpp:91`). Reordering keeps an empty schedule an `Err`
//!   rather than an out-of-range index (D4).
//! - The null-date sentinels for the protection start, the trade date and the
//!   last-period day counter become [`Option`]s, as does the empty
//!   `DayCounter()` the C++ leg builder reads as "use the coupon rate's"
//!   (`fixedratecoupon.cpp:255-257`).
//! - `registerWith(claim_)` (`creditdefaultswap.cpp:175`) has no counterpart:
//!   the ported [`Claim`] is not an `Observable` (see `claim.rs`), so there is
//!   nothing to subscribe to.
//!
//! ## Deferred
//!
//! Within EPIC Credit (#676), and each omitted visibly rather than accepted and
//! ignored:
//!
//! - The [`Instrument`](crate::instrument::Instrument) interface: `isExpired`,
//!   `setupArguments`, `fetchResults` and the priced accessors
//!   (`creditdefaultswap.cpp:207-313`). This module builds the contract only.
//! - The upfront-quoted constructor (`creditdefaultswap.cpp:62-85`), which is
//!   the only path setting `upfront_` and taking an explicit upfront date.
//! - The accrual-rebate arithmetic (`creditdefaultswap.cpp:143-168`). A trade
//!   date on or after the first accrual date rebates the coupon accrued to
//!   `tradeDate + 1`; [`CreditDefaultSwap::with_terms`] rejects that case
//!   instead of silently rebating zero. That covers every `CDS`- or
//!   `CDS2015`-rule contract on default terms, whose deduced trade date is the
//!   first accrual date, so those build only with `rebates_accrual` off.
//! - `impliedHazardRate`, `conventionalSpread` and their objective function
//!   (`creditdefaultswap.cpp:315-428`), and `cdsMaturity`
//!   (`creditdefaultswap.cpp:479-506`).
//! - `protectionEndDate` (`creditdefaultswap.cpp:430-432`), which reads the
//!   accrual end of the last coupon through the `coupon_cast` that
//!   [`CashFlow::as_coupon`](crate::cashflow::CashFlow::as_coupon) ports.
//!
//! [`upfront`]: CreditDefaultSwap::upfront

use crate::cashflow::Leg;
use crate::cashflows::{FixedRateLeg, SimpleCashFlow};
use crate::errors::QlResult;
use crate::instruments::claim::{Claim, FaceValueClaim};
use crate::instruments::protection::ProtectionSide;
use crate::interestrate::Compounding;
use crate::require;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real};

/// The terms a [`CreditDefaultSwap`] defaults when they are not quoted.
///
/// One field per defaulted argument of the C++ constructors
/// (`creditdefaultswap.hpp:105-112`); [`Default`] carries their C++ values.
pub struct CdsTerms {
    /// Whether the accrued coupon is due on a default.
    pub settles_accrual: bool,
    /// Whether a default pays at default time rather than at the end of the
    /// accrual period.
    pub pays_at_default_time: bool,
    /// The first date a default triggers the contract; the schedule's first
    /// date when absent.
    pub protection_start: Option<Date>,
    /// What a default pays out; a [`FaceValueClaim`] when absent.
    pub claim: Option<Shared<dyn Claim>>,
    /// The day counter the last coupon accrues with, overriding the spread's.
    pub last_period_day_counter: Option<DayCounter>,
    /// Whether the protection seller rebates the accrued current coupon.
    pub rebates_accrual: bool,
    /// The trade date; deduced from the protection start when absent.
    pub trade_date: Option<Date>,
    /// The business days from the trade date to cash settlement.
    pub cash_settlement_days: Natural,
}

impl Default for CdsTerms {
    fn default() -> CdsTerms {
        CdsTerms {
            settles_accrual: true,
            pays_at_default_time: true,
            protection_start: None,
            claim: None,
            last_period_day_counter: None,
            rebates_accrual: true,
            trade_date: None,
            cash_settlement_days: 3,
        }
    }
}

/// A credit-default swap quoted as a running spread.
///
/// One side pays the premium leg and receives the protection payment, the other
/// the reverse; which way round is [`side`](CreditDefaultSwap::side).
pub struct CreditDefaultSwap {
    side: ProtectionSide,
    notional: Real,
    upfront: Option<Rate>,
    running_spread: Rate,
    settles_accrual: bool,
    pays_at_default_time: bool,
    claim: Shared<dyn Claim>,
    protection_start: Date,
    trade_date: Date,
    cash_settlement_days: Natural,
    leg: Leg,
    upfront_payment: Shared<SimpleCashFlow>,
    accrual_rebate: Option<Shared<SimpleCashFlow>>,
    maturity: Date,
}

impl CreditDefaultSwap {
    /// A contract on the C++ default terms, settling its accrual and paying at
    /// default time as `settles_accrual` and `pays_at_default_time` say
    /// (`creditdefaultswap.cpp:39-60`).
    ///
    /// The remaining terms take their C++ defaults; [`with_terms`] gives them.
    ///
    /// # Errors
    ///
    /// As [`with_terms`].
    ///
    /// [`with_terms`]: CreditDefaultSwap::with_terms
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        side: ProtectionSide,
        notional: Real,
        spread: Rate,
        schedule: Schedule,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        settles_accrual: bool,
        pays_at_default_time: bool,
    ) -> QlResult<CreditDefaultSwap> {
        CreditDefaultSwap::with_terms(
            side,
            notional,
            spread,
            schedule,
            payment_convention,
            day_counter,
            CdsTerms {
                settles_accrual,
                pays_at_default_time,
                ..CdsTerms::default()
            },
        )
    }

    /// A contract on the given `terms` (`creditdefaultswap.cpp:39-60` and its
    /// `init`, `:87-176`).
    ///
    /// # Errors
    ///
    /// Errors on an empty schedule, on a protection start after the first
    /// accrual date under a pre-Big-Bang date-generation rule, on a cash
    /// settlement date before the protection start, and on the deferred
    /// accrual-rebate case where the trade date falls on or after the first
    /// accrual date. Propagates the premium leg's own preconditions.
    pub fn with_terms(
        side: ProtectionSide,
        notional: Real,
        spread: Rate,
        schedule: Schedule,
        payment_convention: BusinessDayConvention,
        day_counter: DayCounter,
        terms: CdsTerms,
    ) -> QlResult<CreditDefaultSwap> {
        require!(
            !schedule.is_empty(),
            "CreditDefaultSwap needs a non-empty schedule."
        );
        let protection_start = terms.protection_start.unwrap_or_else(|| schedule.date(0));

        let post_big_bang = schedule.has_rule()
            && matches!(
                schedule.rule(),
                DateGeneration::CDS | DateGeneration::CDS2015
            );
        if !post_big_bang {
            require!(
                protection_start <= schedule.date(0),
                "protection can not start after accrual"
            );
        }

        let mut builder = FixedRateLeg::new(schedule.clone())
            .with_notional(notional)
            .with_coupon_rate(spread, day_counter, Compounding::Simple, Frequency::Annual)?
            .with_payment_adjustment(payment_convention);
        if let Some(day_counter) = terms.last_period_day_counter {
            builder = builder.with_last_period_day_counter(day_counter);
        }
        let leg = builder.build()?;

        let trade_date = terms.trade_date.unwrap_or_else(|| {
            if post_big_bang {
                protection_start
            } else {
                protection_start - 1
            }
        });

        let effective_upfront_date = schedule.calendar().advance(
            trade_date,
            terms.cash_settlement_days as Integer,
            TimeUnit::Days,
            payment_convention,
            false,
        );
        require!(
            effective_upfront_date >= protection_start,
            "The cash settlement date must not be before the protection start date."
        );

        let upfront_payment = shared(SimpleCashFlow::new(0.0, effective_upfront_date)?);

        let accrual_rebate = if terms.rebates_accrual {
            require!(
                trade_date < schedule.date(0),
                "a trade date on or after the first accrual date rebates the accrued coupon, \
                 which is not ported yet (creditdefaultswap.cpp:143-168)"
            );
            Some(shared(SimpleCashFlow::new(0.0, effective_upfront_date)?))
        } else {
            None
        };

        Ok(CreditDefaultSwap {
            side,
            notional,
            upfront: None,
            running_spread: spread,
            settles_accrual: terms.settles_accrual,
            pays_at_default_time: terms.pays_at_default_time,
            claim: terms.claim.unwrap_or_else(|| shared(FaceValueClaim)),
            protection_start,
            trade_date,
            cash_settlement_days: terms.cash_settlement_days,
            leg,
            upfront_payment,
            accrual_rebate,
            maturity: schedule.date(schedule.len() - 1),
        })
    }

    /// Which side of the protection this contract holds.
    pub fn side(&self) -> ProtectionSide {
        self.side
    }

    /// The notional the protection covers.
    pub fn notional(&self) -> Real {
        self.notional
    }

    /// The running spread the premium leg pays.
    pub fn running_spread(&self) -> Rate {
        self.running_spread
    }

    /// The upfront, in fractional units.
    ///
    /// Always `None` here: only the deferred upfront-quoted constructor sets it
    /// (`creditdefaultswap.cpp:78`).
    pub fn upfront(&self) -> Option<Rate> {
        self.upfront
    }

    /// Whether the accrued coupon is due on a default.
    pub fn settles_accrual(&self) -> bool {
        self.settles_accrual
    }

    /// Whether a default pays at default time.
    pub fn pays_at_default_time(&self) -> bool {
        self.pays_at_default_time
    }

    /// What a default pays out.
    pub fn claim(&self) -> &Shared<dyn Claim> {
        &self.claim
    }

    /// The premium leg.
    pub fn coupons(&self) -> &Leg {
        &self.leg
    }

    /// The first date a default triggers the contract.
    pub fn protection_start_date(&self) -> Date {
        self.protection_start
    }

    /// The schedule's last date.
    pub fn maturity(&self) -> Date {
        self.maturity
    }

    /// The zero-amount upfront payment, due on the cash settlement date.
    pub fn upfront_payment(&self) -> &Shared<SimpleCashFlow> {
        &self.upfront_payment
    }

    /// The accrual rebate, when the contract carries one.
    pub fn accrual_rebate(&self) -> Option<&Shared<SimpleCashFlow>> {
        self.accrual_rebate.as_ref()
    }

    /// Whether the protection seller rebates the accrued current coupon
    /// (`creditdefaultswap.hpp:186`).
    pub fn rebates_accrual(&self) -> bool {
        self.accrual_rebate.is_some()
    }

    /// The contract's trade date.
    pub fn trade_date(&self) -> Date {
        self.trade_date
    }

    /// The business days from the trade date to cash settlement.
    pub fn cash_settlement_days(&self) -> Natural {
        self.cash_settlement_days
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflow::CashFlow;
    use crate::event::Event;
    use crate::time::calendars::weekendsonly::WeekendsOnly;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::schedule::MakeSchedule;

    const NOTIONAL: Real = 10_000_000.0;
    const SPREAD: Rate = 0.01;

    /// A ten-year semiannual contract on the conventions a standard CDS quotes
    /// with. The `Backward` rule keeps it pre-Big-Bang, which is the arm the
    /// trade-date deduction and the protection-start check both branch on.
    fn ten_year_schedule() -> Schedule {
        MakeSchedule::new()
            .from(Date::new(20, Month::June, 2026))
            .to(Date::new(20, Month::June, 2036))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(WeekendsOnly::new())
            .with_convention(BusinessDayConvention::Following)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build()
    }

    fn contract(terms: CdsTerms) -> QlResult<CreditDefaultSwap> {
        CreditDefaultSwap::with_terms(
            ProtectionSide::Buyer,
            NOTIONAL,
            SPREAD,
            ten_year_schedule(),
            BusinessDayConvention::Following,
            Actual360::new(),
            terms,
        )
    }

    #[test]
    fn the_premium_leg_spans_the_schedule_and_the_contract_matures_with_it() {
        let schedule = ten_year_schedule();
        let cds = CreditDefaultSwap::new(
            ProtectionSide::Buyer,
            NOTIONAL,
            SPREAD,
            schedule.clone(),
            BusinessDayConvention::Following,
            Actual360::new(),
            true,
            true,
        )
        .unwrap();

        assert_eq!(cds.coupons().len(), schedule.len() - 1);
        assert_eq!(cds.coupons().len(), 20);
        assert_eq!(
            Event::date(cds.coupons()[0].as_ref()),
            WeekendsOnly::new().adjust(schedule.date(1), BusinessDayConvention::Following)
        );
        assert_eq!(cds.maturity(), *schedule.dates().last().unwrap());
        assert_eq!(cds.side(), ProtectionSide::Buyer);
        assert_eq!(cds.notional(), NOTIONAL);
        assert_eq!(cds.running_spread(), SPREAD);
        assert!(cds.settles_accrual());
        assert!(cds.pays_at_default_time());
        assert_eq!(cds.cash_settlement_days(), 3);
    }

    /// `creditdefaultswap.cpp:110-116`, the pre-Big-Bang arm.
    #[test]
    fn the_trade_date_defaults_to_the_day_before_the_protection_start() {
        let cds = contract(CdsTerms::default()).unwrap();

        assert_eq!(cds.trade_date(), cds.protection_start_date() - 1);
    }

    /// `creditdefaultswap.cpp:56`.
    #[test]
    fn the_protection_starts_on_the_first_accrual_date_unless_given() {
        let schedule = ten_year_schedule();
        assert_eq!(
            contract(CdsTerms::default())
                .unwrap()
                .protection_start_date(),
            schedule.date(0)
        );

        let earlier = schedule.date(0) - 10;
        let cds = contract(CdsTerms {
            protection_start: Some(earlier),
            ..CdsTerms::default()
        })
        .unwrap();
        assert_eq!(cds.protection_start_date(), earlier);
        assert_eq!(cds.trade_date(), earlier - 1);
    }

    /// A running-spread contract quotes no upfront, so the payment is a zero
    /// flow that exists only for the engines to read
    /// (`creditdefaultswap.cpp:127-131`).
    #[test]
    fn the_upfront_payment_is_a_zero_flow_on_the_cash_settlement_date() {
        let cds = contract(CdsTerms::default()).unwrap();
        let expected = WeekendsOnly::new().advance(
            cds.trade_date(),
            3,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );

        assert_eq!(cds.upfront(), None);
        assert_eq!(cds.upfront_payment().amount().unwrap(), 0.0);
        assert_eq!(Event::date(cds.upfront_payment().as_ref()), expected);
        assert!(expected >= cds.protection_start_date());
    }

    /// `creditdefaultswap.cpp:138-171`: the rebate is a flow the contract either
    /// carries or does not, which is what `rebatesAccrual()` reads
    /// (`creditdefaultswap.hpp:186`).
    #[test]
    fn the_accrual_rebate_is_a_zero_flow_when_rebated_and_absent_otherwise() {
        let rebated = contract(CdsTerms::default()).unwrap();
        let rebate = rebated.accrual_rebate().unwrap();
        assert!(rebated.rebates_accrual());
        assert_eq!(rebate.amount().unwrap(), 0.0);
        assert_eq!(
            Event::date(rebate.as_ref()),
            Event::date(rebated.upfront_payment().as_ref())
        );

        let bare = contract(CdsTerms {
            rebates_accrual: false,
            ..CdsTerms::default()
        })
        .unwrap();
        assert!(bare.accrual_rebate().is_none());
        assert!(!bare.rebates_accrual());
    }

    /// `creditdefaultswap.cpp:91`. C++ reads `schedule[0]` for the protection
    /// start before this check runs; the port checks first, so an empty schedule
    /// is an error rather than an out-of-range index.
    #[test]
    fn an_empty_schedule_is_an_error_not_a_panic() {
        let empty = CreditDefaultSwap::with_terms(
            ProtectionSide::Buyer,
            NOTIONAL,
            SPREAD,
            Schedule::from_dates(Vec::new()),
            BusinessDayConvention::Following,
            Actual360::new(),
            CdsTerms::default(),
        );

        assert!(empty.is_err());
    }

    /// `creditdefaultswap.cpp:99-101`, which only guards the pre-Big-Bang arm.
    #[test]
    fn protection_can_not_start_after_the_first_accrual_date() {
        let late = ten_year_schedule().date(0) + 1;

        assert!(
            contract(CdsTerms {
                protection_start: Some(late),
                ..CdsTerms::default()
            })
            .is_err()
        );
    }

    /// The deferred rebate arithmetic (`creditdefaultswap.cpp:143-168`): a trade
    /// date on or after the first accrual date rebates a non-zero accrued
    /// coupon, so the contract refuses to build rather than rebate zero. Without
    /// the rebate there is nothing to compute and the same terms build.
    #[test]
    fn a_trade_date_on_or_after_the_first_accrual_date_is_refused_while_rebating() {
        let first_accrual = ten_year_schedule().date(0);

        assert!(
            contract(CdsTerms {
                trade_date: Some(first_accrual),
                ..CdsTerms::default()
            })
            .is_err()
        );

        let bare = contract(CdsTerms {
            trade_date: Some(first_accrual),
            rebates_accrual: false,
            ..CdsTerms::default()
        })
        .unwrap();
        assert_eq!(bare.trade_date(), first_accrual);
    }

    /// The post-Big-Bang arm of the trade-date deduction
    /// (`creditdefaultswap.cpp:111-112`): protection is effective on the trade
    /// date itself, and the protection-start check is skipped
    /// (`creditdefaultswap.cpp:94-101`).
    ///
    /// That arm lands on the deferred rebate case by construction, since the
    /// deduced trade date is the first accrual date, so a contract on a
    /// `CDS`-rule schedule only builds without the rebate.
    #[test]
    fn a_post_big_bang_contract_trades_on_the_protection_start() {
        let schedule = MakeSchedule::new()
            .from(Date::new(20, Month::June, 2026))
            .to(Date::new(20, Month::June, 2036))
            .with_frequency(Frequency::Quarterly)
            .with_calendar(WeekendsOnly::new())
            .with_convention(BusinessDayConvention::Following)
            .with_rule(DateGeneration::CDS)
            .build();
        let build = |terms| {
            CreditDefaultSwap::with_terms(
                ProtectionSide::Buyer,
                NOTIONAL,
                SPREAD,
                schedule.clone(),
                BusinessDayConvention::Following,
                Actual360::new(),
                terms,
            )
        };

        let cds = build(CdsTerms {
            rebates_accrual: false,
            ..CdsTerms::default()
        })
        .unwrap();
        assert_eq!(cds.trade_date(), cds.protection_start_date());
        assert_eq!(cds.protection_start_date(), schedule.date(0));

        assert!(build(CdsTerms::default()).is_err());
    }

    /// `creditdefaultswap.cpp:173-174`.
    #[test]
    fn the_claim_defaults_to_the_face_value() {
        let cds = contract(CdsTerms::default()).unwrap();

        assert_eq!(
            cds.claim().amount(&cds.maturity(), NOTIONAL, 0.4),
            NOTIONAL * 0.6
        );
    }

    /// The last-period day counter overrides the spread's on the last coupon
    /// alone, where an absent one leaves the spread's in place
    /// (`fixedratecoupon.cpp:255-257`).
    #[test]
    fn the_last_period_day_counter_reaches_the_last_coupon_only() {
        let plain = contract(CdsTerms::default()).unwrap();
        let overridden = contract(CdsTerms {
            last_period_day_counter: Some(Actual365Fixed::new()),
            ..CdsTerms::default()
        })
        .unwrap();

        let amount = |cds: &CreditDefaultSwap, i: usize| cds.coupons()[i].amount().unwrap();
        let last = plain.coupons().len() - 1;
        assert_ne!(amount(&plain, last), amount(&overridden, last));
        assert_eq!(amount(&plain, 0), amount(&overridden, 0));
    }
}
