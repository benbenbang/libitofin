//! Analytics over a [`Leg`].
//!
//! Port of `ql/cashflows/cashflows.{hpp,cpp}`: the [`CashFlows`] namespace of
//! free functions that inspect a leg's dates and price it off a discount curve.
//! The `InterestRate` and `(Rate, DayCounter, Compounding, Frequency)`
//! overloads of the same four analytics are the yield half of the file and are
//! not ported here.
//!
//! ## Oracle
//!
//! The QuantLib test-suite never calls `bps`, `npvbps` or `atmRate`, in any
//! overload, and its only calls to the discount-curve [`npv`](CashFlows::npv)
//! come from the multiple-reset and inflation test files, which need index
//! machinery this port does not have. No ported test case stands behind these
//! numbers: the `npv` test adapts a case written against a different overload,
//! and the rest are checked against the definitions in `cashflows.cpp` and
//! against each other. Each test says which it is.
//!
//! ## Divergences from QuantLib
//!
//! C++ computes the coupon split twice: `bps` and `atmRate` through a
//! `BPSCalculator` `AcyclicVisitor`, `npvbps` through `coupon_cast`. The port
//! keeps only the `coupon_cast` path ([`CashFlow::as_coupon`]) and runs every
//! analytic off one pass. One consequence: `bps` evaluates the amount of every
//! surviving flow, where the visitor evaluates it only for the flows that are
//! not coupons. A coupon whose [`amount`](CashFlow::amount) fails - a floating
//! one missing its fixing - therefore makes `bps` fail, where C++ returns a
//! number.
//!
//! C++ deletes every constructor to make `CashFlows` a namespace; the port uses
//! an uninhabited type, which cannot be constructed at all.
//!
//! The settlement date is passed as an [`Option`] rather than a null [`Date`],
//! and resolving it against an unset evaluation date is an error rather than a
//! fall back to the system clock (D10). The [`Settings`] travel as an argument
//! (D5).
//!
//! `previousCashFlow` and `nextCashFlow` return iterators into the leg, which
//! the `*Date` and `*Amount` helpers dereference and, in the amount's case,
//! walk on from. The port returns the index instead, `leg.rend()` and
//! `leg.end()` becoming `None`. The `*Amount` helpers likewise return `None`
//! where C++ returns a default-constructed `Real`, which no caller can tell
//! from a genuinely zero flow.

use crate::cashflow::{CashFlow, Leg};
use crate::errors::QlResult;
use crate::event::reference_date;
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::{DiscountFactor, Rate, Real, Spread};

/// The `basisPoint_` of `cashflows.cpp`.
const BASIS_POINT: Spread = 1.0e-4;

/// The one pass over a leg that the discount-curve analytics share, as `npvbps`
/// already does in C++.
///
/// Every sum is raw: undivided by the discount factor at the NPV date and
/// unscaled by [`BASIS_POINT`], which is how [`CashFlows::atm_rate`] needs them.
#[derive(Default)]
struct Totals {
    /// `sum(amount * df)` over every surviving flow.
    npv: Real,
    /// `sum(nominal * accrual_period * df)` over the surviving coupons.
    bps: Real,
    /// `sum(amount * df)` over the surviving flows that are not coupons. Dead
    /// weight in `bps`, which is why C++ computes it there too, and load-bearing
    /// in [`atm_rate`](CashFlows::atm_rate).
    non_sens_npv: Real,
}

/// The `CashFlows` namespace of `cashflows.hpp`.
pub enum CashFlows {}

impl CashFlows {
    /// The earliest date the leg accrues from: the first accrual start over the
    /// coupons, and the payment date of anything that is not one.
    ///
    /// # Errors
    ///
    /// The leg must not be empty.
    pub fn start_date(leg: &Leg) -> QlResult<Date> {
        require!(!leg.is_empty(), "empty leg");
        Ok(leg
            .iter()
            .map(|flow| Self::accrual_start_or_payment(flow.as_ref()))
            .min()
            .expect("a non-empty leg has a minimum"))
    }

    /// The latest date the leg accrues to: the last accrual end over the
    /// coupons, and the payment date of anything that is not one.
    ///
    /// # Errors
    ///
    /// The leg must not be empty.
    pub fn maturity_date(leg: &Leg) -> QlResult<Date> {
        require!(!leg.is_empty(), "empty leg");
        Ok(leg
            .iter()
            .map(|flow| Self::accrual_end_or_payment(flow.as_ref()))
            .max()
            .expect("a non-empty leg has a maximum"))
    }

    /// The index of the last flow paying before or at `settlement_date`.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn previous_cash_flow(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<usize>> {
        for index in (0..leg.len()).rev() {
            if leg[index].has_occurred(settings, settlement_date, include_settlement_date_flows)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    /// The index of the first flow paying after `settlement_date`.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn next_cash_flow(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<usize>> {
        for (index, flow) in leg.iter().enumerate() {
            if !flow.has_occurred(settings, settlement_date, include_settlement_date_flows)? {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }

    /// The payment date of the [`previous_cash_flow`](Self::previous_cash_flow).
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn previous_cash_flow_date(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Date>> {
        let found = Self::previous_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?;
        Ok(found.map(|index| leg[index].date()))
    }

    /// The payment date of the [`next_cash_flow`](Self::next_cash_flow).
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`].
    pub fn next_cash_flow_date(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Date>> {
        let found = Self::next_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?;
        Ok(found.map(|index| leg[index].date()))
    }

    /// The total amount paid on the date of the
    /// [`previous_cash_flow`](Self::previous_cash_flow), summed over every flow
    /// sharing it.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`] and [`CashFlow::amount`].
    pub fn previous_cash_flow_amount(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Real>> {
        let Some(index) = Self::previous_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?
        else {
            return Ok(None);
        };
        Self::amount_on_payment_date(leg[..=index].iter().rev(), leg[index].date()).map(Some)
    }

    /// The total amount paid on the date of the
    /// [`next_cash_flow`](Self::next_cash_flow), summed over every flow sharing
    /// it.
    ///
    /// # Errors
    ///
    /// Propagates [`CashFlow::has_occurred`] and [`CashFlow::amount`].
    pub fn next_cash_flow_amount(
        leg: &Leg,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
    ) -> QlResult<Option<Real>> {
        let Some(index) = Self::next_cash_flow(
            leg,
            settings,
            include_settlement_date_flows,
            settlement_date,
        )?
        else {
            return Ok(None);
        };
        Self::amount_on_payment_date(leg[index..].iter(), leg[index].date()).map(Some)
    }

    /// The NPV of the leg: every surviving flow discounted to `npv_date`.
    ///
    /// An empty leg is worth nothing. `settlement_date` defaults to the
    /// evaluation date and `npv_date` to `settlement_date`.
    ///
    /// # Errors
    ///
    /// Propagates the flow and curve lookups; without a `settlement_date` the
    /// evaluation date must be set.
    pub fn npv(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        Ok(totals.npv / discount)
    }

    /// The change in [`npv`](Self::npv) for a uniform one-basis-point change in
    /// the rate the coupons pay. Flows that are not coupons contribute nothing.
    ///
    /// An empty leg has no sensitivity.
    ///
    /// # Errors
    ///
    /// As [`npv`](Self::npv).
    pub fn bps(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<Real> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        Ok(BASIS_POINT * totals.bps / discount)
    }

    /// The [`npv`](Self::npv) and the [`bps`](Self::bps), from one pass over the
    /// leg.
    ///
    /// # Errors
    ///
    /// As [`npv`](Self::npv).
    pub fn npvbps(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<(Real, Real)> {
        if leg.is_empty() {
            return Ok((0.0, 0.0));
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;
        Ok((totals.npv / discount, BASIS_POINT * totals.bps / discount))
    }

    /// The fixed rate at which the leg's coupons would reach `target_npv`,
    /// taking the flows that are not coupons as given.
    ///
    /// `target_npv` is measured at `npv_date`, as [`npv`](Self::npv) is, and
    /// defaults to the leg's own NPV, which makes the result the rate that
    /// reprices the leg. An empty leg has no rate, and neither has a target of
    /// zero once the flows that do not accrue have paid for themselves.
    ///
    /// # Errors
    ///
    /// As [`npv`](Self::npv), and the leg must have some basis-point
    /// sensitivity for a rate to exist at all.
    pub fn atm_rate(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
        target_npv: Option<Real>,
    ) -> QlResult<Rate> {
        if leg.is_empty() {
            return Ok(0.0);
        }
        let (totals, discount) = Self::measure(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement_date,
            npv_date,
        )?;

        let required = match target_npv {
            None => totals.npv,
            Some(target) => target * discount,
        };
        let target = required - totals.non_sens_npv;
        if target == 0.0 {
            return Ok(0.0);
        }
        require!(totals.bps != 0.0, "null bps: impossible atm rate");
        Ok(target / totals.bps)
    }

    /// The single pass the analytics share, and the discount factor at the NPV
    /// date they all divide by.
    fn measure(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement_date: Option<Date>,
        npv_date: Option<Date>,
    ) -> QlResult<(Totals, DiscountFactor)> {
        let settlement = reference_date(settings, settlement_date)?;
        let npv_date = npv_date.unwrap_or(settlement);
        let totals = Self::totals(
            leg,
            discount_curve,
            settings,
            include_settlement_date_flows,
            settlement,
        )?;
        Ok((totals, discount_curve.discount_date(npv_date, false)?))
    }

    fn totals(
        leg: &Leg,
        discount_curve: &dyn YieldTermStructure,
        settings: &Settings<Date>,
        include_settlement_date_flows: Option<bool>,
        settlement: Date,
    ) -> QlResult<Totals> {
        let settlement = Some(settlement);
        let mut totals = Totals::default();
        for flow in leg {
            if flow.has_occurred(settings, settlement, include_settlement_date_flows)?
                || flow.trading_ex_coupon(settings, settlement)?
            {
                continue;
            }
            let discount = discount_curve.discount_date(flow.date(), false)?;
            let amount = flow.amount()? * discount;
            totals.npv += amount;
            match flow.as_coupon() {
                Some(coupon) => totals.bps += coupon.nominal() * coupon.accrual_period() * discount,
                None => totals.non_sens_npv += amount,
            }
        }
        Ok(totals)
    }

    fn accrual_start_or_payment(flow: &dyn CashFlow) -> Date {
        match flow.as_coupon() {
            Some(coupon) => coupon.accrual_start_date(),
            None => flow.date(),
        }
    }

    fn accrual_end_or_payment(flow: &dyn CashFlow) -> Date {
        match flow.as_coupon() {
            Some(coupon) => coupon.accrual_end_date(),
            None => flow.date(),
        }
    }

    fn amount_on_payment_date<'a>(
        flows: impl Iterator<Item = &'a Shared<dyn CashFlow>>,
        payment_date: Date,
    ) -> QlResult<Real> {
        let mut total = 0.0;
        for flow in flows.take_while(|flow| flow.date() == payment_date) {
            total += flow.amount()?;
        }
        Ok(total)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::fixedratecoupon::FixedRateCoupon;
    use crate::cashflows::simplecashflow::{Redemption, SimpleCashFlow};
    use crate::shared::shared;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    fn accrual_start() -> Date {
        Date::new(15, Month::January, 2026)
    }

    fn accrual_end() -> Date {
        Date::new(15, Month::July, 2026)
    }

    fn payment() -> Date {
        Date::new(20, Month::July, 2026)
    }

    fn early_payment() -> Date {
        Date::new(10, Month::January, 2026)
    }

    fn coupon_amount() -> Real {
        100.0 * 0.03 * 181.0 / 360.0
    }

    /// An early plain flow, a coupon accruing across the evaluation date, and a
    /// redemption sharing the coupon's payment date.
    fn leg() -> Leg {
        vec![
            shared(SimpleCashFlow::new(5.0, early_payment())) as Shared<dyn CashFlow>,
            shared(FixedRateCoupon::from_rate(
                payment(),
                100.0,
                0.03,
                Actual360::new(),
                accrual_start(),
                accrual_end(),
                None,
                None,
                None,
            )) as Shared<dyn CashFlow>,
            shared(Redemption::new(100.0, payment())) as Shared<dyn CashFlow>,
        ]
    }

    fn settings() -> Settings<Date> {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings
    }

    /// The coupon is read through its accrual dates and the plain flows through
    /// their payment dates, so the leg starts before its first payment and
    /// matures after its last accrual.
    #[test]
    fn the_leg_spans_the_accrual_dates_of_its_coupons() {
        let leg = leg();

        assert_eq!(CashFlows::start_date(&leg).unwrap(), early_payment());
        assert_eq!(CashFlows::maturity_date(&leg).unwrap(), payment());
    }

    #[test]
    fn an_empty_leg_has_no_dates() {
        let leg = Leg::new();

        assert!(CashFlows::start_date(&leg).is_err());
        assert!(CashFlows::maturity_date(&leg).is_err());
    }

    #[test]
    fn the_previous_and_next_flows_straddle_the_settlement_date() {
        let (leg, settings) = (leg(), settings());
        let at = |date| {
            (
                CashFlows::previous_cash_flow(&leg, &settings, None, date).unwrap(),
                CashFlows::next_cash_flow(&leg, &settings, None, date).unwrap(),
            )
        };

        assert_eq!(at(None), (Some(0), Some(1)));
        assert_eq!(at(Some(early_payment() - 1)), (None, Some(0)));
        assert_eq!(at(Some(payment() + 1)), (Some(2), None));
    }

    #[test]
    fn the_flow_dates_follow_the_flows_they_are_read_off() {
        let (leg, settings) = (leg(), settings());
        let previous = |date| CashFlows::previous_cash_flow_date(&leg, &settings, None, date);
        let next = |date| CashFlows::next_cash_flow_date(&leg, &settings, None, date);

        assert_eq!(previous(None).unwrap(), Some(early_payment()));
        assert_eq!(next(None).unwrap(), Some(payment()));
        assert_eq!(previous(Some(early_payment() - 1)).unwrap(), None);
        assert_eq!(next(Some(payment() + 1)).unwrap(), None);
    }

    /// Both amounts sum every flow sharing the payment date they land on: the
    /// coupon and the redemption pay together.
    #[test]
    fn the_amounts_sum_the_flows_sharing_a_payment_date() {
        let (leg, settings) = (leg(), settings());
        let previous = |date| CashFlows::previous_cash_flow_amount(&leg, &settings, None, date);
        let next = |date| CashFlows::next_cash_flow_amount(&leg, &settings, None, date);

        assert_eq!(previous(None).unwrap(), Some(5.0));
        assert_eq!(previous(Some(early_payment() - 1)).unwrap(), None);
        assert_eq!(next(Some(payment() + 1)).unwrap(), None);

        let both = next(None).unwrap().unwrap();
        assert!((both - (coupon_amount() + 100.0)).abs() < 1e-13);
    }

    /// Without a settlement date the flows are measured against the evaluation
    /// date, which the port refuses to invent (D10).
    #[test]
    fn an_unset_evaluation_date_is_an_error() {
        let (leg, settings) = (leg(), Settings::new());

        assert!(CashFlows::previous_cash_flow(&leg, &settings, None, None).is_err());
        assert!(CashFlows::next_cash_flow(&leg, &settings, None, None).is_err());
    }
}

#[cfg(test)]
mod analytics_tests {
    use super::*;
    use crate::cashflows::fixedratecoupon::FixedRateCoupon;
    use crate::cashflows::fixedrateleg::FixedRateLeg;
    use crate::cashflows::simplecashflow::{Redemption, SimpleCashFlow};
    use crate::interestrate::{Compounding, InterestRate};
    use crate::shared::shared;
    use crate::termstructures::yields::FlatForward;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::nullcalendar::NullCalendar;
    use crate::time::date::{Month, Year};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::types::Rate;

    const NOMINAL: Real = 100.0;
    const RATE: Rate = 0.05;
    const FORWARD: Rate = 0.03;

    fn day(month: Month, year: Year) -> Date {
        Date::new(15, month, year)
    }

    fn today() -> Date {
        day(Month::January, 2026)
    }

    fn maturity() -> Date {
        day(Month::January, 2028)
    }

    fn settings() -> Settings<Date> {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings
    }

    /// A flat continuously-compounded curve on `Actual365Fixed`, so that
    /// `df(d) = exp(-FORWARD * (d - today) / 365)` by hand.
    fn curve(forward: Rate) -> FlatForward {
        FlatForward::with_rate(
            today(),
            forward,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )
    }

    /// The discount factor of [`curve`], from the definition rather than off
    /// the curve.
    fn df(date: Date) -> Real {
        (-FORWARD * f64::from(date - today()) / 365.0).exp()
    }

    /// The `Actual360` accrual period of a coupon, by hand.
    fn accrual(start: Date, end: Date) -> Real {
        f64::from(end - start) / 360.0
    }

    fn simple(rate: Rate) -> InterestRate {
        InterestRate::new(
            rate,
            Actual360::new(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .unwrap()
    }

    /// The accrual periods of [`fixed_leg`], which pays on its accrual ends.
    fn periods() -> [(Date, Date); 4] {
        [
            (today(), day(Month::July, 2026)),
            (day(Month::July, 2026), day(Month::January, 2027)),
            (day(Month::January, 2027), day(Month::July, 2027)),
            (day(Month::July, 2027), maturity()),
        ]
    }

    /// Four semiannual `Actual360` coupons on an unadjusted schedule.
    fn fixed_leg(rate: Rate) -> Leg {
        let schedule = MakeSchedule::new()
            .from(today())
            .to(maturity())
            .with_frequency(Frequency::Semiannual)
            .with_calendar(NullCalendar::new())
            .with_convention(BusinessDayConvention::Unadjusted)
            .backwards()
            .build();
        FixedRateLeg::new(schedule)
            .with_notional(NOMINAL)
            .with_interest_rate(simple(rate))
            .build()
            .unwrap()
    }

    /// An adaptation of the `CHECK_NPV` block of `cashflows.cpp::testSettings`
    /// (:157-179) onto the `YieldTermStructure` overload, which C++ does not
    /// call there. It prices the leg off `InterestRate(0.0, Actual365Fixed(),
    /// Continuous, Annual)`, whose every discount factor is 1.0; a flat 0%
    /// `FlatForward` gives the same factors, so the `2.0` and `3.0` expected
    /// across the `includeTodaysCashFlows` matrix carry over unchanged.
    #[test]
    fn the_npv_counts_the_flows_the_settlement_date_rule_admits() {
        let (settings, curve) = (settings(), curve(0.0));
        let leg: Leg = (0..3)
            .map(|i| shared(SimpleCashFlow::new(1.0, today() + i)) as Shared<dyn CashFlow>)
            .collect();
        let npv = |include| {
            CashFlows::npv(&leg, &curve, &settings, Some(include), Some(today()), None).unwrap()
        };

        settings.set_include_todays_cash_flows(None);
        assert_eq!(npv(false), 2.0);
        assert_eq!(npv(true), 3.0);

        settings.set_include_todays_cash_flows(Some(false));
        assert_eq!(npv(false), 2.0);
        assert_eq!(npv(true), 2.0);
    }

    /// `npv = sum(amount * df(date)) / df(npv_date)`, with the amounts and the
    /// discount factors hand-computed from the coupon and curve definitions.
    #[test]
    fn the_npv_discounts_every_flow_and_then_the_npv_date() {
        let (settings, curve, leg) = (settings(), curve(FORWARD), fixed_leg(RATE));
        let expected: Real = periods()
            .iter()
            .map(|&(start, end)| NOMINAL * RATE * accrual(start, end) * df(end))
            .sum();
        let npv = |npv_date| CashFlows::npv(&leg, &curve, &settings, None, None, npv_date).unwrap();

        assert!((npv(None) - expected).abs() < 1e-12);
        assert!((npv(Some(maturity())) - expected / df(maturity())).abs() < 1e-12);
    }

    /// `bps = 1e-4 * sum(nominal * accrual_period * df(date)) / df(npv_date)`,
    /// hand-computed from the same definitions.
    #[test]
    fn the_bps_sums_the_discounted_accruals_of_the_coupons() {
        let (settings, curve, leg) = (settings(), curve(FORWARD), fixed_leg(RATE));
        let expected: Real = 1.0e-4
            * periods()
                .iter()
                .map(|&(start, end)| NOMINAL * accrual(start, end) * df(end))
                .sum::<Real>();

        let bps = CashFlows::bps(&leg, &curve, &settings, None, None, None).unwrap();
        assert!((bps - expected).abs() < 1e-14);
    }

    /// The definition of a basis-point sensitivity: the change in NPV when the
    /// coupons pay one basis point more. A `Simple` coupon is linear in its
    /// rate, so the finite difference is exact but for the cancellation in the
    /// `(1 + rate * accrual) - 1` its compound factor goes through.
    #[test]
    fn the_bps_is_the_npv_change_for_a_one_basis_point_coupon_spread() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let npv =
            |rate| CashFlows::npv(&fixed_leg(rate), &curve, &settings, None, None, None).unwrap();

        let bumped = npv(RATE + 1.0e-4) - npv(RATE);
        let bps = CashFlows::bps(&fixed_leg(RATE), &curve, &settings, None, None, None).unwrap();
        assert!((bumped - bps).abs() < 1e-12);
    }

    /// A coupon trading ex-coupon is dropped though it has not been paid: the
    /// analytics filter on `has_occurred` *and* on `trading_ex_coupon`. The
    /// same coupon without an ex-coupon date is the control, without which a
    /// filter that dropped every flow would pass this.
    #[test]
    fn a_flow_trading_ex_coupon_is_left_out() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let (start, end) = periods()[0];
        let leg = |ex_coupon_date| {
            vec![shared(FixedRateCoupon::new(
                end,
                NOMINAL,
                simple(RATE),
                start,
                end,
                None,
                None,
                ex_coupon_date,
            )) as Shared<dyn CashFlow>]
        };
        let npv = |leg: &Leg| CashFlows::npv(leg, &curve, &settings, None, None, None).unwrap();
        let bps = |leg: &Leg| CashFlows::bps(leg, &curve, &settings, None, None, None).unwrap();

        let paying = leg(None);
        assert!((npv(&paying) - NOMINAL * RATE * accrual(start, end) * df(end)).abs() < 1e-13);
        assert!(bps(&paying) > 0.0);

        let ex_coupon = leg(Some(today()));
        assert_eq!(npv(&ex_coupon), 0.0);
        assert_eq!(bps(&ex_coupon), 0.0);
    }

    /// The empty-leg short circuit comes before the settlement date is
    /// resolved, so it stands with no evaluation date set.
    #[test]
    fn an_empty_leg_is_worth_nothing() {
        let (settings, curve, leg) = (Settings::new(), curve(FORWARD), Leg::new());

        assert_eq!(
            CashFlows::npv(&leg, &curve, &settings, None, None, None).unwrap(),
            0.0
        );
        assert_eq!(
            CashFlows::bps(&leg, &curve, &settings, None, None, None).unwrap(),
            0.0
        );
    }

    #[test]
    fn npvbps_returns_the_npv_and_the_bps() {
        let (settings, curve, leg) = (settings(), curve(FORWARD), fixed_leg(RATE));
        let at = Some(day(Month::January, 2027));

        let (npv, bps) = CashFlows::npvbps(&leg, &curve, &settings, None, None, at).unwrap();
        assert_eq!(
            npv,
            CashFlows::npv(&leg, &curve, &settings, None, None, at).unwrap()
        );
        assert_eq!(
            bps,
            CashFlows::bps(&leg, &curve, &settings, None, None, at).unwrap()
        );
    }

    /// With no target the ATM rate reprices the leg, so a leg of coupons all
    /// paying one rate is already at the money. The redemption is not a coupon:
    /// it lands in `nonSensNPV`, is subtracted from the target, and so leaves
    /// the rate alone. Passing the leg's own NPV as the target is that same
    /// computation with the `df(npv_date)` division undone.
    #[test]
    fn the_atm_rate_reprices_the_leg_and_ignores_the_flows_that_do_not_accrue() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let mut leg = fixed_leg(RATE);
        let atm = |leg: &Leg, target| {
            CashFlows::atm_rate(leg, &curve, &settings, None, None, None, target).unwrap()
        };

        assert!((atm(&leg, None) - RATE).abs() < 1e-14);

        leg.push(shared(Redemption::new(NOMINAL, maturity())) as Shared<dyn CashFlow>);
        assert!((atm(&leg, None) - RATE).abs() < 1e-14);

        let npv = CashFlows::npv(&leg, &curve, &settings, None, None, None).unwrap();
        assert!((atm(&leg, Some(npv)) - RATE).abs() < 1e-14);
    }

    /// A target NPV is quoted at the NPV date, so `atm_rate` scales it back up
    /// by `df(npv_date)` before dividing by the sensitivity. Every other test
    /// leaves `npv_date` at the settlement date, where that factor is 1.0 and a
    /// missing multiplication would go unseen.
    #[test]
    fn a_target_npv_is_scaled_by_the_discount_at_the_npv_date() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let leg = fixed_leg(RATE);
        let npv_date = maturity();
        let discount = curve.discount_date(npv_date, false).unwrap();
        assert!((discount - 1.0).abs() > 0.05);

        let npv = CashFlows::npv(&leg, &curve, &settings, None, None, Some(npv_date)).unwrap();
        let atm = CashFlows::atm_rate(
            &leg,
            &curve,
            &settings,
            None,
            None,
            Some(npv_date),
            Some(npv),
        )
        .unwrap();

        assert!((atm - RATE).abs() < 1e-14);
    }

    /// A target NPV of zero short-circuits before the sensitivity is consulted;
    /// a leg with no coupon at all has none to consult.
    #[test]
    fn the_atm_rate_needs_a_target_and_a_sensitivity() {
        let (settings, curve) = (settings(), curve(FORWARD));
        let leg = fixed_leg(RATE);
        let bare: Leg = vec![shared(Redemption::new(NOMINAL, maturity())) as Shared<dyn CashFlow>];
        let atm = |leg: &Leg, target| {
            CashFlows::atm_rate(leg, &curve, &settings, None, None, None, target)
        };

        assert_eq!(atm(&leg, Some(0.0)).unwrap(), 0.0);
        assert_eq!(atm(&bare, None).unwrap(), 0.0);
        assert!(atm(&bare, Some(0.0)).is_err());
    }

    /// The empty-leg short circuit comes before the settlement date is
    /// resolved, so it stands with no evaluation date set.
    #[test]
    fn an_empty_leg_has_no_atm_rate() {
        let (settings, curve, leg) = (Settings::new(), curve(FORWARD), Leg::new());

        assert_eq!(
            CashFlows::npvbps(&leg, &curve, &settings, None, None, None).unwrap(),
            (0.0, 0.0)
        );
        assert_eq!(
            CashFlows::atm_rate(&leg, &curve, &settings, None, None, None, None).unwrap(),
            0.0
        );
    }
}
