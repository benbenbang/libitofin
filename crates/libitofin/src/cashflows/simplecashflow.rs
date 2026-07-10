//! Predetermined cash flows.
//!
//! Port of `ql/cashflows/simplecashflow.hpp`. [`Redemption`] and
//! [`AmortizingPayment`] add no state or behaviour to [`SimpleCashFlow`] in
//! C++: they exist so that an `AcyclicVisitor` can tell them apart. The port
//! has no visitor, so they wrap a `SimpleCashFlow` and keep the type identity
//! that gives them their meaning.

use crate::cashflow::{CashFlow, cash_flow_has_occurred};
use crate::cashflows::Coupon;
use crate::errors::QlResult;
use crate::event::Event;
use crate::patterns::observable::{AsObservable, Observable};
use crate::settings::Settings;
use crate::time::date::Date;
use crate::types::Real;

/// A payment of a predetermined amount on a given date.
pub struct SimpleCashFlow {
    amount: Real,
    date: Date,
    observable: Observable,
}

impl SimpleCashFlow {
    /// Creates a flow paying `amount` on `date`.
    pub fn new(amount: Real, date: Date) -> Self {
        SimpleCashFlow {
            amount,
            date,
            observable: Observable::new(),
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
        None
    }

    fn as_coupon(&self) -> Option<&dyn Coupon> {
        None
    }
}

macro_rules! simple_cash_flow_alias {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        pub struct $name(SimpleCashFlow);

        impl $name {
            /// Creates a flow paying `amount` on `date`.
            pub fn new(amount: Real, date: Date) -> Self {
                $name(SimpleCashFlow::new(amount, date))
            }
        }

        impl AsObservable for $name {
            fn observable(&self) -> &Observable {
                self.0.observable()
            }
        }

        impl Event for $name {
            fn date(&self) -> Date {
                self.0.date()
            }

            fn has_occurred(
                &self,
                settings: &Settings<Date>,
                ref_date: Option<Date>,
                include_ref_date: Option<bool>,
            ) -> QlResult<bool> {
                self.0.has_occurred(settings, ref_date, include_ref_date)
            }
        }

        impl CashFlow for $name {
            fn amount(&self) -> QlResult<Real> {
                self.0.amount()
            }

            fn ex_coupon_date(&self) -> Option<Date> {
                self.0.ex_coupon_date()
            }

            fn as_coupon(&self) -> Option<&dyn Coupon> {
                self.0.as_coupon()
            }
        }
    };
}

simple_cash_flow_alias! {
    /// The redemption of a bond's notional: a [`SimpleCashFlow`] that cash-flow
    /// analysis can single out.
    Redemption
}

simple_cash_flow_alias! {
    /// A repayment of principal: a [`SimpleCashFlow`] that cash-flow analysis
    /// can single out.
    AmortizingPayment
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflow::Leg;
    use crate::shared::{Shared, shared};
    use crate::time::date::Month;

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
    fn a_simple_cash_flow_pays_its_amount_on_its_date() {
        let flow = SimpleCashFlow::new(105.0, today());

        assert_eq!(flow.date(), today());
        assert_eq!(flow.amount().unwrap(), 105.0);
        assert_eq!(flow.ex_coupon_date(), None);
    }

    /// `cashflows.cpp::testSettings`, cases 1 and 2: reference-date payments
    /// excluded, with and without an explicit override at today's date.
    #[test]
    fn reference_date_payments_are_excluded() {
        for todays_flows in [None, Some(false)] {
            let settings = Settings::new();
            let leg = leg(&settings);
            settings.set_include_reference_date_events(false);
            settings.set_include_todays_cash_flows(todays_flows);

            assert!(!included(&leg, &settings, 0, 0));
            assert!(!included(&leg, &settings, 0, 1));
            assert!(included(&leg, &settings, 1, 0));
            assert!(!included(&leg, &settings, 1, 1));
            assert!(!included(&leg, &settings, 1, 2));
            assert!(included(&leg, &settings, 2, 1));
            assert!(!included(&leg, &settings, 2, 2));
            assert!(!included(&leg, &settings, 2, 3));
        }
    }

    /// `cashflows.cpp::testSettings`, cases 3 and 4: reference-date payments
    /// included, with and without an explicit override at today's date.
    #[test]
    fn reference_date_payments_are_included() {
        for todays_flows in [None, Some(true)] {
            let settings = Settings::new();
            let leg = leg(&settings);
            settings.set_include_reference_date_events(true);
            settings.set_include_todays_cash_flows(todays_flows);

            assert!(included(&leg, &settings, 0, 0));
            assert!(!included(&leg, &settings, 0, 1));
            assert!(included(&leg, &settings, 1, 0));
            assert!(included(&leg, &settings, 1, 1));
            assert!(!included(&leg, &settings, 1, 2));
            assert!(included(&leg, &settings, 2, 1));
            assert!(included(&leg, &settings, 2, 2));
            assert!(!included(&leg, &settings, 2, 3));
        }
    }

    /// `cashflows.cpp::testSettings`, case 5: today's flows override the
    /// reference-date rule, which is what forbids
    /// [`event_has_occurred`](crate::event::event_has_occurred) here.
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

    /// No test-suite case covers `Redemption` or `AmortizingPayment`; the
    /// header makes them pass-through `SimpleCashFlow`s, including the
    /// today's-cash-flows rule they inherit with it.
    #[test]
    fn a_redemption_and_an_amortizing_payment_behave_as_simple_cash_flows() {
        let settings = Settings::new();
        settings.set_evaluation_date(today());
        settings.set_include_reference_date_events(true);
        settings.set_include_todays_cash_flows(Some(false));

        let redemption = Redemption::new(100.0, today());
        let amortizing = AmortizingPayment::new(2.5, today());

        assert_eq!(redemption.date(), today());
        assert_eq!(redemption.amount().unwrap(), 100.0);
        assert!(redemption.has_occurred(&settings, None, None).unwrap());

        assert_eq!(amortizing.date(), today());
        assert_eq!(amortizing.amount().unwrap(), 2.5);
        assert!(amortizing.has_occurred(&settings, None, None).unwrap());
    }
}
