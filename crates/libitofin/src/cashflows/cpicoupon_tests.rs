use super::*;
use crate::cashflow::CashFlow;
use crate::cashflows::{Coupon, CpiLeg};
use crate::indexes::Index;
use crate::indexes::inflation::UkRpi;
use crate::settings::Settings;
use crate::shared::shared;
use crate::time::{
    date::Month::*, daycounters::actual365fixed::Actual365Fixed, schedule::Schedule,
    timeunit::TimeUnit,
};

fn index() -> Shared<ZeroInflationIndex> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(15, June, 2025));
    let index = shared(UkRpi::new(settings));
    for (month, year, value) in [
        (October, 2024, 99.5),
        (November, 2024, 99.8),
        (December, 2024, 100.0),
        (January, 2025, 100.2),
        (February, 2025, 100.4),
        (March, 2025, 100.6),
        (April, 2025, 100.8),
        (May, 2025, 101.0),
    ] {
        index.add_fixing(Date::new(1, month, year), value).unwrap();
    }
    index
}
fn coupon(index: Shared<ZeroInflationIndex>, interpolation: CpiInterpolationType) -> CpiCoupon {
    let coupon = CpiCoupon::new(
        100.0,
        Date::new(15, July, 2025),
        100000.0,
        Date::new(15, January, 2025),
        Date::new(15, July, 2025),
        index,
        Period::new(3, TimeUnit::Months),
        interpolation,
        Actual365Fixed::new(),
        0.02,
        None,
        None,
        Some(Date::new(10, July, 2025)),
    )
    .unwrap();
    coupon.set_pricer(shared(CpiCouponPricer::default()));
    coupon
}
fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-10, "{actual} != {expected}");
}
#[test]
fn explicit_base_coupon_matches_independent_quantlib_flat_and_linear() {
    let index = index();
    let flat = coupon(index.clone(), CpiInterpolationType::Flat);
    near(flat.index_fixing().unwrap(), 100.8);
    near(flat.rate().unwrap(), 0.02016);
    near(Coupon::amount(&flat).unwrap(), 999.7150684931507);
    near(
        flat.accrued_amount(Date::new(15, June, 2025)).unwrap(),
        832.3616438356165,
    );
    near(
        flat.accrued_amount(Date::new(10, July, 2025)).unwrap(),
        -27.61643835616438,
    );
    let linear = coupon(index.clone(), CpiInterpolationType::Linear);
    near(linear.index_fixing().unwrap(), 100.8 + 0.2 * 14.0 / 31.0);
    assert_eq!(flat.accrued_amount(Date::new(16, July, 2025)).unwrap(), 0.0);
    Index::settings(&*index).clear_fixing(&index.name());
    index.add_fixing(Date::new(1, April, 2025), 101.8).unwrap();
    index.add_fixing(Date::new(1, May, 2025), 101.0).unwrap();
    near(flat.index_fixing().unwrap(), 101.8);
    near(linear.index_fixing().unwrap(), 101.8 - 0.8 * 14.0 / 31.0);
}
#[test]
fn cpi_principal_observes_contract_end_not_payment_and_growth_switch() {
    let index = index();
    for growth in [false, true] {
        let flow = CpiCashFlow::new(
            100000.0,
            index.clone(),
            100.0,
            Date::new(15, July, 2025),
            Period::new(3, TimeUnit::Months),
            CpiInterpolationType::Flat,
            Date::new(15, August, 2025),
            growth,
        )
        .unwrap();
        near(
            flow.amount().unwrap(),
            if growth { 800.0 } else { 100800.0 },
        );
        Index::settings(&*index).set_evaluation_date(Date::new(15, January, 2026));
        let missing = CpiCashFlow::new(
            100000.0,
            index.clone(),
            100.0,
            Date::new(15, January, 2026),
            Period::new(3, TimeUnit::Months),
            CpiInterpolationType::Flat,
            Date::new(15, January, 2026),
            growth,
        )
        .unwrap();
        assert!(missing.amount().is_err());
    }
}
#[test]
fn zero_rate_leg_coupon_needs_no_fixing_but_principal_does() {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(15, January, 2026));
    let index = shared(UkRpi::new(settings));
    let schedule = Schedule::from_dates(vec![
        Date::new(15, January, 2025),
        Date::new(15, July, 2025),
    ]);
    let flows = CpiLeg::new(schedule, index, 100.0, Period::new(3, TimeUnit::Months))
        .with_notional(100000.0)
        .with_fixed_rate(0.0)
        .build()
        .unwrap();
    assert_eq!(flows[0].amount().unwrap(), 0.0);
    assert!(flows[1].amount().is_err());
}
#[test]
fn explicit_base_and_date_guards_are_fallible() {
    let index = index();
    for base in [0.0, 1e-17, f64::NAN, f64::INFINITY] {
        assert!(
            CpiCashFlow::new(
                1.0,
                index.clone(),
                base,
                Date::new(1, July, 2025),
                Period::new(3, TimeUnit::Months),
                CpiInterpolationType::Flat,
                Date::new(1, July, 2025),
                false
            )
            .is_err()
        );
    }
    let c = coupon(index.clone(), CpiInterpolationType::Flat);
    assert!(c.index_ratio(Date::null()).is_err());
    assert!(
        checked_cpi_shift(Date::new(1, January, 1901), Period::new(1, TimeUnit::Days)).is_err()
    );
    assert!(
        checked_cpi_shift(
            Date::new(1, January, 2025),
            Period::new(i32::MIN, TimeUnit::Months)
        )
        .is_err()
    );
    assert!(
        checked_cpi_shift(
            Date::new(1, January, 2025),
            Period::new(i32::MAX, TimeUnit::Years)
        )
        .is_err()
    );
    Index::settings(&*index).set_evaluation_date(Date::new(1, January, 1901));
    assert!(c.rate().is_err());
}

#[test]
fn preceding_payment_before_accrual_end_and_zero_ex_period_are_valid() {
    use crate::time::{
        businessdayconvention::BusinessDayConvention as Bdc, calendars::WeekendsOnly,
    };
    let schedule = Schedule::from_dates(vec![
        Date::new(19, January, 2025),
        Date::new(19, July, 2025),
    ]);
    let leg = CpiLeg::new(schedule, index(), 100.0, Period::new(3, TimeUnit::Months))
        .with_notional(1000.0)
        .with_fixed_rate(0.02)
        .with_payment_calendar(WeekendsOnly::new())
        .with_payment_adjustment(Bdc::Preceding)
        .with_ex_coupon_period(
            Period::new(0, TimeUnit::Days),
            WeekendsOnly::new(),
            Bdc::Preceding,
            false,
        )
        .build()
        .unwrap();
    assert_eq!(leg[0].date(), Date::new(18, July, 2025));
    assert!(leg[0].amount().is_ok());
    assert_eq!(leg[0].ex_coupon_date(), None);
}
#[test]
fn safe_calendar_walk_and_linear_boundary_are_fallible() {
    use super::super::cpidates::checked_cpi_adjust;
    use crate::time::{
        businessdayconvention::BusinessDayConvention as Bdc, calendars::WeekendsOnly,
    };
    let calendar = WeekendsOnly::new();
    calendar.add_holiday(Date::min_date());
    assert!(checked_cpi_adjust(&calendar, Date::min_date(), Bdc::Preceding).is_err());
    calendar.add_holiday(Date::max_date());
    assert!(checked_cpi_adjust(&calendar, Date::max_date(), Bdc::Following).is_err());
    let i = index();
    assert!(
        observed_cpi(
            &i,
            Date::max_date(),
            Period::new(0, TimeUnit::Days),
            CpiInterpolationType::Linear
        )
        .is_err()
    );
}

#[test]
fn irregular_reference_period_after_2099_remains_supported() {
    use crate::time::{
        businessdayconvention::BusinessDayConvention as Bdc, calendars::NullCalendar,
    };
    let schedule = Schedule::with_metadata(
        vec![Date::new(15, January, 2101), Date::new(15, April, 2101)],
        NullCalendar::new(),
        Bdc::Unadjusted,
        None,
        Some(Period::new(6, TimeUnit::Months)),
        None,
        None,
        vec![false],
    );
    let leg = CpiLeg::new(schedule, index(), 100.0, Period::new(3, TimeUnit::Months))
        .with_notional(100.0)
        .with_fixed_rate(0.02)
        .build()
        .unwrap();
    assert_eq!(
        leg[0].as_coupon().unwrap().reference_period_end(),
        Date::new(15, July, 2101)
    );
}
