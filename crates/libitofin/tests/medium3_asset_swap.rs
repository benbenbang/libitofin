use libitofin::cashflows::FixedRateLeg;
use libitofin::handle::Handle;
use libitofin::indexes::ibor::Euribor;
use libitofin::indexes::{IborIndex, Index};
use libitofin::instrument::Instrument;
use libitofin::instruments::{AssetSwap, Bond};
use libitofin::interestrate::Compounding;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::swap::DiscountingSwapEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::schedule::{MakeSchedule, Schedule};

fn today() -> Date {
    Date::new(2, Month::January, 2025)
}
fn maturity() -> Date {
    Date::new(2, Month::January, 2029)
}
fn settings() -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today());
    settings
}
fn schedule(start: Date, end: Date, frequency: Frequency) -> Schedule {
    MakeSchedule::new()
        .from(start)
        .to(end)
        .with_frequency(frequency)
        .with_calendar(Target::new())
        .with_convention(Bdc::ModifiedFollowing)
        .backwards()
        .build()
}
fn bond(s: &Shared<Settings<Date>>, redemption: f64) -> Shared<Bond> {
    let coupons = FixedRateLeg::new(schedule(
        Date::new(2, Month::January, 2024),
        maturity(),
        Frequency::Annual,
    ))
    .with_notional(1000.0)
    .with_coupon_rate(
        0.05,
        Actual365Fixed::new(),
        Compounding::Simple,
        Frequency::Annual,
    )
    .unwrap()
    .with_payment_adjustment(Bdc::Following)
    .build()
    .unwrap();
    let mut bond = Bond::new(2, Target::new(), None, coupons, s.clone()).unwrap();
    bond.add_redemptions_to_cashflows(&[redemption]).unwrap();
    shared(bond)
}
fn curve(quote: &Shared<SimpleQuote>) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::new(
        today(),
        Handle::new(quote.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}
fn index(curve: Handle<dyn YieldTermStructure>, s: &Shared<Settings<Date>>) -> Shared<IborIndex> {
    let index = shared(Euribor::six_months(curve, s.clone()));
    s.add_fixing(&index.name(), Date::new(30, Month::December, 2024), 0.031)
        .unwrap();
    index
}
fn attach(
    swap: &mut AssetSwap,
    discount: Handle<dyn YieldTermStructure>,
    s: &Shared<Settings<Date>>,
    include: bool,
) {
    swap.base_mut()
        .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
            discount,
            Some(include),
            None,
            None,
            s.clone(),
        )) as SharedMut<dyn PricingEngine>);
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 3e-10,
        "actual {actual:.17}, expected {expected:.17}"
    );
}
fn rows() -> Vec<Vec<String>> {
    include_str!("fixtures/medium3_asset_swap/prices.csv")
        .lines()
        .skip(1)
        .map(|line| line.split(',').map(str::to_owned).collect())
        .collect()
}
fn value(row: &[String], i: usize) -> f64 {
    row[i].parse().unwrap()
}
fn build(row: &[String], s: &Shared<Settings<Date>>) -> AssetSwap {
    s.set_include_todays_cash_flows((row[0] == "coupon_today_override").then_some(true));
    let start = value(row, 6) as i32;
    AssetSwap::new(
        value(row, 1) != 0.0,
        bond(s, value(row, 7)),
        value(row, 2),
        index(curve(&shared(SimpleQuote::new(value(row, 4)))), s),
        value(row, 3),
        (start != 0).then(|| schedule(Date::from_serial(start), maturity(), Frequency::Semiannual)),
        Some(Actual360::new()),
    )
    .unwrap()
}

#[test]
fn native_quantlib_par_prices_dirty_upfront_cutoff_redemption_and_signs() {
    for row in rows() {
        let s = settings();
        let mut swap = build(&row, &s);
        assert_eq!(swap.bond_leg().len(), value(&row, 9) as usize, "{}", row[0]);
        assert_eq!(
            swap.floating_leg().len(),
            value(&row, 10) as usize,
            "{}",
            row[0]
        );
        close(
            swap.bond()
                .accrued_amount(Some(swap.upfront_date()))
                .unwrap(),
            value(&row, 11),
        );
        close(swap.floating_leg()[0].amount().unwrap(), value(&row, 12));
        attach(
            &mut swap,
            curve(&shared(SimpleQuote::new(value(&row, 5)))),
            &s,
            value(&row, 8) != 0.0,
        );
        close(swap.npv().unwrap(), value(&row, 13));
        close(swap.bond_leg_npv().unwrap(), value(&row, 14));
        close(swap.swap_mut().leg_npv(0).unwrap(), value(&row, 14));
        close(swap.floating_leg_npv().unwrap(), value(&row, 15));
        close(swap.floating_leg_bps().unwrap(), value(&row, 16));
        close(swap.fair_spread().unwrap(), value(&row, 17));
        close(swap.fair_clean_price().unwrap(), value(&row, 18));
        assert!(swap.par_swap());
        assert_eq!(swap.swap().payer(0).unwrap(), swap.pay_bond_coupon());
    }
}

#[test]
fn native_leg_dates_and_amounts_pin_generated_schedule_and_full_redemption() {
    let cases = rows();
    for name in ["default", "coupon_cutoff", "nonpar_redemption"] {
        let s = settings();
        let swap = build(cases.iter().find(|row| row[0] == name).unwrap(), &s);
        let expected: Vec<_> = include_str!("fixtures/medium3_asset_swap/flows.csv")
            .lines()
            .skip(1)
            .map(|line| line.split(',').collect::<Vec<_>>())
            .filter(|row| row[0] == name)
            .collect();
        for leg in 0..2 {
            let rows: Vec<_> = expected
                .iter()
                .filter(|row| row[1] == leg.to_string())
                .collect();
            for (flow, row) in swap.swap().leg(leg).unwrap().iter().zip(&rows) {
                assert_eq!(flow.date().serial_number(), row[2].parse::<i32>().unwrap());
                close(flow.amount().unwrap(), row[3].parse().unwrap());
            }
            assert_eq!(swap.swap().leg(leg).unwrap().len(), rows.len());
        }
    }
}

#[test]
fn fair_spread_reprices_and_bps_matches_decimal_spread_bump() {
    let row = rows().remove(0);
    let s = settings();
    let mut original = build(&row, &s);
    attach(
        &mut original,
        curve(&shared(SimpleQuote::new(0.02))),
        &s,
        true,
    );
    let npv = original.npv().unwrap();
    let bps = original.floating_leg_bps().unwrap();
    let fair = original.fair_spread().unwrap();
    let fair_price = original.fair_clean_price().unwrap();
    for (spread, price, expected) in [
        (0.0011, 102.5, npv + bps),
        (fair, 102.5, 0.0),
        (0.001, fair_price, 0.0),
    ] {
        let mut input = row.clone();
        input[2] = price.to_string();
        input[3] = spread.to_string();
        let mut swap = build(&input, &s);
        attach(&mut swap, curve(&shared(SimpleQuote::new(0.02))), &s, true);
        close(swap.npv().unwrap(), expected);
    }
}

#[test]
fn live_forward_discount_engine_replacement_and_expiry_invalidate_cache() {
    let s = settings();
    let forward = shared(SimpleQuote::new(0.03));
    let discount = shared(SimpleQuote::new(0.02));
    let mut swap = AssetSwap::new(
        true,
        bond(&s, 100.0),
        102.5,
        index(curve(&forward), &s),
        0.001,
        None,
        None,
    )
    .unwrap();
    attach(&mut swap, curve(&discount), &s, true);
    close(swap.npv().unwrap(), value(&rows()[0], 13));
    forward.set_value(0.035);
    close(swap.npv().unwrap(), value(&rows()[6], 13));
    forward.set_value(0.03);
    discount.set_value(0.04);
    close(swap.npv().unwrap(), value(&rows()[7], 13));
    attach(&mut swap, curve(&shared(SimpleQuote::new(0.02))), &s, true);
    close(swap.npv().unwrap(), value(&rows()[0], 13));
    discount.set_value(0.06);
    close(swap.npv().unwrap(), value(&rows()[0], 13));
    s.set_evaluation_date(maturity() + 1);
    assert!(swap.is_expired().unwrap());
    assert_eq!(swap.npv().unwrap(), 0.0);
    assert_eq!(swap.floating_leg_npv().unwrap(), 0.0);
    assert_eq!(swap.bond_leg_npv().unwrap(), 0.0);
    assert!(swap.fair_spread().is_err());
    assert!(swap.fair_clean_price().is_err());
    s.set_evaluation_date(today());
    close(swap.npv().unwrap(), value(&rows()[0], 13));
}

#[test]
fn invalid_inputs_and_absent_results_fail_without_changing_existing_apis() {
    let s = settings();
    let b = bond(&s, 100.0);
    let i = index(curve(&shared(SimpleQuote::new(0.03))), &s);
    for (price, spread) in [(f64::NAN, 0.0), (100.0, f64::INFINITY)] {
        assert!(AssetSwap::new(true, b.clone(), price, i.clone(), spread, None, None).is_err());
    }
    let partial = schedule(
        today(),
        Date::new(2, Month::January, 2028),
        Frequency::Semiannual,
    );
    assert!(AssetSwap::new(true, b.clone(), 100.0, i.clone(), 0.0, Some(partial), None).is_err());
    assert!(
        AssetSwap::new(
            true,
            b.clone(),
            100.0,
            i.clone(),
            0.0,
            Some(Schedule::from_dates(vec![])),
            None
        )
        .is_err()
    );
    let other = settings();
    assert!(AssetSwap::new(true, bond(&other, 100.0), 100.0, i.clone(), 0.0, None, None).is_err());
    let mut swap = AssetSwap::new(true, b, 100.0, i, 0.0, None, None).unwrap();
    assert!(swap.npv().is_err());
    assert!(swap.bond_leg_npv().is_err());
    assert!(swap.fair_spread().is_err());
    assert!(swap.fair_clean_price().is_err());
}

#[test]
fn seasoned_clean_price_and_unsupported_bonds_schedules_and_indexes_fail() {
    let s = settings();
    let b = bond(&s, 100.0);
    let i = index(curve(&shared(SimpleQuote::new(0.03))), &s);
    let mut swap = AssetSwap::new(true, b.clone(), 102.5, i.clone(), 0.001, None, None).unwrap();
    let discount = Handle::new(shared(FlatForward::with_rate(
        today() + 5,
        0.02,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    attach(&mut swap, discount, &s, true);
    assert!(swap.fair_spread().unwrap().is_finite());
    assert!(swap.fair_clean_price().is_err());
    let invalid = Schedule::from_dates(vec![today(), today(), maturity()]);
    assert!(AssetSwap::new(true, b.clone(), 100.0, i.clone(), 0.0, Some(invalid), None).is_err());
    let coupons = FixedRateLeg::new(schedule(today(), maturity(), Frequency::Annual))
        .with_notionals(vec![1000.0, 800.0, 600.0, 400.0])
        .with_coupon_rate(
            0.05,
            Actual365Fixed::new(),
            Compounding::Simple,
            Frequency::Annual,
        )
        .unwrap()
        .build()
        .unwrap();
    let amortizing =
        shared(Bond::from_coupons(2, Target::new(), None, coupons, s.clone()).unwrap());
    assert!(AssetSwap::new(true, amortizing, 100.0, i.clone(), 0.0, None, None).is_err());
    let overnight =
        libitofin::indexes::ibor::Eonia::new(curve(&shared(SimpleQuote::new(0.03))), s.clone());
    assert!(
        AssetSwap::new(
            true,
            b.clone(),
            100.0,
            overnight.ibor_index(),
            0.0,
            None,
            None
        )
        .is_err()
    );
    s.set_evaluation_date(maturity() + 1);
    assert!(AssetSwap::new(true, b, 100.0, i, 0.0, None, None).is_err());
}
