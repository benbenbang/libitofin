//! Base bond instrument.
//!
//! Port of `ql/instruments/bond.{hpp,cpp}`: the abstract [`Bond`] an
//! interest-rate bond derives from. It is an [`Instrument`] holding a [`Leg`]
//! of cash flows, a notional schedule and a number of settlement days, and it
//! prices through a [`Bond::engine`](BondEngine) that returns a settlement
//! value.
//!
//! `Bond` has no standalone oracle: it is abstract, and the numbers are pinned
//! by the derived `FixedRateBond` + `DiscountingBondEngine` against
//! `bonds.cpp`. This module ports the interface header-faithfully.
//!
//! Deviations, all by existing design decisions:
//! - The `Bond::arguments`, `Bond::results` and `Bond::engine` inner classes
//!   become the free [`BondArguments`], [`BondResults`] and [`BondEngine`].
//! - The C++ `Date()`/`Null<Real>` sentinels for an unset issue date, maturity
//!   and settlement value become [`Option`] (D4/D5).
//! - `settlementDate`, `notional` and the price accessors read the evaluation
//!   date from the [`Settings`] handle the bond is built with (D5); with none
//!   set they return an error rather than the C++ system-clock fall back (D10).
//! - The methods delegating to `BondFunctions`/`CashFlows` -
//!   [`is_expired`](Instrument::is_expired) and
//!   [`accrued_amount`](Bond::accrued_amount) - call [`CashFlows`] directly, the
//!   port's home for those analytics.
//! - The price-fed [`Bond::yield_rate`] and [`BondPrice`] land here (#290),
//!   solver-backed through `BondFunctions`. The engine-priced
//!   `Bond::yield(dayCounter, ...)` overload, the price-from-yield
//!   `cleanPrice`/`dirtyPrice`, the coupon-rate and cash-flow-date inspectors,
//!   `setSingleRedemption` and the `faceAmount` constructor remain follow-up
//!   work.

use std::any::Any;

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{AmortizingPayment, CashFlows, Redemption};
use crate::errors::QlResult;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::interestrate::Compounding;
use crate::math::comparison::close;
use crate::pricingengine::{Arguments, GenericEngine, Results};
use crate::pricingengines::bond::BondFunctions;
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::frequency::Frequency;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real};
use crate::{fail, require};

/// Arguments passed to a bond pricing engine (the C++ `Bond::arguments`).
pub struct BondArguments {
    /// The settlement date the value refers to.
    pub settlement_date: Option<Date>,
    /// All the bond's cash flows, redemptions included.
    pub cashflows: Leg,
    /// The bond's calendar.
    pub calendar: Calendar,
}

impl Arguments for BondArguments {
    fn validate(&self) -> QlResult<()> {
        require!(
            self.settlement_date.is_some(),
            "no settlement date provided"
        );
        require!(!self.cashflows.is_empty(), "no cash flow provided");
        Ok(())
    }
}

/// Results returned by a bond pricing engine (the C++ `Bond::results`).
#[derive(Default)]
pub struct BondResults {
    /// The instrument-level results (NPV and the rest).
    pub instrument: InstrumentResults,
    /// The dirty value at the settlement date.
    pub settlement_value: Option<Real>,
}

impl Results for BondResults {
    fn reset(&mut self) {
        self.settlement_value = None;
        self.instrument.reset();
    }

    fn as_instrument_results(&self) -> Option<&InstrumentResults> {
        Some(&self.instrument)
    }
}

/// Engine base for bonds (the C++ `Bond::engine`).
pub type BondEngine = GenericEngine<BondArguments, BondResults>;

/// Base bond instrument.
///
/// Derived bonds build a [`Leg`] of coupons, hand it to [`Bond::new`] and append
/// their redemption flows with
/// [`add_redemptions_to_cashflows`](Bond::add_redemptions_to_cashflows).
/// [`Bond::from_coupons`] does the latter with a full (par) redemption.
pub struct Bond {
    base: InstrumentBase,
    settings: Shared<Settings<Date>>,
    settlement_days: Natural,
    calendar: Calendar,
    notional_schedule: Vec<Date>,
    notionals: Vec<Real>,
    cashflows: Leg,
    redemptions: Leg,
    maturity_date: Option<Date>,
    issue_date: Option<Date>,
    settlement_value: Option<Real>,
}

/// A bond price quote, per 100 of notional (the C++ `Bond::Price`): either the
/// [`Clean`](Self::Clean) price or the [`Dirty`](Self::Dirty) price that folds
/// in the accrued interest.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BondPrice {
    /// The quoted price, net of accrued interest.
    Clean(Real),
    /// The settlement price, accrued interest included.
    Dirty(Real),
}

impl BondPrice {
    /// The quoted amount, whichever convention it carries.
    pub fn amount(&self) -> Real {
        match self {
            BondPrice::Clean(amount) | BondPrice::Dirty(amount) => *amount,
        }
    }
}

/// Which price convention a quote is expressed in (the C++ `Bond::Price::Type`):
/// either the [`Clean`](Self::Clean) price, net of accrued interest, or the
/// [`Dirty`](Self::Dirty) price that folds it in. Unlike [`BondPrice`] this is a
/// bare discriminant, carried where only the convention matters (a bond helper's
/// `priceType_`), not the amount.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BondPriceType {
    /// The quoted price, net of accrued interest.
    Clean,
    /// The settlement price, accrued interest included.
    Dirty,
}

impl Bond {
    /// Builds the base of a bond from its coupons, before its redemptions are
    /// appended.
    ///
    /// The pre-redemption half of the C++ coupon constructor: it sorts the
    /// coupons by date, checks the issue date precedes the first payment, sets
    /// the maturity to the last coupon date and registers the bond with the
    /// settings evaluation date and every coupon. A derived bond then calls
    /// [`add_redemptions_to_cashflows`](Bond::add_redemptions_to_cashflows);
    /// [`from_coupons`](Bond::from_coupons) does it with a par redemption.
    ///
    /// # Errors
    ///
    /// The issue date, when given, must be earlier than the first payment date.
    pub fn new(
        settlement_days: Natural,
        calendar: Calendar,
        issue_date: Option<Date>,
        coupons: Leg,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Bond> {
        let base = InstrumentBase::new();
        settings.register_eval_date_observer(&base.observer());
        let mut bond = Bond {
            base,
            settings,
            settlement_days,
            calendar,
            notional_schedule: Vec::new(),
            notionals: Vec::new(),
            cashflows: coupons,
            redemptions: Vec::new(),
            maturity_date: None,
            issue_date,
            settlement_value: None,
        };

        if !bond.cashflows.is_empty() {
            bond.cashflows.sort_by_key(|a| a.date());
            if let Some(issue) = bond.issue_date {
                require!(
                    issue < bond.cashflows[0].date(),
                    "issue date must be earlier than first payment date"
                );
            }
            bond.maturity_date = Some(bond.cashflows.last().expect("non-empty").date());
        }

        for cashflow in &bond.cashflows {
            bond.base.register_with(cashflow.observable());
        }
        Ok(bond)
    }

    /// Builds a bond from its coupons, appending a full (par) redemption of the
    /// notional (the C++ coupon constructor).
    ///
    /// # Errors
    ///
    /// As for [`new`](Bond::new), plus the coupon leg must not be empty.
    pub fn from_coupons(
        settlement_days: Natural,
        calendar: Calendar,
        issue_date: Option<Date>,
        coupons: Leg,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Bond> {
        let mut bond = Bond::new(settlement_days, calendar, issue_date, coupons, settings)?;
        bond.add_redemptions_to_cashflows(&[])?;
        Ok(bond)
    }

    /// The number of settlement days.
    pub fn settlement_days(&self) -> Natural {
        self.settlement_days
    }

    /// The bond's calendar.
    pub fn calendar(&self) -> &Calendar {
        &self.calendar
    }

    /// The settings the bond reads its evaluation date from.
    pub fn settings(&self) -> &Settings<Date> {
        &self.settings
    }

    /// A shared handle to the settings the bond was built with, for wiring a
    /// pricing engine onto the same evaluation-date source (D5).
    pub fn settings_handle(&self) -> Shared<Settings<Date>> {
        Shared::clone(&self.settings)
    }

    /// The notionals the bond has carried, most recent last.
    pub fn notionals(&self) -> &[Real] {
        &self.notionals
    }

    /// All the bond's cash flows, redemptions included.
    pub fn cashflows(&self) -> &Leg {
        &self.cashflows
    }

    /// Just the redemption flows.
    pub fn redemptions(&self) -> &Leg {
        &self.redemptions
    }

    /// The bond's issue date, when given.
    pub fn issue_date(&self) -> Option<Date> {
        self.issue_date
    }

    /// The notional the bond carries at `date` (the settlement date when
    /// `None`), zero once the notional has been redeemed.
    ///
    /// # Errors
    ///
    /// The notional schedule must have been built (a derived bond appends its
    /// redemptions); without a `date` the evaluation date must be set.
    pub fn notional(&self, date: Option<Date>) -> QlResult<Real> {
        let date = match date {
            Some(date) => date,
            None => self.settlement_date(None)?,
        };
        let Some(&last) = self.notional_schedule.last() else {
            fail!("no notional schedule provided");
        };
        if date > last {
            return Ok(0.0);
        }
        let mut index = 1;
        while index < self.notional_schedule.len() && self.notional_schedule[index] < date {
            index += 1;
        }
        if date < self.notional_schedule[index] {
            Ok(self.notionals[index - 1])
        } else {
            Ok(self.notionals[index])
        }
    }

    /// The bond's maturity date: the last redemption date, or the last accrual
    /// end when no maturity was given.
    ///
    /// # Errors
    ///
    /// The cash-flow fall back needs a non-empty leg.
    pub fn maturity_date(&self) -> QlResult<Date> {
        match self.maturity_date {
            Some(date) => Ok(date),
            None => CashFlows::maturity_date(&self.cashflows),
        }
    }

    /// Overrides the maturity date with an explicit one (the C++ protected
    /// `maturityDate_` assignment a derived bond such as `FixedRateBond` makes).
    pub(crate) fn set_maturity_date(&mut self, date: Date) {
        self.maturity_date = Some(date);
    }

    /// The payment date of the next cash flow after `settlement` (the evaluation
    /// date's settlement date when `None`), or `None` once all flows have paid.
    ///
    /// The thin `Bond::nextCashFlowDate` wrapper (`bond.cpp:272`), delegating to
    /// [`CashFlows::next_cash_flow_date`] with settlement-date flows excluded
    /// (`bondfunctions.cpp:80`).
    ///
    /// # Errors
    ///
    /// Propagates the settlement-date resolution and the flow scan.
    pub fn next_cash_flow_date(&self, settlement: Option<Date>) -> QlResult<Option<Date>> {
        let settlement = match settlement {
            Some(date) => date,
            None => self.settlement_date(None)?,
        };
        CashFlows::next_cash_flow_date(
            &self.cashflows,
            &self.settings,
            Some(false),
            Some(settlement),
        )
    }

    /// The settlement date for a given date (the evaluation date when `None`).
    ///
    /// The date advanced by the settlement days, never earlier than the issue
    /// date.
    ///
    /// # Errors
    ///
    /// Without a `date` the evaluation date must be set.
    pub fn settlement_date(&self, date: Option<Date>) -> QlResult<Date> {
        let date = match date {
            Some(date) => date,
            None => {
                let Some(date) = self.settings.evaluation_date() else {
                    fail!("no evaluation date set: a bond needs a settlement date");
                };
                date
            }
        };
        let settlement = self.calendar.advance(
            date,
            self.settlement_days as Integer,
            TimeUnit::Days,
            BusinessDayConvention::Following,
            false,
        );
        Ok(match self.issue_date {
            Some(issue) => settlement.max(issue),
            None => settlement,
        })
    }

    /// The theoretical clean price, per 100 of notional
    /// (`dirtyPrice - accruedAmount`).
    ///
    /// # Errors
    ///
    /// Propagates the calculation and the settlement-date resolution.
    pub fn clean_price(&mut self) -> QlResult<Real> {
        let settlement = self.settlement_date(None)?;
        let dirty = self.dirty_price()?;
        let accrued = self.accrued_amount(Some(settlement))?;
        Ok(dirty - accrued)
    }

    /// The theoretical dirty price, per 100 of notional
    /// (`settlementValue * 100 / notional`).
    ///
    /// # Errors
    ///
    /// Propagates the calculation and the settlement-date resolution.
    pub fn dirty_price(&mut self) -> QlResult<Real> {
        let settlement = self.settlement_date(None)?;
        let current_notional = self.notional(Some(settlement))?;
        if current_notional == 0.0 {
            return Ok(0.0);
        }
        let value = self.settlement_value()?;
        Ok(value * 100.0 / current_notional)
    }

    /// The theoretical settlement value read from the engine.
    ///
    /// # Errors
    ///
    /// The engine must provide a settlement value.
    pub fn settlement_value(&mut self) -> QlResult<Real> {
        self.calculate()?;
        let Some(value) = self.settlement_value else {
            fail!("settlement value not provided");
        };
        Ok(value)
    }

    /// The interest accrued at `date` (the settlement date when `None`), per 100
    /// of notional; zero once the notional has been redeemed.
    ///
    /// # Errors
    ///
    /// Propagates the notional and accrual lookups.
    pub fn accrued_amount(&self, date: Option<Date>) -> QlResult<Real> {
        let settlement = match date {
            Some(date) => date,
            None => self.settlement_date(None)?,
        };
        let current_notional = self.notional(Some(settlement))?;
        if current_notional == 0.0 {
            return Ok(0.0);
        }
        let accrued = CashFlows::accrued_amount(
            &self.cashflows,
            &self.settings,
            Some(false),
            Some(settlement),
        )?;
        Ok(accrued * 100.0 / current_notional)
    }

    /// The yield (internal rate of return) that reprices the bond at `price`
    /// (the C++ `Bond::yield(Bond::Price, ...)`, `bond.cpp:239`).
    ///
    /// `settlement` defaults to the bond's settlement date, `accuracy` to
    /// `1e-8` (the distinct `Bond::yield` default of `bond.hpp:211`, tighter
    /// than the `1e-10` of the `BondFunctions`/`CashFlows` layers),
    /// `max_evaluations` to `100` and `guess` to `0.05`. A redeemed bond has a
    /// zero yield.
    ///
    /// # Errors
    ///
    /// The bond must be tradable at the settlement date, and the solve must
    /// converge (as [`BondFunctions::yield_rate`]).
    #[allow(clippy::too_many_arguments)]
    pub fn yield_rate(
        &self,
        price: BondPrice,
        day_counter: DayCounter,
        compounding: Compounding,
        frequency: Frequency,
        settlement: Option<Date>,
        accuracy: Option<Real>,
        max_evaluations: Option<usize>,
        guess: Option<Rate>,
    ) -> QlResult<Rate> {
        if self.notional(settlement)? == 0.0 {
            return Ok(0.0);
        }
        BondFunctions::yield_rate(
            self,
            price,
            day_counter,
            compounding,
            frequency,
            settlement,
            Some(accuracy.unwrap_or(1.0e-8)),
            max_evaluations,
            guess,
        )
    }

    /// Builds the redemption flows from the notional schedule and appends them
    /// to the cash flows (the protected `Bond::addRedemptionsToCashflows`).
    ///
    /// Called by a derived bond after its coupons are in place. The elements of
    /// `redemptions` scale the redemption amounts in base 100 (100 leaves the
    /// amount unchanged); a missing element defaults to the last given, or to a
    /// full redemption when none are.
    ///
    /// # Errors
    ///
    /// The cash flows must contain at least one coupon, and each built payment
    /// must be valid.
    pub fn add_redemptions_to_cashflows(&mut self, redemptions: &[Real]) -> QlResult<()> {
        self.calculate_notionals_from_cashflows()?;
        self.redemptions.clear();
        for i in 1..self.notional_schedule.len() {
            let r = if i < redemptions.len() {
                redemptions[i]
            } else if let Some(&last) = redemptions.last() {
                last
            } else {
                100.0
            };
            let amount = (r / 100.0) * (self.notionals[i - 1] - self.notionals[i]);
            let date = self.notional_schedule[i];
            let payment: Shared<dyn CashFlow> = if i < self.notional_schedule.len() - 1 {
                shared(AmortizingPayment::new(amount, date)?) as Shared<dyn CashFlow>
            } else {
                shared(Redemption::new(amount, date)?) as Shared<dyn CashFlow>
            };
            self.base.register_with(payment.observable());
            self.cashflows.push(Shared::clone(&payment));
            self.redemptions.push(payment);
        }
        self.cashflows.sort_by_key(|a| a.date());
        Ok(())
    }

    /// Collects the notional schedule from the coupons (the protected
    /// `Bond::calculateNotionalsFromCashflows`).
    fn calculate_notionals_from_cashflows(&mut self) -> QlResult<()> {
        self.notional_schedule.clear();
        self.notionals.clear();

        let mut last_payment_date = Date::null();
        self.notional_schedule.push(Date::null());
        for flow in &self.cashflows {
            let Some(coupon) = flow.as_coupon() else {
                continue;
            };
            let notional = coupon.nominal();
            if self.notionals.is_empty() {
                self.notionals.push(notional);
                last_payment_date = flow.date();
            } else if !close(notional, *self.notionals.last().expect("non-empty")) {
                self.notionals.push(notional);
                self.notional_schedule.push(last_payment_date);
                last_payment_date = flow.date();
            } else {
                last_payment_date = flow.date();
            }
        }
        require!(!self.notionals.is_empty(), "no coupons provided");
        self.notionals.push(0.0);
        self.notional_schedule.push(last_payment_date);
        Ok(())
    }
}

impl Instrument for Bond {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    fn is_expired(&self) -> QlResult<bool> {
        CashFlows::is_expired(&self.cashflows, &self.settings, None, None)
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(arguments) = (arguments as &mut dyn Any).downcast_mut::<BondArguments>() else {
            fail!("wrong argument type");
        };
        arguments.settlement_date = Some(self.settlement_date(None)?);
        arguments.cashflows = self.cashflows.clone();
        arguments.calendar = self.calendar.clone();
        Ok(())
    }

    fn setup_expired(&mut self) {
        let expired = InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        };
        self.base_mut().store_results(&expired);
        self.settlement_value = Some(0.0);
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let Some(results) = (results as &dyn Any).downcast_ref::<BondResults>() else {
            fail!("wrong result type");
        };
        self.settlement_value = results.settlement_value;
        self.base_mut().store_results(&results.instrument);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::FixedRateCoupon;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::pricingengine::PricingEngine;
    use crate::shared::{SharedMut, shared_mut};
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        settings
    }

    /// Two annual par coupons on a notional of 100, maturing 7 Jul 2028.
    fn coupons() -> Leg {
        let dc = Actual360::new();
        vec![
            shared(FixedRateCoupon::from_rate(
                Date::new(7, Month::July, 2027),
                100.0,
                0.05,
                dc.clone(),
                today(),
                Date::new(7, Month::July, 2027),
                None,
                None,
                None,
            )) as Shared<dyn CashFlow>,
            shared(FixedRateCoupon::from_rate(
                Date::new(7, Month::July, 2028),
                100.0,
                0.05,
                dc,
                Date::new(7, Month::July, 2027),
                Date::new(7, Month::July, 2028),
                None,
                None,
                None,
            )) as Shared<dyn CashFlow>,
        ]
    }

    fn par_bond() -> Bond {
        Bond::from_coupons(
            2,
            NullCalendar::new(),
            Some(Date::new(1, Month::July, 2026)),
            coupons(),
            settings_today(),
        )
        .unwrap()
    }

    struct StubEngine {
        base: BondEngine,
        settlement_value: Real,
    }

    impl AsObservable for StubEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for StubEngine {
        fn arguments_mut(&mut self) -> &mut dyn Arguments {
            self.base.arguments_mut()
        }

        fn results(&self) -> &dyn Results {
            self.base.results()
        }

        fn reset(&mut self) {
            self.base.reset();
        }

        fn calculate(&mut self) -> QlResult<()> {
            let value = self.settlement_value;
            let results = self.base.results_mut();
            results.settlement_value = Some(value);
            results.instrument.value = Some(value);
            Ok(())
        }
    }

    fn stub_engine(settlement_value: Real) -> SharedMut<StubEngine> {
        shared_mut(StubEngine {
            base: BondEngine::new(
                BondArguments {
                    settlement_date: None,
                    cashflows: Vec::new(),
                    calendar: NullCalendar::new(),
                },
                BondResults::default(),
            ),
            settlement_value,
        })
    }

    /// The redemption is appended after the coupons and carries the full
    /// notional, and the notional schedule spans par-until-maturity.
    #[test]
    fn a_par_bond_appends_a_full_redemption_and_builds_its_notional_schedule() {
        let bond = par_bond();

        assert_eq!(bond.cashflows().len(), 3, "two coupons plus the redemption");
        assert_eq!(bond.redemptions().len(), 1);
        assert_eq!(bond.redemptions()[0].amount().unwrap(), 100.0);
        assert_eq!(
            bond.redemptions()[0].date(),
            Date::new(7, Month::July, 2028)
        );
        assert_eq!(bond.notionals(), &[100.0, 0.0]);
        assert_eq!(
            bond.maturity_date().unwrap(),
            Date::new(7, Month::July, 2028)
        );
        assert_eq!(bond.issue_date(), Some(Date::new(1, Month::July, 2026)));
    }

    /// Settlement is the evaluation date advanced by the settlement days (the
    /// null calendar has no holidays), and the notional is par before maturity
    /// and zero after it.
    #[test]
    fn settlement_date_and_notional_track_the_schedule() {
        let bond = par_bond();

        let settlement = bond.settlement_date(None).unwrap();
        assert_eq!(settlement, Date::new(9, Month::July, 2026));
        assert_eq!(bond.notional(Some(settlement)).unwrap(), 100.0);
        assert_eq!(
            bond.notional(Some(Date::new(8, Month::July, 2028)))
                .unwrap(),
            0.0,
            "the notional is redeemed at maturity"
        );
        assert!(!bond.is_expired().unwrap());
    }

    /// The lazy price accessors read the engine's settlement value: at par
    /// notional the dirty price is the settlement value itself, and the clean
    /// price nets off the accrued interest.
    #[test]
    fn the_price_accessors_read_the_engine_settlement_value() {
        let mut bond = par_bond();
        bond.base_mut().set_pricing_engine(stub_engine(98.5));

        assert_eq!(bond.settlement_value().unwrap(), 98.5);
        assert_eq!(bond.dirty_price().unwrap(), 98.5);

        let settlement = bond.settlement_date(None).unwrap();
        let accrued = bond.accrued_amount(Some(settlement)).unwrap();
        assert!(accrued > 0.0, "the first coupon is accruing");
        assert_eq!(bond.clean_price().unwrap(), 98.5 - accrued);
    }

    /// A derived bond builds its base with [`Bond::new`] and appends redemptions
    /// itself; a redemption factor below 100 scales the amount down.
    #[test]
    fn a_derived_bond_can_scale_the_redemption() {
        let mut bond =
            Bond::new(2, NullCalendar::new(), None, coupons(), settings_today()).unwrap();
        assert!(
            bond.redemptions().is_empty(),
            "new leaves redemptions unset"
        );

        bond.add_redemptions_to_cashflows(&[100.0, 98.0]).unwrap();
        assert_eq!(bond.redemptions().len(), 1);
        assert_eq!(bond.redemptions()[0].amount().unwrap(), 98.0);
    }

    #[test]
    fn an_unset_evaluation_date_fails_the_settlement_date() {
        let settings = shared(Settings::new());
        let bond = Bond::from_coupons(2, NullCalendar::new(), None, coupons(), settings).unwrap();
        assert_eq!(
            bond.settlement_date(None).unwrap_err().message(),
            "no evaluation date set: a bond needs a settlement date"
        );
    }

    #[test]
    fn the_arguments_reject_a_missing_settlement_date_and_empty_leg() {
        let mut arguments = BondArguments {
            settlement_date: None,
            cashflows: Vec::new(),
            calendar: NullCalendar::new(),
        };
        assert_eq!(
            arguments.validate().unwrap_err().message(),
            "no settlement date provided"
        );
        arguments.settlement_date = Some(today());
        assert_eq!(
            arguments.validate().unwrap_err().message(),
            "no cash flow provided"
        );
        arguments.cashflows = coupons();
        assert!(arguments.validate().is_ok());
    }
}
