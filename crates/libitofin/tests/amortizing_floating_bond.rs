use libitofin::currency::Currency;
use libitofin::handle::Handle;
use libitofin::indexes::{IborIndex, Index};
use libitofin::instruments::FloatingRateBond;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::{NullCalendar, WeekendsOnly};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::period::Period;
use libitofin::time::schedule::Schedule;
use libitofin::time::timeunit::TimeUnit;

fn dates() -> Vec<Date> {
    vec![
        Date::new(5, Month::October, 2026),
        Date::new(5, Month::January, 2027),
        Date::new(5, Month::April, 2027),
        Date::new(5, Month::July, 2027),
        Date::new(5, Month::October, 2027),
    ]
}
fn market() -> (Shared<Settings<Date>>, Shared<IborIndex>) {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(10, Month::October, 2027));
    let index = shared(IborIndex::new(
        "AmortizingCore".into(),
        Period::new(6, TimeUnit::Months),
        2,
        Currency::eur(),
        NullCalendar::new(),
        Bdc::Following,
        false,
        Actual360::new(),
        Handle::empty(),
        settings.clone(),
    ));
    (settings, index)
}
fn construct(
    notionals: Vec<f64>,
    settings: Shared<Settings<Date>>,
    index: Shared<IborIndex>,
) -> libitofin::errors::QlResult<FloatingRateBond> {
    FloatingRateBond::new_amortizing(
        0,
        notionals,
        Schedule::from_dates(dates()),
        index,
        Actual360::new(),
        Bdc::Following,
        None,
        vec![0.001],
        None,
        None,
        NullCalendar::new(),
        Bdc::Unadjusted,
        false,
        Bdc::Preceding,
        settings,
    )
}
fn near(actual: f64, expected: f64) {
    assert!(actual.is_finite() && expected.is_finite());
    assert!((actual - expected).abs() <= 3e-11, "{actual} != {expected}");
}

#[test]
fn inferred_par_principal_and_current_balances_follow_coupon_payments() {
    let (settings, index) = market();
    let days = dates();
    for start in days.iter().take(4) {
        index.add_fixing(*start - 2, 0.037).unwrap();
    }
    let bond = construct(vec![1000., 800., 800., 500.], settings, index).unwrap();
    let flows = bond.bond().cashflows();
    assert_eq!(flows.len(), 7);
    for (position, amount) in [(1, 200.), (4, 300.), (6, 500.)] {
        assert!(flows[position].as_coupon().is_none());
        near(flows[position].amount().unwrap(), amount);
    }
    for ((position, nominal), dates) in [(0, 1000.), (2, 800.), (3, 800.), (5, 500.)]
        .into_iter()
        .zip(days.windows(2))
    {
        let coupon = flows[position].as_coupon().unwrap();
        near(coupon.nominal(), nominal);
        near(
            coupon.amount().unwrap(),
            nominal * 0.038 * f64::from(dates[1] - dates[0]) / 360.0,
        );
    }
    near(bond.bond().notional(Some(days[1] - 1)).unwrap(), 1000.);
    near(bond.bond().notional(Some(days[1])).unwrap(), 800.);
    near(bond.bond().notional(Some(days[3])).unwrap(), 500.);
    near(bond.bond().notional(Some(days[4])).unwrap(), 0.);
}

#[test]
fn exact_positive_finite_nonincreasing_balances_are_required() {
    let (settings, index) = market();
    for balances in [
        vec![],
        vec![1000.],
        vec![1000.; 5],
        vec![1000., 0., 800., 500.],
        vec![1000., -1., 800., 500.],
        vec![1000., 1100., 800., 500.],
        vec![1000., f64::NAN, 800., 500.],
        vec![f64::INFINITY, 800., 800., 500.],
    ] {
        assert!(construct(balances, settings.clone(), index.clone()).is_err());
    }
    assert!(construct(vec![1000.; 4], shared(Settings::new()), index).is_err());
}

#[test]
fn contractual_maturity_remains_unadjusted_and_close_balances_coalesce_natively() {
    let (settings, index) = market();
    let schedule = Schedule::with_metadata(
        vec![
            Date::new(31, Month::January, 2027),
            Date::new(30, Month::April, 2027),
            Date::new(31, Month::July, 2027),
            Date::new(31, Month::October, 2027),
        ],
        WeekendsOnly::new(),
        Bdc::Unadjusted,
        None,
        None,
        None,
        None,
        vec![],
    );
    let bond = FloatingRateBond::new_amortizing(
        0,
        vec![1000., 700., 500.],
        schedule,
        index.clone(),
        Actual360::new(),
        Bdc::Following,
        None,
        vec![],
        None,
        None,
        NullCalendar::new(),
        Bdc::Unadjusted,
        false,
        Bdc::Preceding,
        settings.clone(),
    )
    .unwrap();
    assert_eq!(
        bond.bond().maturity_date().unwrap(),
        Date::new(31, Month::October, 2027)
    );
    assert_eq!(
        bond.bond().redemptions().last().unwrap().date(),
        Date::new(1, Month::November, 2027)
    );
    let close = construct(vec![1000., 1000. - 1e-13, 800., 500.], settings, index).unwrap();
    assert_eq!(close.bond().redemptions().len(), 3);
    assert_eq!(close.bond().redemptions()[0].date(), dates()[2]);
}

#[test]
fn oversized_new_factory_date_lags_return_errors_before_leg_construction() {
    let (settings, index) = market();
    let bound = (Date::max_date() - Date::min_date()) as u32;
    let build = |settlement, fixing, index| {
        FloatingRateBond::new_amortizing(
            settlement,
            vec![1000.; 4],
            Schedule::from_dates(dates()),
            index,
            Actual360::new(),
            Bdc::Following,
            fixing,
            vec![],
            None,
            None,
            NullCalendar::new(),
            Bdc::Unadjusted,
            false,
            Bdc::Preceding,
            settings.clone(),
        )
    };
    for (settlement, fixing) in [
        (bound + 1, None),
        (u32::MAX, None),
        (0, Some(bound + 1)),
        (0, Some(u32::MAX)),
    ] {
        let error = build(settlement, fixing, index.clone())
            .err()
            .expect("oversized lag rejected");
        assert!(error.message().contains("supported date range"));
    }
    let oversized_index = shared(IborIndex::new(
        "OversizedAmortizingIndex".into(),
        Period::new(6, TimeUnit::Months),
        bound + 1,
        Currency::eur(),
        NullCalendar::new(),
        Bdc::Following,
        false,
        Actual360::new(),
        Handle::empty(),
        settings.clone(),
    ));
    assert!(build(0, Some(0), oversized_index).is_err());
}
