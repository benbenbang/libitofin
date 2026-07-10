//! A concrete coupon written outside `libitofin`, using only its public API.
//!
//! Integration tests compile as their own crate, so this fails to build if
//! anything a downstream implementor of [`Coupon`] needs is private.

use libitofin::cashflow::{CashFlow, cash_flow_has_occurred};
use libitofin::cashflows::{Coupon, CouponBase};
use libitofin::errors::QlResult;
use libitofin::event::Event;
use libitofin::patterns::observable::{AsObservable, Observable};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::types::{Rate, Real};

/// A coupon paying a fixed rate, in the shape a downstream crate would write.
struct SimpleFixedRateCoupon {
    base: CouponBase,
    rate: Rate,
    day_counter: DayCounter,
    observable: Observable,
}

impl AsObservable for SimpleFixedRateCoupon {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl Event for SimpleFixedRateCoupon {
    fn date(&self) -> Date {
        self.base.payment_date()
    }

    fn has_occurred(
        &self,
        settings: &Settings<Date>,
        ref_date: Option<Date>,
        include_ref_date: Option<bool>,
    ) -> QlResult<bool> {
        cash_flow_has_occurred(self.date(), settings, ref_date, include_ref_date)
    }
}

impl CashFlow for SimpleFixedRateCoupon {
    fn amount(&self) -> QlResult<Real> {
        Ok(self.nominal() * self.rate * self.accrual_period())
    }

    fn ex_coupon_date(&self) -> Option<Date> {
        self.base.ex_coupon_date()
    }

    fn as_coupon(&self) -> Option<&dyn Coupon> {
        Some(self)
    }
}

impl Coupon for SimpleFixedRateCoupon {
    fn coupon_base(&self) -> &CouponBase {
        &self.base
    }

    fn rate(&self) -> QlResult<Rate> {
        Ok(self.rate)
    }

    fn day_counter(&self) -> DayCounter {
        self.day_counter.clone()
    }

    fn accrued_amount(&self, date: Date) -> QlResult<Real> {
        Ok(self.nominal() * self.rate * self.accrued_period(date))
    }
}

fn coupon(ex_coupon_date: Option<Date>) -> SimpleFixedRateCoupon {
    SimpleFixedRateCoupon {
        base: CouponBase::new(
            Date::new(20, Month::July, 2026),
            1_000.0,
            Date::new(15, Month::January, 2026),
            Date::new(15, Month::July, 2026),
            None,
            None,
            ex_coupon_date,
        ),
        rate: 0.05,
        day_counter: Actual365Fixed::new(),
        observable: Observable::new(),
    }
}

#[test]
fn a_coupon_defined_outside_the_crate_accrues_and_pays() {
    let coupon = coupon(None);

    assert_eq!(coupon.nominal(), 1_000.0);
    assert_eq!(coupon.rate().unwrap(), 0.05);
    assert_eq!(coupon.accrual_days(), 181);
    assert!((coupon.accrual_period() - 181.0 / 365.0).abs() < 1e-15);
    assert!((coupon.amount().unwrap() - 1_000.0 * 0.05 * 181.0 / 365.0).abs() < 1e-12);

    let mid = Date::new(15, Month::April, 2026);
    assert_eq!(coupon.accrued_days(mid), 90);
    assert!((coupon.accrued_amount(mid).unwrap() - 1_000.0 * 0.05 * 90.0 / 365.0).abs() < 1e-12);
}

#[test]
fn an_externally_defined_coupon_goes_negative_ex_coupon() {
    let ex_coupon = Date::new(1, Month::July, 2026);
    let coupon = coupon(Some(ex_coupon));

    assert!((coupon.accrued_period(ex_coupon) + 14.0 / 365.0).abs() < 1e-15);
    assert_eq!(coupon.accrued_days(ex_coupon), 167);
    assert!(coupon.accrued_amount(ex_coupon).unwrap() < 0.0);
}

#[test]
fn an_externally_defined_coupon_is_a_cash_flow_in_a_leg() {
    let settings = Settings::new();
    settings.set_evaluation_date(Date::new(1, Month::February, 2026));

    let leg: libitofin::cashflow::Leg = vec![shared(coupon(None)) as Shared<dyn CashFlow>];

    assert_eq!(leg[0].date(), Date::new(20, Month::July, 2026));
    assert!(!leg[0].has_occurred(&settings, None, None).unwrap());
    assert!(!leg[0].trading_ex_coupon(&settings, None).unwrap());
}
