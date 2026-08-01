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
//!   instead of silently rebating zero.
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
