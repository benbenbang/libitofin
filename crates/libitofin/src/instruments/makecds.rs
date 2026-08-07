//! Standard credit-default-swap builder (`MakeCreditDefaultSwap`).
//!
//! Port of `ql/instruments/makecds.{hpp,cpp}`: the comfortable way to
//! instantiate a market-standard CDS. It derives the trade date, the cash
//! settlement date, the protection start and the premium schedule from a quoted
//! tenor, term date or schedule plus a handful of overrides, then hands them to
//! [`CreditDefaultSwap::with_upfront_and_terms`]. C++'s
//! `operator CreditDefaultSwap()` and `operator shared_ptr<CreditDefaultSwap>()`
//! become [`MakeCreditDefaultSwap::build`].
//!
//! ## Divergences from QuantLib
//!
//! - The trade date defaults to the evaluation date held on the [`Settings`] the
//!   builder is given (`makecds.cpp:49`), never a global singleton (D5); with no
//!   evaluation date set [`build`](MakeCreditDefaultSwap::build) is an `Err`
//!   rather than a clock reading (D10).
//! - The three mutually exclusive C++ quotations (`tenor_`, `termDate_` and
//!   `schedule_`, exactly one of which is non-null, `makecds.cpp:65`) become one
//!   enum field, so "exactly one is set" is a type invariant instead of an
//!   unchecked optional access.
//! - `cdsMaturity` returns a null `Date` for a `CDS2015` contract quoted at a
//!   zero tenor off a 20 December or 20 June anchor, which C++ then feeds to
//!   `Schedule`; the port's [`cds_maturity`] reports that case as `Ok(None)` and
//!   [`build`](MakeCreditDefaultSwap::build) turns it into a typed `Err` (D4).
//! - `withPricingEngine` has no counterpart: engines are installed on the
//!   instrument here, so the builder returns an unpriced
//!   [`CreditDefaultSwap`] as the rest of the `Make*` builders that take no
//!   curve do.

use crate::errors::QlResult;
use crate::instruments::claim::Claim;
use crate::instruments::protection::ProtectionSide;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendars::weekendsonly::WeekendsOnly;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual360::Actual360;
use crate::time::period::Period;
use crate::time::schedule::Schedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real};

use super::creditdefaultswap::{CdsTerms, CreditDefaultSwap, cds_maturity};

/// What the contract is quoted against; exactly one of C++'s `tenor_`,
/// `termDate_` and `schedule_` (`makecds.cpp:29-39`).
enum Quotation {
    Tenor(Period),
    TermDate(Date),
    Schedule(Schedule),
}

/// The premium schedule the tenor and term-date quotations generate
/// (`makecds.cpp:79-80`), on the weekends-only calendar and the unadjusted
/// termination date a standard CDS rolls with.
fn premium_schedule(
    protection_start: Date,
    end: Date,
    coupon_tenor: Period,
    convention: BusinessDayConvention,
    rule: DateGeneration,
) -> Schedule {
    Schedule::new(
        protection_start,
        end,
        coupon_tenor,
        WeekendsOnly::new(),
        convention,
        BusinessDayConvention::Unadjusted,
        rule,
        false,
        Date::null(),
        Date::null(),
    )
}

/// Builder for a [`CreditDefaultSwap`] (`ql/instruments/makecds.hpp`).
///
/// Construct with [`new`](Self::new), [`from_term_date`](Self::from_term_date)
/// or [`from_schedule`](Self::from_schedule), chain the `with_*` overrides, then
/// [`build`](Self::build).
pub struct MakeCreditDefaultSwap {
    quotation: Quotation,
    running_spread: Rate,
    settings: Shared<Settings<Date>>,
    side: ProtectionSide,
    nominal: Real,
    upfront_rate: Real,
    coupon_tenor: Period,
    rule: DateGeneration,
    convention: BusinessDayConvention,
    day_counter: DayCounter,
    settles_accrual: bool,
    pays_at_default_time: bool,
    protection_start: Option<Date>,
    upfront_date: Option<Date>,
    claim: Option<Shared<dyn Claim>>,
    last_period_day_counter: DayCounter,
    rebates_accrual: bool,
    trade_date: Option<Date>,
    cash_settlement_days: Natural,
}

impl MakeCreditDefaultSwap {
    /// A contract quoted at `tenor` off the trade date (`makecds.cpp:29-31`),
    /// on the C++ defaults (`makecds.hpp:71-88`). `settings` carries the
    /// evaluation date the trade date defaults to (D5).
    pub fn new(
        tenor: Period,
        running_spread: Rate,
        settings: Shared<Settings<Date>>,
    ) -> MakeCreditDefaultSwap {
        MakeCreditDefaultSwap::with_quotation(Quotation::Tenor(tenor), running_spread, settings)
    }

    /// A contract maturing on `term_date` (`makecds.cpp:33-35`).
    pub fn from_term_date(
        term_date: Date,
        running_spread: Rate,
        settings: Shared<Settings<Date>>,
    ) -> MakeCreditDefaultSwap {
        MakeCreditDefaultSwap::with_quotation(
            Quotation::TermDate(term_date),
            running_spread,
            settings,
        )
    }

    /// A contract on a premium `schedule` given outright (`makecds.cpp:37-39`).
    pub fn from_schedule(
        schedule: Schedule,
        running_spread: Rate,
        settings: Shared<Settings<Date>>,
    ) -> MakeCreditDefaultSwap {
        MakeCreditDefaultSwap::with_quotation(
            Quotation::Schedule(schedule),
            running_spread,
            settings,
        )
    }

    fn with_quotation(
        quotation: Quotation,
        running_spread: Rate,
        settings: Shared<Settings<Date>>,
    ) -> MakeCreditDefaultSwap {
        MakeCreditDefaultSwap {
            quotation,
            running_spread,
            settings,
            side: ProtectionSide::Buyer,
            nominal: 1.0,
            upfront_rate: 0.0,
            coupon_tenor: Period::new(3, TimeUnit::Months),
            rule: DateGeneration::CDS,
            convention: BusinessDayConvention::Following,
            day_counter: Actual360::new(),
            settles_accrual: true,
            pays_at_default_time: true,
            protection_start: None,
            upfront_date: None,
            claim: None,
            last_period_day_counter: Actual360::with_last_day(true),
            rebates_accrual: true,
            trade_date: None,
            cash_settlement_days: 3,
        }
    }

    /// Which side of the protection the contract holds.
    pub fn with_side(mut self, side: ProtectionSide) -> MakeCreditDefaultSwap {
        self.side = side;
        self
    }

    /// The notional the premium and the protection are quoted on.
    pub fn with_nominal(mut self, nominal: Real) -> MakeCreditDefaultSwap {
        self.nominal = nominal;
        self
    }

    /// The upfront quoted alongside the running spread.
    pub fn with_upfront_rate(mut self, upfront_rate: Real) -> MakeCreditDefaultSwap {
        self.upfront_rate = upfront_rate;
        self
    }

    /// The premium leg's payment tenor.
    pub fn with_coupon_tenor(mut self, coupon_tenor: Period) -> MakeCreditDefaultSwap {
        self.coupon_tenor = coupon_tenor;
        self
    }

    /// The rule the premium schedule is generated by.
    pub fn with_date_generation_rule(mut self, rule: DateGeneration) -> MakeCreditDefaultSwap {
        self.rule = rule;
        self
    }

    /// The convention the premium payments are adjusted by.
    pub fn with_convention(mut self, convention: BusinessDayConvention) -> MakeCreditDefaultSwap {
        self.convention = convention;
        self
    }

    /// The day counter the premium accrues with.
    pub fn with_day_counter(mut self, day_counter: DayCounter) -> MakeCreditDefaultSwap {
        self.day_counter = day_counter;
        self
    }

    /// Whether the accrued coupon is due on a default.
    pub fn settle_accrual(mut self, settles_accrual: bool) -> MakeCreditDefaultSwap {
        self.settles_accrual = settles_accrual;
        self
    }

    /// Whether a default pays at default time.
    pub fn pay_at_default_time(mut self, pays_at_default_time: bool) -> MakeCreditDefaultSwap {
        self.pays_at_default_time = pays_at_default_time;
        self
    }

    /// The first date a default triggers the contract, overriding the default
    /// deduced from the trade date or the given schedule.
    pub fn with_protection_start(mut self, protection_start: Date) -> MakeCreditDefaultSwap {
        self.protection_start = Some(protection_start);
        self
    }

    /// The cash settlement date, overriding the one deduced from the trade date
    /// and [`with_cash_settlement_days`](Self::with_cash_settlement_days).
    pub fn with_upfront_date(mut self, upfront_date: Date) -> MakeCreditDefaultSwap {
        self.upfront_date = Some(upfront_date);
        self
    }

    /// What a default pays out.
    pub fn with_claim(mut self, claim: Shared<dyn Claim>) -> MakeCreditDefaultSwap {
        self.claim = Some(claim);
        self
    }

    /// The day counter the last coupon accrues with.
    pub fn with_last_period_day_counter(
        mut self,
        last_period_day_counter: DayCounter,
    ) -> MakeCreditDefaultSwap {
        self.last_period_day_counter = last_period_day_counter;
        self
    }

    /// Whether the protection seller rebates the accrued current coupon.
    pub fn rebate_accrual(mut self, rebates_accrual: bool) -> MakeCreditDefaultSwap {
        self.rebates_accrual = rebates_accrual;
        self
    }

    /// The trade date, overriding the evaluation date.
    pub fn with_trade_date(mut self, trade_date: Date) -> MakeCreditDefaultSwap {
        self.trade_date = Some(trade_date);
        self
    }

    /// The business days from the trade date to cash settlement.
    pub fn with_cash_settlement_days(
        mut self,
        cash_settlement_days: Natural,
    ) -> MakeCreditDefaultSwap {
        self.cash_settlement_days = cash_settlement_days;
        self
    }

    /// Builds the contract (`makecds.cpp:47-92`).
    ///
    /// # Errors
    ///
    /// Errors when the trade date is neither set nor available from the
    /// [`Settings`], when the quoted tenor has already matured under the
    /// `CDS2015` roll-off, and on whatever
    /// [`CreditDefaultSwap::with_upfront_and_terms`] rejects.
    pub fn build(self) -> QlResult<CreditDefaultSwap> {
        let trade_date = match self.trade_date {
            Some(trade_date) => trade_date,
            None => match self.settings.evaluation_date() {
                Some(today) => today,
                None => crate::fail!(
                    "no evaluation date set: MakeCreditDefaultSwap needs one to derive the trade date"
                ),
            },
        };
        let upfront_date = match self.upfront_date {
            Some(upfront_date) => upfront_date,
            None => WeekendsOnly::new().advance(
                trade_date,
                self.cash_settlement_days as Integer,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            ),
        };

        let post_big_bang = matches!(self.rule, DateGeneration::CDS | DateGeneration::CDS2015);
        let protection_start = match (self.protection_start, &self.quotation) {
            (Some(protection_start), _) => protection_start,
            (None, Quotation::Schedule(schedule)) => schedule.date(0),
            (None, _) if post_big_bang => trade_date,
            (None, _) => trade_date + 1,
        };

        let schedule = match self.quotation {
            Quotation::Schedule(schedule) => schedule,
            Quotation::TermDate(term_date) => premium_schedule(
                protection_start,
                term_date,
                self.coupon_tenor,
                self.convention,
                self.rule,
            ),
            Quotation::Tenor(tenor) => {
                let end = if post_big_bang || self.rule == DateGeneration::OldCDS {
                    match cds_maturity(trade_date, tenor, self.rule)? {
                        Some(end) => end,
                        None => crate::fail!(
                            "a {tenor} CDS2015 contract traded on {trade_date} has already matured"
                        ),
                    }
                } else {
                    trade_date + tenor
                };
                premium_schedule(
                    protection_start,
                    end,
                    self.coupon_tenor,
                    self.convention,
                    self.rule,
                )
            }
        };

        CreditDefaultSwap::with_upfront_and_terms(
            self.side,
            self.nominal,
            self.upfront_rate,
            self.running_spread,
            schedule,
            self.convention,
            self.day_counter,
            CdsTerms {
                settles_accrual: self.settles_accrual,
                pays_at_default_time: self.pays_at_default_time,
                protection_start: Some(protection_start),
                upfront_date: Some(upfront_date),
                claim: self.claim,
                last_period_day_counter: Some(self.last_period_day_counter),
                rebates_accrual: self.rebates_accrual,
                trade_date: Some(trade_date),
                cash_settlement_days: self.cash_settlement_days,
            },
            self.settings,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflow::CashFlow;
    use crate::event::Event;
    use crate::shared::shared;
    use crate::time::date::Month;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;

    /// `creditdefaultswap.cpp:964`, a Friday.
    fn today() -> Date {
        Date::new(6, Month::March, 2026)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        settings
    }

    fn years(n: Integer) -> Period {
        Period::new(n, TimeUnit::Years)
    }

    fn five_year() -> MakeCreditDefaultSwap {
        MakeCreditDefaultSwap::new(years(5), 0.01, settings_today())
    }

    /// What C++'s `protectionEndDate` reads (`creditdefaultswap.cpp:430-432`),
    /// which is deferred on [`CreditDefaultSwap`] itself.
    fn protection_end_date(cds: &CreditDefaultSwap) -> Date {
        cds.coupons()[cds.coupons().len() - 1]
            .as_coupon()
            .unwrap()
            .accrual_end_date()
    }

    fn day_counter_names(cds: &CreditDefaultSwap) -> (String, String) {
        let coupons = cds.coupons();
        (
            coupons[0].as_coupon().unwrap().day_counter().name(),
            coupons[coupons.len() - 1]
                .as_coupon()
                .unwrap()
                .day_counter()
                .name(),
        )
    }

    /// `testDefaultConventions` (`creditdefaultswap.cpp:969-994`): every default
    /// of `makecds.hpp:71-88` read back off the built contract.
    #[test]
    fn the_tenor_quotation_builds_on_the_standard_conventions() {
        let cds = five_year().build().unwrap();

        assert_eq!(cds.running_spread(), 0.01);
        assert_eq!(cds.notional(), 1.0);
        assert_eq!(cds.upfront(), Some(0.0));

        assert_eq!(cds.trade_date(), today());
        assert_eq!(cds.cash_settlement_days(), 3);
        assert_eq!(cds.upfront_payment().date(), today() + 5);
        assert_eq!(cds.protection_start_date(), today());
        assert_eq!(
            protection_end_date(&cds),
            cds_maturity(today(), years(5), DateGeneration::CDS)
                .unwrap()
                .unwrap()
        );

        assert_eq!(cds.coupons().len(), 21);

        assert!(cds.settles_accrual());
        assert!(cds.pays_at_default_time());
        assert!(cds.rebates_accrual());

        let (first, last) = day_counter_names(&cds);
        assert_eq!(first, "Actual/360");
        assert_eq!(last, "Actual/360 (inc)");
    }

    /// `creditdefaultswap.cpp:996-998`. The term date is quoted off `CDS2015`
    /// but the builder still generates on its default `CDS` rule.
    #[test]
    fn the_term_date_quotation_matures_on_the_given_date() {
        let term_date = cds_maturity(today(), years(3), DateGeneration::CDS2015)
            .unwrap()
            .unwrap();
        let cds = MakeCreditDefaultSwap::from_term_date(term_date, 0.01, settings_today())
            .build()
            .unwrap();

        assert_eq!(protection_end_date(&cds), term_date);
    }

    /// `creditdefaultswap.cpp:1000-1005`: a schedule given outright frames the
    /// protection, so the protection start comes off its front rather than off
    /// the trade date.
    #[test]
    fn the_schedule_quotation_frames_the_protection() {
        let term_date = cds_maturity(today() - 4, years(10), DateGeneration::CDS2015)
            .unwrap()
            .unwrap();
        let schedule = premium_schedule(
            today() - 4,
            term_date,
            Period::new(3, TimeUnit::Months),
            BusinessDayConvention::Following,
            DateGeneration::CDS2015,
        );
        let front = schedule.date(0);
        let back = schedule.date(schedule.len() - 1);

        let cds = MakeCreditDefaultSwap::from_schedule(schedule, 0.01, settings_today())
            .build()
            .unwrap();

        assert_eq!(cds.protection_start_date(), front);
        assert_eq!(protection_end_date(&cds), back);
    }

    /// `creditdefaultswap.cpp:1009-1018`: the notional reaches both the contract
    /// and its coupons, and the upfront payment is `upfront * notional`.
    #[test]
    fn the_nominal_and_the_upfront_rate_scale_the_contract() {
        let cds = five_year()
            .with_nominal(10_000.0)
            .with_upfront_rate(0.02)
            .build()
            .unwrap();

        assert_eq!(cds.notional(), 10_000.0);
        assert_eq!(cds.coupons()[0].as_coupon().unwrap().nominal(), 10_000.0);
        assert_eq!(cds.upfront(), Some(0.02));
        assert_eq!(cds.upfront_payment().amount().unwrap(), 200.0);
    }

    /// `creditdefaultswap.cpp:1020-1030`: the cash settlement days roll the
    /// upfront date over the weekend, and an explicit upfront date wins.
    #[test]
    fn the_cash_settlement_days_place_the_upfront_date() {
        let cds = five_year().with_cash_settlement_days(2).build().unwrap();
        assert_eq!(cds.cash_settlement_days(), 2);
        assert_eq!(cds.upfront_payment().date(), today() + 4);

        let cds = five_year()
            .with_cash_settlement_days(2)
            .with_upfront_date(today() + 7)
            .build()
            .unwrap();
        assert_eq!(cds.cash_settlement_days(), 2);
        assert_eq!(cds.upfront_payment().date(), today() + 7);
    }

    /// `creditdefaultswap.cpp:1032-1034`.
    #[test]
    fn an_explicit_protection_start_overrides_the_trade_date() {
        let cds = five_year()
            .with_protection_start(today() + 2)
            .build()
            .unwrap();

        assert_eq!(cds.protection_start_date(), today() + 2);
    }

    /// `creditdefaultswap.cpp:1036-1038`.
    #[test]
    fn the_coupon_tenor_sets_the_premium_frequency() {
        let cds = five_year()
            .with_coupon_tenor(Period::new(6, TimeUnit::Months))
            .build()
            .unwrap();

        assert_eq!(cds.coupons().len(), 11);
    }

    /// `creditdefaultswap.cpp:1040-1046`: the trade date carries the cash
    /// settlement and the protection start with it, leaving the settlement days
    /// at their default.
    #[test]
    fn an_explicit_trade_date_moves_the_settlement_and_the_protection_start() {
        let cds = five_year().with_trade_date(today() + 3).build().unwrap();

        assert_eq!(cds.trade_date(), today() + 3);
        assert_eq!(cds.cash_settlement_days(), 3);
        assert_eq!(cds.upfront_payment().date(), today() + 6);
        assert_eq!(cds.protection_start_date(), today() + 3);
    }

    /// `creditdefaultswap.cpp:1048-1058`.
    #[test]
    fn the_three_default_conventions_can_each_be_turned_off() {
        assert!(
            !five_year()
                .settle_accrual(false)
                .build()
                .unwrap()
                .settles_accrual()
        );
        assert!(
            !five_year()
                .pay_at_default_time(false)
                .build()
                .unwrap()
                .pays_at_default_time()
        );
        assert!(
            !five_year()
                .rebate_accrual(false)
                .build()
                .unwrap()
                .rebates_accrual()
        );
    }

    /// `creditdefaultswap.cpp:1060-1077`: the two day counters are independent,
    /// the last period's overriding the premium's only on the last coupon.
    #[test]
    fn the_premium_and_the_last_period_day_counters_are_independent() {
        let cds = five_year()
            .with_day_counter(Actual365Fixed::new())
            .build()
            .unwrap();
        assert_eq!(
            day_counter_names(&cds),
            (
                "Actual/365 (Fixed)".to_string(),
                "Actual/360 (inc)".to_string()
            )
        );

        let cds = five_year()
            .with_last_period_day_counter(Actual365Fixed::new())
            .build()
            .unwrap();
        assert_eq!(
            day_counter_names(&cds),
            ("Actual/360".to_string(), "Actual/365 (Fixed)".to_string())
        );
    }

    /// D5/D10: with no evaluation date there is no trade date to fall back on,
    /// and the builder says so rather than reading a clock.
    #[test]
    fn an_unset_evaluation_date_is_an_error() {
        let settings = shared(Settings::new());
        let built = MakeCreditDefaultSwap::new(years(5), 0.01, settings).build();

        assert!(built.is_err());
    }
}
