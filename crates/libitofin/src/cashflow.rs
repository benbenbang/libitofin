//! Cash flows.
//!
//! Port of `ql/cashflow.hpp`: an [`Event`] that pays an [`amount`](CashFlow::amount),
//! and [`Leg`], the ordered sequence of them. The concrete flows follow in
//! `cashflows/`, mirroring `ql/cashflows/`.
//!
//! `CashFlow` inherits `LazyObject` in C++ purely so that `Coupon` can cache -
//! `performCalculations()` on the base is empty - so the port leaves the
//! caching to the concrete flows that need it. The `accept(AcyclicVisitor&)`
//! override, the `isCoupon`/`coupon_cast` downcast pair and the
//! `earlier_than<CashFlow>` comparator have no counterpart in the port.

use crate::errors::QlResult;
use crate::event::{Event, reference_date};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::types::Real;

/// A payment of a known [`amount`](CashFlow::amount) on a known date.
///
/// Mirrors QuantLib's `CashFlow`. Implementors must implement
/// [`Event::has_occurred`] with [`cash_flow_has_occurred`] and not with
/// [`event_has_occurred`](crate::event::event_has_occurred), whose plain-event
/// rule ignores [`Settings::include_todays_cash_flows`]. Rust has no
/// specialization, so a provided method here would collide with the
/// supertrait's rather than override it; `Event` leaves `has_occurred`
/// required so that the choice cannot be made by omission.
pub trait CashFlow: Event {
    /// The amount paid at [`date`](Event::date), undiscounted.
    fn amount(&self) -> QlResult<Real>;

    /// The date from which the flow trades ex-coupon, when it has one.
    ///
    /// Required rather than defaulted to `None`: a `Coupon` stores this date on
    /// its base and cannot override a provided method here, so a default would
    /// let an implementor accrue ex-coupon while reporting
    /// [`trading_ex_coupon`](CashFlow::trading_ex_coupon) as `false`.
    fn ex_coupon_date(&self) -> Option<Date>;

    /// Whether the flow trades ex-coupon as of `ref_date` (the evaluation date
    /// when `None`).
    fn trading_ex_coupon(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
    ) -> QlResult<bool> {
        let Some(ex_coupon) = self.ex_coupon_date() else {
            return Ok(false);
        };
        Ok(ex_coupon <= reference_date(settings, ref_date)?)
    }
}

/// An ordered sequence of cash flows (`std::vector<ext::shared_ptr<CashFlow>>`).
pub type Leg = Vec<Shared<dyn CashFlow>>;

/// The `CashFlow::hasOccurred` rule of `cashflow.cpp`.
///
/// Identical to [`event_has_occurred`](crate::event::event_has_occurred) except
/// on the evaluation date itself, where [`Settings::include_todays_cash_flows`]
/// overrides `include_ref_date`.
///
/// An explicit `ref_date` resolves the reference date even with no evaluation
/// date set, but nothing then marks that date as today's, so the override is
/// skipped. C++ cannot reach this state: its evaluation date falls back to the
/// system clock, which this port refuses to do (D10).
pub fn cash_flow_has_occurred(
    date: Date,
    settings: &Settings<Date>,
    ref_date: Option<Date>,
    include_ref_date: Option<bool>,
) -> QlResult<bool> {
    let reference = reference_date(settings, ref_date)?;
    if date != reference {
        return Ok(date < reference);
    }
    let include_ref_date = if settings.evaluation_date() == Some(reference) {
        settings.include_todays_cash_flows().or(include_ref_date)
    } else {
        include_ref_date
    };
    Ok(!include_ref_date.unwrap_or_else(|| settings.include_reference_date_events()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::event_has_occurred;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::shared::shared;
    use crate::time::date::Month;

    struct SimpleCashFlow {
        amount: Real,
        date: Date,
        ex_coupon_date: Option<Date>,
        observable: Observable,
    }

    impl SimpleCashFlow {
        fn new(amount: Real, date: Date) -> Self {
            SimpleCashFlow {
                amount,
                date,
                ex_coupon_date: None,
                observable: Observable::new(),
            }
        }

        fn with_ex_coupon_date(amount: Real, date: Date, ex_coupon_date: Date) -> Self {
            SimpleCashFlow {
                ex_coupon_date: Some(ex_coupon_date),
                ..SimpleCashFlow::new(amount, date)
            }
        }
    }

    impl AsObservable for SimpleCashFlow {
        fn observable(&self) -> &Observable {
            &self.observable
        }
    }

    impl Event for SimpleCashFlow {
        fn date(&self) -> Date {
            self.date
        }

        fn has_occurred(
            &self,
            settings: &Settings<Date>,
            ref_date: Option<Date>,
            include_ref_date: Option<bool>,
        ) -> QlResult<bool> {
            cash_flow_has_occurred(self.date, settings, ref_date, include_ref_date)
        }
    }

    impl CashFlow for SimpleCashFlow {
        fn amount(&self) -> QlResult<Real> {
            Ok(self.amount)
        }

        fn ex_coupon_date(&self) -> Option<Date> {
            self.ex_coupon_date
        }
    }

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    /// The `leg` of `cashflows.cpp::testSettings`: unit flows at T+0, T+1, T+2.
    fn leg(settings: &Settings<Date>) -> Leg {
        settings.set_evaluation_date(today());
        (0..3)
            .map(|i| shared(SimpleCashFlow::new(1.0, today() + i)) as Shared<dyn CashFlow>)
            .collect()
    }

    /// `CHECK_INCLUSION`: flow `n` is included at `T+days` when it has not occurred.
    fn included(leg: &Leg, settings: &Settings<Date>, n: usize, days: i32) -> bool {
        !leg[n]
            .has_occurred(settings, Some(today() + days), None)
            .unwrap()
    }

    #[test]
    fn reference_date_payments_excluded_without_an_override() {
        let settings = Settings::new();
        let leg = leg(&settings);
        settings.set_include_reference_date_events(false);
        settings.set_include_todays_cash_flows(None);

        assert!(!included(&leg, &settings, 0, 0));
        assert!(!included(&leg, &settings, 0, 1));
        assert!(included(&leg, &settings, 1, 0));
        assert!(!included(&leg, &settings, 1, 1));
        assert!(!included(&leg, &settings, 1, 2));
        assert!(included(&leg, &settings, 2, 1));
        assert!(!included(&leg, &settings, 2, 2));
        assert!(!included(&leg, &settings, 2, 3));
    }

    #[test]
    fn excluding_todays_cash_flows_agrees_with_the_reference_date_rule() {
        let settings = Settings::new();
        let leg = leg(&settings);
        settings.set_include_reference_date_events(false);
        settings.set_include_todays_cash_flows(Some(false));

        assert!(!included(&leg, &settings, 0, 0));
        assert!(!included(&leg, &settings, 0, 1));
        assert!(included(&leg, &settings, 1, 0));
        assert!(!included(&leg, &settings, 1, 1));
        assert!(!included(&leg, &settings, 1, 2));
        assert!(included(&leg, &settings, 2, 1));
        assert!(!included(&leg, &settings, 2, 2));
        assert!(!included(&leg, &settings, 2, 3));
    }

    #[test]
    fn reference_date_payments_included_without_an_override() {
        let settings = Settings::new();
        let leg = leg(&settings);
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(None);

        assert!(included(&leg, &settings, 0, 0));
        assert!(!included(&leg, &settings, 0, 1));
        assert!(included(&leg, &settings, 1, 0));
        assert!(included(&leg, &settings, 1, 1));
        assert!(!included(&leg, &settings, 1, 2));
        assert!(included(&leg, &settings, 2, 1));
        assert!(included(&leg, &settings, 2, 2));
        assert!(!included(&leg, &settings, 2, 3));
    }

    #[test]
    fn including_todays_cash_flows_agrees_with_the_reference_date_rule() {
        let settings = Settings::new();
        let leg = leg(&settings);
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(true));

        assert!(included(&leg, &settings, 0, 0));
        assert!(!included(&leg, &settings, 0, 1));
        assert!(included(&leg, &settings, 1, 0));
        assert!(included(&leg, &settings, 1, 1));
        assert!(!included(&leg, &settings, 1, 2));
        assert!(included(&leg, &settings, 2, 1));
        assert!(included(&leg, &settings, 2, 2));
        assert!(!included(&leg, &settings, 2, 3));
    }

    #[test]
    fn todays_cash_flows_override_the_reference_date_rule() {
        let settings = Settings::new();
        let leg = leg(&settings);
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(false));

        assert!(!included(&leg, &settings, 0, 0));
        assert!(!included(&leg, &settings, 0, 1));
        assert!(included(&leg, &settings, 1, 0));
        assert!(included(&leg, &settings, 1, 1));
        assert!(!included(&leg, &settings, 1, 2));
        assert!(included(&leg, &settings, 2, 1));
        assert!(included(&leg, &settings, 2, 2));
        assert!(!included(&leg, &settings, 2, 3));
    }

    #[test]
    fn todays_cash_flows_do_not_override_a_plain_event() {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(false));

        assert!(!event_has_occurred(today(), &settings, None, None).unwrap());
        assert!(cash_flow_has_occurred(today(), &settings, None, None).unwrap());
    }

    /// With no evaluation date, no date is today's, so the override has nothing
    /// to key on and the plain reference-date rule stands.
    #[test]
    fn todays_cash_flows_are_skipped_without_an_evaluation_date() {
        let settings = Settings::new();
        let flow = SimpleCashFlow::new(1.0, today());
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(false));

        assert!(!flow.has_occurred(&settings, Some(today()), None).unwrap());

        settings.set_evaluation_date(today());
        assert!(flow.has_occurred(&settings, Some(today()), None).unwrap());
    }

    #[test]
    fn an_unset_evaluation_date_is_an_error() {
        let settings = Settings::new();
        let flow = SimpleCashFlow::new(1.0, today());

        assert!(flow.has_occurred(&settings, None, None).is_err());
        assert!(flow.has_occurred(&settings, Some(today()), None).is_ok());
    }

    /// The `None` ex-coupon date must short-circuit before the reference date
    /// is resolved, matching `cashflow.cpp`'s early `return false`: the unset
    /// evaluation date here would otherwise be an error.
    #[test]
    fn a_flow_without_an_ex_coupon_date_never_trades_ex_coupon() {
        let settings = Settings::new();
        let flow = SimpleCashFlow::new(1.0, today() + 30);

        assert_eq!(flow.ex_coupon_date(), None);
        assert!(!flow.trading_ex_coupon(&settings, None).unwrap());
    }

    #[test]
    fn an_ex_coupon_flow_needs_an_evaluation_date_when_no_reference_date_is_given() {
        let settings = Settings::new();
        let flow = SimpleCashFlow::with_ex_coupon_date(1.0, today() + 30, today() + 25);

        assert!(flow.trading_ex_coupon(&settings, None).is_err());

        settings.set_evaluation_date(today() + 25);
        assert!(flow.trading_ex_coupon(&settings, None).unwrap());
    }

    #[test]
    fn a_flow_trades_ex_coupon_from_its_ex_coupon_date_on() {
        let settings = Settings::new();
        let flow = SimpleCashFlow::with_ex_coupon_date(1.0, today() + 30, today() + 25);

        assert!(
            !flow
                .trading_ex_coupon(&settings, Some(today() + 24))
                .unwrap()
        );
        assert!(
            flow.trading_ex_coupon(&settings, Some(today() + 25))
                .unwrap()
        );
        assert!(
            flow.trading_ex_coupon(&settings, Some(today() + 26))
                .unwrap()
        );
    }

    #[test]
    fn a_leg_keeps_its_flows_in_order() {
        let settings = Settings::new();
        let leg = leg(&settings);

        let dates: Vec<Date> = leg.iter().map(|flow| flow.date()).collect();
        assert_eq!(dates, vec![today(), today() + 1, today() + 2]);
        assert_eq!(leg[0].amount().unwrap(), 1.0);
    }
}
