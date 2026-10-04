//! QuantLib 1.43 compiled-oracle cases for `cashflows.cpp` spread overloads and
//! `bondfunctions.cpp` curve/yield prices. Reproduce with the High28 oracle
//! generator in `tests/fixtures/high28`. Price tolerance is 1e-10; inversion
//! tolerance is 1e-10. Values are independent of the Rust implementation.

use libitofin::cashflow::CashFlow;
use libitofin::cashflows::FixedRateCoupon;
use libitofin::instruments::Bond;
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::bond::BondFunctions;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;

fn date(day: i32, month: Month, year: i32) -> Date {
    Date::new(day, month, year)
}

fn curve(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        date(1, Month::July, 2026),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn settings() -> Shared<Settings<Date>> {
    let s = shared(Settings::new());
    s.set_evaluation_date(date(1, Month::July, 2026));
    s
}

#[test]
fn future_settlement_zero_coupon_price_is_discount_ratio_not_reference_npv() {
    let issue = date(7, Month::July, 2026);
    let maturity = date(7, Month::July, 2028);
    let settlement = date(10, Month::July, 2027);
    let coupon: Shared<dyn CashFlow> = shared(FixedRateCoupon::from_rate(
        maturity,
        1e308,
        0.0,
        Actual360::new(),
        issue,
        maturity,
        None,
        None,
        None,
    ));
    let bond = Bond::from_coupons(
        0,
        NullCalendar::new(),
        Some(issue),
        vec![coupon],
        settings(),
    )
    .unwrap();
    let curve = curve(0.03);
    let expected = 100.0 * (-0.03 * 363.0_f64 / 360.0).exp();
    let dirty = BondFunctions::dirty_price(&bond, curve.as_ref(), Some(settlement)).unwrap();
    let clean = BondFunctions::clean_price(&bond, curve.as_ref(), Some(settlement)).unwrap();
    assert!((dirty - expected).abs() < 1e-10);
    assert_eq!(clean, dirty);
}

#[test]
fn large_representable_notional_has_finite_accrued_and_quoted_prices() {
    let issue = date(7, Month::July, 2026);
    let maturity = date(7, Month::July, 2028);
    let settlement = date(10, Month::July, 2027);
    let coupon: Shared<dyn CashFlow> = shared(FixedRateCoupon::from_rate(
        maturity,
        1e308,
        0.01,
        Actual360::new(),
        issue,
        maturity,
        None,
        None,
        None,
    ));
    let bond = Bond::from_coupons(
        0,
        NullCalendar::new(),
        Some(issue),
        vec![coupon],
        settings(),
    )
    .unwrap();
    let curve = curve(0.03);
    let accrued = 100.0 * 0.01 * 368.0 / 360.0;
    let dirty = 100.0 * (1.0 + 0.01 * 731.0 / 360.0) * (-0.03 * 363.0_f64 / 360.0).exp();
    assert!((bond.accrued_amount(Some(settlement)).unwrap() - accrued).abs() < 1e-12);
    assert!(
        (BondFunctions::dirty_price(&bond, curve.as_ref(), Some(settlement)).unwrap() - dirty)
            .abs()
            < 1e-10
    );
    assert!(
        (BondFunctions::clean_price(&bond, curve.as_ref(), Some(settlement)).unwrap()
            - (dirty - accrued))
            .abs()
            < 1e-10
    );
}
