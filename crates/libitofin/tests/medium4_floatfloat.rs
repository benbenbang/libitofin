//! Whole-instrument QuantLib 1.43 oracle and plain-slice regression contracts.
#[path = "fixtures/medium4_floatfloat/support.rs"]
mod support;
use libitofin::currency::Currency;
use libitofin::handle::Handle;
use libitofin::indexes::{IborIndex, Index};
use libitofin::instrument::Instrument;
use libitofin::instruments::FloatFloatSwap;
use libitofin::interestrate::Compounding;
use libitofin::quotes::Quote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use libitofin::time::{
    date::Date, frequency::Frequency, period::Period, schedule::Schedule, timeunit::TimeUnit,
};
use support::*;
#[test]
fn whole_native_swap_fixture_matches_signed_measures_and_fair_spreads() {
    for (name, row) in rows() {
        let market = Market::new(&row);
        let mut swap = market.standard(&row);
        for (actual, expected) in [
            swap.npv().unwrap(),
            swap.pay_leg_npv().unwrap(),
            swap.receive_leg_npv().unwrap(),
            swap.pay_leg_bps().unwrap(),
            swap.receive_leg_bps().unwrap(),
        ]
        .into_iter()
        .zip(&row[11..16])
        {
            close(actual, *expected, 2e-9);
        }
        assert_eq!(swap.swap().leg(0).unwrap().len(), 4);
        assert_eq!(swap.swap().leg(1).unwrap().len(), 2);
        if name == "expired" {
            assert!(swap.fair_pay_spread().is_err());
            assert!(swap.fair_receive_spread().is_err());
        } else {
            close(swap.fair_pay_spread().unwrap(), row[16], 2e-13);
            close(swap.fair_receive_spread().unwrap(), row[17], 2e-13);
            let mut fair_pay = market
                .build(&row, 1_000_000.0, row[16], row[9], schedule(false))
                .unwrap();
            let mut fair_receive = market
                .build(&row, 1_000_000.0, row[8], row[17], schedule(false))
                .unwrap();
            close(fair_pay.npv().unwrap(), 0.0, 2e-9);
            close(fair_receive.npv().unwrap(), 0.0, 2e-9);
        }
    }
}
#[test]
fn signed_bps_uses_each_coupon_accrual_convention() {
    let row = &rows()[0].1;
    let market = Market::new(row);
    let mut swap = market.standard(row);
    let npv = swap.npv().unwrap();
    let mut paid_shift = market
        .build(row, 1_000_000.0, row[8] + 1e-4, row[9], schedule(false))
        .unwrap();
    let mut received_shift = market
        .build(row, 1_000_000.0, row[8], row[9] + 1e-4, schedule(false))
        .unwrap();
    close(
        paid_shift.npv().unwrap() - npv,
        swap.pay_leg_bps().unwrap(),
        2e-9,
    );
    close(
        received_shift.npv().unwrap() - npv,
        swap.receive_leg_bps().unwrap(),
        2e-9,
    );
}
#[test]
fn forwarding_discount_quotes_and_relinks_reprice_an_existing_swap() {
    let row = &rows()[0].1;
    let market = Market::new(row);
    let mut swap = market.standard(row);
    let mut last = swap.npv().unwrap();
    for quote in &market.quotes {
        quote.set_value(quote.value().unwrap() + 0.003);
        let next = swap.npv().unwrap();
        assert!((next - last).abs() > 1.0);
        last = next;
    }
    for curve in &market.curves {
        curve.link_to(shared(FlatForward::with_rate(
            reference(),
            0.08,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let next = swap.npv().unwrap();
        assert!((next - last).abs() > 1.0);
        last = next;
    }
}
#[test]
fn non_notifying_today_setting_is_reread_without_manual_recalculate() {
    let (_, row) = rows()
        .into_iter()
        .find(|(name, _)| *name == "today_in")
        .unwrap();
    let market = Market::new(&row);
    let mut swap = market.standard(&row);
    let included = swap.npv().unwrap();
    market.settings.set_include_todays_cash_flows(Some(false));
    let excluded = swap.npv().unwrap();
    assert!((included - excluded).abs() > 1.0);
    let (_, expected) = rows()
        .into_iter()
        .find(|(name, _)| *name == "today_out")
        .unwrap();
    close(excluded, expected[11], 2e-9);
}
#[test]
fn missing_fixings_and_empty_discount_handle_fail_then_recover() {
    let (_, row) = rows()
        .into_iter()
        .find(|(name, _)| *name == "seasoned")
        .unwrap();
    let market = Market::new(&row);
    let mut swap = market.standard(&row);
    market.settings.clear_fixing(&market.indexes[0].name());
    assert!(swap.npv().is_err());
    for fixing_date in dates(false)
        .into_iter()
        .filter(|fixing_date| *fixing_date < date(row[0]))
    {
        market.indexes[0].add_fixing(fixing_date, 0.021).unwrap();
    }
    close(swap.npv().unwrap(), row[11], 2e-9);
    market.curves[2].reset();
    assert!(swap.npv().is_err());
}
#[test]
fn malformed_numeric_contract_and_schedules_return_errors() {
    let row = &rows()[0].1;
    let market = Market::new(row);
    for nominal in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            market
                .build(row, nominal, row[8], row[9], schedule(false))
                .is_err()
        );
    }
    for spread in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            market
                .build(row, 1.0, spread, 0.0, schedule(false))
                .is_err()
        );
        assert!(
            market
                .build(row, 1.0, 0.0, spread, schedule(false))
                .is_err()
        );
    }
    for first in [
        Schedule::from_dates(vec![]),
        Schedule::from_dates(vec![reference()]),
        Schedule::from_dates(vec![reference(), reference()]),
        Schedule::from_dates(vec![reference() + 1, reference()]),
        Schedule::from_dates(vec![Date::null(), reference()]),
    ] {
        assert!(market.build(row, 1.0, 0.0, 0.0, first).is_err());
    }
}
#[test]
fn currency_settings_and_overnight_contracts_are_rejected() {
    let row = &rows()[0].1;
    let market = Market::new(row);
    let make_index = |currency, settings, tenor| {
        shared(IborIndex::new(
            "Mismatch".into(),
            tenor,
            0,
            currency,
            NullCalendar::new(),
            BusinessDayConvention::Unadjusted,
            false,
            Actual360::new(),
            Handle::empty(),
            settings,
        ))
    };
    for second in [
        make_index(
            Currency::usd(),
            market.settings.clone(),
            Period::new(6, TimeUnit::Months),
        ),
        make_index(
            Currency::eur(),
            shared(Settings::new()),
            Period::new(6, TimeUnit::Months),
        ),
        make_index(
            Currency::eur(),
            market.settings.clone(),
            Period::new(1, TimeUnit::Days),
        ),
    ] {
        assert!(
            FloatFloatSwap::new(
                1.0,
                schedule(false),
                market.indexes[0].clone(),
                0.0,
                Actual360::new(),
                schedule(true),
                second,
                0.0,
                Actual365Fixed::new(),
                None,
                market.settings.clone()
            )
            .is_err()
        );
    }
}
#[test]
fn evaluation_roll_expiry_and_unset_date_are_not_stale() {
    let row = &rows()[0].1;
    let market = Market::new(row);
    let mut swap = market.standard(row);
    assert!(swap.npv().unwrap().abs() > 1.0);
    market
        .settings
        .set_evaluation_date(Date::new(18, libitofin::time::date::Month::January, 2027));
    assert_eq!(swap.npv().unwrap(), 0.0);
    assert!(swap.fair_receive_spread().is_err());
    market.settings.reset_evaluation_date();
    assert!(swap.npv().is_err());
}
#[test]
fn frozen_queries_pin_results_and_reads_do_not_notify_observers() {
    use libitofin::patterns::observable::Observer;
    use libitofin::shared::{SharedMut, shared_mut};
    struct Counter(usize);
    impl Observer for Counter {
        fn update(&mut self) {
            self.0 += 1;
        }
    }
    let (_, row) = rows()
        .into_iter()
        .find(|(name, _)| *name == "today_in")
        .unwrap();
    let market = Market::new(&row);
    let mut swap = market.standard(&row);
    let counter = shared_mut(Counter(0));
    swap.base()
        .register_observer(&(counter.clone() as SharedMut<dyn Observer>));
    let initial = swap.npv().unwrap();
    swap.npv().unwrap();
    swap.pay_leg_bps().unwrap();
    assert_eq!(counter.borrow().0, 0);
    swap.base_mut().freeze();
    market.settings.set_include_todays_cash_flows(Some(false));
    market.quotes[0].set_value(0.08);
    assert_eq!(swap.npv().unwrap(), initial);
    assert_eq!(counter.borrow().0, 0);
    swap.base_mut().unfreeze();
    assert_eq!(counter.borrow().0, 1);
    assert!((swap.npv().unwrap() - initial).abs() > 1.0);
    assert_eq!(counter.borrow().0, 1);
}
