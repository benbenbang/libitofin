//! Analytics over a [`Leg`].
//!
//! Port of `ql/cashflows/cashflows.{hpp,cpp}`: the [`CashFlows`] namespace of
//! free functions that inspect a leg's dates and price it off a discount curve.
//!
//! ## Divergences from QuantLib
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
use crate::require;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::types::Real;

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
