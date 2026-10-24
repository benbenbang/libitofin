use super::*;
use crate::cashflow::Leg;
use crate::cashflows::FixedRateCoupon;
use crate::settings::Settings;
use crate::shared::shared;
use crate::termstructures::yields::FlatForward;
use crate::time::calendars::NullCalendar;
use crate::time::date::Month;
use crate::time::daycounters::actual360::Actual360;

fn date(day: i32, year: i32) -> Date {
    Date::new(day, Month::January, year)
}

fn bond(notionals: &[f64], ex_coupon: bool) -> Bond {
    let leg: Leg = notionals
        .iter()
        .enumerate()
        .map(|(i, &notional)| {
            let year = 2025 + i as i32;
            let coupon = FixedRateCoupon::from_rate(
                date(1, year + 1),
                notional,
                0.04,
                Actual360::new(),
                date(1, year),
                date(1, year + 1),
                None,
                None,
                ex_coupon.then(|| date(25, year)),
            );
            shared(coupon) as _
        })
        .collect();
    Bond::from_coupons(0, NullCalendar::new(), None, leg, shared(Settings::new())).unwrap()
}

fn curve(rate: f64) -> FlatForward {
    FlatForward::with_rate(
        date(1, 2025),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )
}

fn near(actual: f64, expected: f64) {
    assert!(actual.is_finite() && expected.is_finite());
    assert!((actual - expected).abs() < 3e-13, "{actual} vs {expected}");
}

#[test]
fn price_atm_preserves_default_and_clean_dirty_round_trip() {
    let curve = curve(0.035);
    for face in [100.0, 375.0, 10_000.0] {
        let bond = bond(&[face; 3], false);
        let settlement = date(15, 2025);
        let dirty = BondFunctions::dirty_price(&bond, &curve, Some(settlement)).unwrap();
        let clean = BondFunctions::clean_price(&bond, &curve, Some(settlement)).unwrap();
        for price in [BondPrice::Clean(clean), BondPrice::Dirty(dirty)] {
            near(
                BondFunctions::atm_rate_from_price(&bond, &curve, price, Some(settlement)).unwrap(),
                0.04,
            );
        }
        near(
            BondFunctions::atm_rate(&bond, &curve, Some(settlement)).unwrap(),
            0.04,
        );
        let target = BondFunctions::atm_rate_from_price(
            &bond,
            &curve,
            BondPrice::Dirty(110.0),
            Some(settlement),
        )
        .unwrap();
        assert!((target - 0.04).abs() > 0.01);
    }
}

#[test]
fn price_atm_scales_by_current_amortizing_notional_at_payment() {
    let curve = curve(0.03);
    let bond = bond(&[375.0, 250.0, 125.0], false);
    let settlement = date(1, 2026);
    assert_eq!(bond.notional(Some(settlement)).unwrap(), 250.0);
    let target = 101.25;
    let discount = |year| (-0.03 * (date(1, year) - settlement) as f64 / 360.0).exp();
    let annuity = 250.0 * 365.0 / 360.0 * discount(2027) + 125.0 * 365.0 / 360.0 * discount(2028);
    let principal = 125.0 * discount(2027) + 125.0 * discount(2028);
    let expected = (target / 100.0 * 250.0 - principal) / annuity;
    near(
        BondFunctions::atm_rate_from_price(
            &bond,
            &curve,
            BondPrice::Dirty(target),
            Some(settlement),
        )
        .unwrap(),
        expected,
    );
    assert!((expected - (target / 100.0 * 375.0 - principal) / annuity).abs() > 0.2);
}

#[test]
fn price_atm_clean_conversion_ex_coupon_and_invalid_domains() {
    let curve = curve(0.03);
    let bond = bond(&[375.0; 3], true);
    let settlement = date(26, 2025);
    let accrued = bond.accrued_amount(Some(settlement)).unwrap();
    assert!(accrued < 0.0);
    let clean =
        BondFunctions::atm_rate_from_price(&bond, &curve, BondPrice::Clean(99.0), Some(settlement))
            .unwrap();
    let dirty = BondFunctions::atm_rate_from_price(
        &bond,
        &curve,
        BondPrice::Dirty(99.0 + accrued),
        Some(settlement),
    )
    .unwrap();
    near(clean, dirty);
    for amount in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        assert!(
            BondFunctions::atm_rate_from_price(
                &bond,
                &curve,
                BondPrice::Clean(amount),
                Some(settlement)
            )
            .is_err()
        );
    }
    for settlement in [date(1, 2028), date(2, 2028)] {
        assert!(
            BondFunctions::atm_rate_from_price(
                &bond,
                &curve,
                BondPrice::Dirty(100.0),
                Some(settlement)
            )
            .is_err()
        );
    }
}

#[test]
fn price_atm_zero_bps_zero_target_and_missing_default_date() {
    use crate::instruments::ZeroCouponBond;
    use crate::time::businessdayconvention::BusinessDayConvention;
    let curve = curve(0.0);
    let settlement = date(1, 2025);
    let zero = ZeroCouponBond::new(
        0,
        NullCalendar::new(),
        375.0,
        date(1, 2028),
        BusinessDayConvention::Unadjusted,
        100.0,
        None,
        shared(Settings::new()),
    )
    .unwrap()
    .into_bond();
    near(
        BondFunctions::atm_rate_from_price(
            &zero,
            &curve,
            BondPrice::Dirty(100.0),
            Some(settlement),
        )
        .unwrap(),
        0.0,
    );
    assert!(
        BondFunctions::atm_rate_from_price(&zero, &curve, BondPrice::Dirty(99.0), Some(settlement))
            .is_err()
    );
    assert!(
        BondFunctions::atm_rate_from_price(&zero, &curve, BondPrice::Clean(100.0), None).is_err()
    );
}
