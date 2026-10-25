use super::cpicoupon::observed_cpi;
use super::*;
use crate::indexes::{Index, inflation::UkRpi};
use crate::settings::Settings;
use crate::shared::{Shared, shared};
use crate::time::{
    businessdayconvention::BusinessDayConvention as Bdc,
    date::{Date, Month::*},
    daycounters::actual365fixed::Actual365Fixed,
    period::Period,
    schedule::Schedule,
    timeunit::TimeUnit,
};

fn market() -> Shared<crate::indexes::ZeroInflationIndex> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(15, January, 2025));
    let index = shared(UkRpi::new(settings));
    for (m, value) in [
        (January, 100.0),
        (February, 110.0),
        (May, 140.0),
        (June, 150.0),
    ] {
        index.add_fixing(Date::new(1, m, 2024), value).unwrap();
    }
    index
}
fn coupon(index: Shared<crate::indexes::ZeroInflationIndex>) -> CpiCoupon {
    let result = CpiCoupon::from_base_date(
        Date::new(31, January, 2024),
        Date::new(30, June, 2024),
        10000.0,
        Date::new(31, January, 2024),
        Date::new(30, June, 2024),
        index,
        Period::new(1, TimeUnit::Months),
        crate::indexes::inflationindex::CpiInterpolationType::Linear,
        Actual365Fixed::new(),
        0.025,
        None,
        None,
        None,
    )
    .unwrap();
    result.set_pricer(shared(CpiCouponPricer::default()));
    result
}
fn near(actual: f64, expected: f64) {
    assert!(
        actual.is_finite() && (actual - expected).abs() < 1e-10,
        "{actual} != {expected}"
    );
}
#[test]
fn month_end_coupon_and_principal_keep_distinct_native_base_observations() {
    let index = market();
    let coupon = coupon(index.clone());
    near(coupon.base_fixing().unwrap(), 100.0 + 10.0 * 28.0 / 29.0);
    assert_eq!(coupon.base_cpi(), f64::MAX);
    assert_eq!(coupon.base_date(), Some(Date::new(31, January, 2024)));
    let leg = CpiLeg::from_base_date(
        Schedule::from_dates(vec![
            Date::new(31, January, 2024),
            Date::new(30, June, 2024),
        ]),
        index,
        Some(Date::new(31, January, 2024)),
        Period::new(1, TimeUnit::Months),
    )
    .with_notional(10000.0)
    .with_fixed_rate(0.025)
    .with_payment_day_counter(Actual365Fixed::new())
    .with_payment_adjustment(Bdc::Unadjusted)
    .with_observation_interpolation(crate::indexes::inflationindex::CpiInterpolationType::Linear)
    .with_subtract_inflation_nominal(false)
    .build()
    .unwrap();
    near(leg[0].amount().unwrap(), Coupon::amount(&coupon).unwrap());
    near(
        leg[1].amount().unwrap(),
        10000.0 * (140.0 + 10.0 * 29.0 / 30.0) / (100.0 + 10.0 * 30.0 / 31.0),
    );
}
#[test]
fn new_base_date_paths_are_lazy_and_follow_later_fixings() {
    let index = market();
    index.clear_fixings();
    let coupon = coupon(index.clone());
    assert!(coupon.base_fixing().is_err());
    assert!(Coupon::amount(&coupon).is_err());
    for (m, value) in [
        (January, 100.0),
        (February, 110.0),
        (May, 140.0),
        (June, 150.0),
    ] {
        index.add_fixing(Date::new(1, m, 2024), value).unwrap();
    }
    let before = Coupon::amount(&coupon).unwrap();
    index.clear_fixings();
    for (m, value) in [
        (January, 90.0),
        (February, 100.0),
        (May, 140.0),
        (June, 150.0),
    ] {
        index.add_fixing(Date::new(1, m, 2024), value).unwrap();
    }
    assert!(Coupon::amount(&coupon).unwrap() > before);
    near(coupon.base_fixing().unwrap(), 90.0 + 10.0 * 28.0 / 29.0);
}
#[test]
fn explicit_max_base_is_not_reinterpreted_as_a_date_based_coupon() {
    let index = market();
    let coupon = CpiCoupon::new(
        f64::MAX,
        Date::new(30, June, 2024),
        10000.0,
        Date::new(31, January, 2024),
        Date::new(30, June, 2024),
        index,
        Period::new(1, TimeUnit::Months),
        crate::indexes::inflationindex::CpiInterpolationType::Flat,
        Actual365Fixed::new(),
        0.025,
        None,
        None,
        None,
    )
    .unwrap();
    assert_eq!(coupon.base_cpi(), f64::MAX);
    assert_eq!(coupon.base_fixing().unwrap(), f64::MAX);
    assert_eq!(coupon.base_date(), None);
    assert_eq!(
        coupon
            .index_ratio(Date::new(30, June, 2024))
            .unwrap()
            .to_bits(),
        (140.0 / f64::MAX).to_bits(),
    );
}
#[test]
fn default_date_needs_a_coupon_schedule_but_explicit_single_payment_is_supported() {
    let index = market();
    let schedule = Schedule::from_dates(vec![Date::new(30, June, 2024)]);
    let leg = CpiLeg::from_base_date(
        schedule.clone(),
        index.clone(),
        None,
        Period::new(1, TimeUnit::Months),
    )
    .with_notional(10000.0);
    assert!(leg.build().is_err());
    let leg = CpiLeg::from_base_date(
        schedule.clone(),
        index.clone(),
        Some(Date::new(31, January, 2024)),
        Period::new(1, TimeUnit::Months),
    )
    .with_notional(10000.0)
    .build()
    .unwrap();
    near(leg[0].amount().unwrap(), 4000.0);
    let old = CpiLeg::new(schedule, index, 100.0, Period::new(1, TimeUnit::Months))
        .with_notional(10000.0)
        .build()
        .unwrap();
    near(old[0].amount().unwrap(), 4000.0);
}
#[test]
fn zero_rate_leg_does_not_fetch_a_base_fixing_for_fixed_coupons() {
    let index = market();
    index.clear_fixings();
    let leg = CpiLeg::from_base_date(
        Schedule::from_dates(vec![
            Date::new(31, January, 2024),
            Date::new(30, June, 2024),
        ]),
        index,
        None,
        Period::new(1, TimeUnit::Months),
    )
    .with_notional(10000.0)
    .with_fixed_rate(0.0)
    .build()
    .unwrap();
    assert_eq!(leg[0].amount().unwrap(), 0.0);
    assert!(leg[1].amount().is_err());
}
#[test]
fn linear_observation_checks_both_next_period_bounds_before_core_arithmetic() {
    let index = market();
    assert!(
        observed_cpi(
            &index,
            Date::max_date(),
            Period::new(1, TimeUnit::Months),
            crate::indexes::inflationindex::CpiInterpolationType::Linear
        )
        .is_err()
    );
    assert!(
        CpiCoupon::from_base_date(
            Date::max_date(),
            Date::new(30, June, 2024),
            1.0,
            Date::new(31, January, 2024),
            Date::new(30, June, 2024),
            index,
            Period::new(1, TimeUnit::Months),
            crate::indexes::inflationindex::CpiInterpolationType::Flat,
            Actual365Fixed::new(),
            0.025,
            None,
            None,
            None,
        )
        .is_err()
    );
}

#[test]
fn extracted_live_principal_and_coupon_reject_mutated_settings_and_max_linear_period() {
    let index = market();
    let coupon = coupon(index.clone());
    let leg = CpiLeg::from_base_date(
        Schedule::from_dates(vec![
            Date::new(31, January, 2024),
            Date::new(30, June, 2024),
        ]),
        index.clone(),
        Some(Date::new(31, January, 2024)),
        Period::new(1, TimeUnit::Months),
    )
    .with_notional(10000.0)
    .with_fixed_rate(0.025)
    .build()
    .unwrap();
    let principal = leg.last().unwrap().clone();
    Index::settings(&*index).set_evaluation_date(Date::min_date());
    assert!(Coupon::amount(&coupon).is_err());
    assert!(principal.amount().is_err());
    Index::settings(&*index).set_evaluation_date(Date::new(15, January, 2025));
    assert!(Coupon::amount(&coupon).is_ok());
    assert!(principal.amount().is_ok());
    let base_date = Date::new(15, December, 2199);
    let coupon = CpiCoupon::from_base_date(
        base_date,
        base_date,
        10000.0,
        Date::new(1, June, 2199),
        base_date,
        index.clone(),
        Period::new(0, TimeUnit::Months),
        crate::indexes::inflationindex::CpiInterpolationType::Linear,
        Actual365Fixed::new(),
        0.025,
        None,
        None,
        None,
    )
    .unwrap();
    coupon.set_pricer(shared(CpiCouponPricer::default()));
    let leg = CpiLeg::from_base_date(
        Schedule::from_dates(vec![Date::new(1, June, 2199), base_date]),
        index,
        Some(base_date),
        Period::new(0, TimeUnit::Months),
    )
    .with_notional(10000.0)
    .with_fixed_rate(0.025)
    .with_observation_interpolation(crate::indexes::inflationindex::CpiInterpolationType::Linear)
    .with_payment_adjustment(Bdc::Unadjusted)
    .build()
    .unwrap();
    assert!(Coupon::amount(&coupon).is_err());
    assert!(leg.last().unwrap().amount().is_err());
}
