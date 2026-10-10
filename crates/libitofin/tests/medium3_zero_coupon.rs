use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{BondPrice, ZeroCouponBond};
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::{nullcalendar::NullCalendar, weekendsonly::WeekendsOnly};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn reference() -> Date {
    Date::new(1, Month::October, 2026)
}

fn settings(offset: i32) -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference() + offset);
    settings
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2e-12,
        "actual={actual:.17}, expected={expected:.17}"
    );
}

fn bond(settings: Shared<Settings<Date>>, lag: u32, calendar: Calendar) -> ZeroCouponBond {
    ZeroCouponBond::new(
        lag,
        calendar,
        100.0,
        reference() + 30,
        Bdc::Following,
        100.0,
        None,
        settings,
    )
    .unwrap()
}

fn attach_curve(bond: &mut ZeroCouponBond, rate: f64, comp: Compounding) {
    let curve = shared(FlatForward::with_rate(
        reference(),
        rate,
        Actual365Fixed::new(),
        comp,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>;
    let engine = shared_mut(DiscountingBondEngine::new(
        Handle::new(curve),
        None,
        bond.bond().settings_handle(),
    ));
    bond.bond_mut().base_mut().set_pricing_engine(engine);
}

#[test]
fn compiled_native_vectors_cover_redemption_settlement_and_pricing() {
    for row in include_str!("fixtures/medium3_zero_coupon/native.csv")
        .lines()
        .skip(1)
    {
        let v: Vec<f64> = row.split(',').map(|x| x.parse().unwrap()).collect();
        assert_eq!(v.len(), 18);
        let comp = [
            Compounding::Continuous,
            Compounding::Compounded,
            Compounding::Simple,
        ][v[7] as usize];
        let mut zero = ZeroCouponBond::new(
            v[0] as u32,
            WeekendsOnly::new(),
            v[2],
            reference() + v[4] as i32,
            [
                Bdc::Following,
                Bdc::ModifiedFollowing,
                Bdc::Preceding,
                Bdc::Unadjusted,
            ][v[1] as usize],
            v[3],
            (v[6] >= 0.0).then(|| reference() + v[6] as i32),
            settings(v[5] as i32),
        )
        .unwrap();
        attach_curve(&mut zero, v[8], comp);
        let base = zero.bond_mut();
        assert_eq!(base.maturity_date().unwrap(), reference() + v[4] as i32);
        assert_eq!(base.cashflows().len(), 1);
        assert_eq!(base.redemptions().len(), 1);
        assert!(base.cashflows()[0].as_coupon().is_none());
        assert_eq!(base.cashflows()[0].date(), reference() + v[9] as i32);
        close(base.cashflows()[0].amount().unwrap(), v[2] * v[3] / 100.0);
        assert_eq!(
            base.settlement_date(None).unwrap(),
            reference() + v[10] as i32
        );
        assert_eq!(base.notionals(), &[v[2], 0.0]);
        close(base.notional(None).unwrap(), v[11]);
        close(base.npv().unwrap(), v[12]);
        close(base.settlement_value().unwrap(), v[13]);
        close(base.clean_price().unwrap(), v[14]);
        close(base.dirty_price().unwrap(), v[15]);
        close(base.accrued_amount(None).unwrap(), v[16]);
        assert_eq!(base.is_expired().unwrap(), v[17] != 0.0);
    }
}

#[test]
fn evaluation_date_and_quote_updates_invalidate_cached_prices() {
    let settings = settings(27);
    let mut zero = bond(Shared::clone(&settings), 0, WeekendsOnly::new());
    let quote = shared(SimpleQuote::new(Some(0.04)));
    let curve = shared(FlatForward::new(
        reference(),
        Handle::new(Shared::clone(&quote) as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>;
    zero.bond_mut()
        .base_mut()
        .set_pricing_engine(shared_mut(DiscountingBondEngine::new(
            Handle::new(curve),
            None,
            Shared::clone(&settings),
        )));
    close(
        zero.bond_mut().clean_price().unwrap(),
        100.0 * (-0.04_f64 * 5.0 / 365.0).exp(),
    );
    quote.set_value(0.08);
    close(
        zero.bond_mut().clean_price().unwrap(),
        100.0 * (-0.08_f64 * 5.0 / 365.0).exp(),
    );
    settings.set_evaluation_date(reference() + 32);
    assert!(zero.bond().is_expired().unwrap());
    close(zero.bond_mut().npv().unwrap(), 0.0);
    close(zero.bond_mut().clean_price().unwrap(), 0.0);
    settings.set_evaluation_date(reference() + 27);
    close(
        zero.bond_mut().clean_price().unwrap(),
        100.0 * (-0.08_f64 * 5.0 / 365.0).exp(),
    );
}

#[test]
fn expiry_inclusion_is_distinct_from_redemption_date_notional() {
    let settings = settings(32);
    let mut zero = bond(Shared::clone(&settings), 0, WeekendsOnly::new());
    attach_curve(&mut zero, 0.04, Compounding::Continuous);
    assert!(zero.bond().is_expired().unwrap());
    settings.set_include_reference_date_events(true);
    assert!(!zero.bond().is_expired().unwrap());
    close(zero.bond().notional(None).unwrap(), 0.0);
    close(
        zero.bond_mut().npv().unwrap(),
        100.0 * (-0.04_f64 * 32.0 / 365.0).exp(),
    );
    close(zero.bond_mut().settlement_value().unwrap(), 0.0);
    close(zero.bond_mut().dirty_price().unwrap(), 0.0);
    settings.set_include_todays_cash_flows(Some(false));
    assert!(zero.bond().is_expired().unwrap());
}

#[test]
fn price_yield_round_trip_and_ownership_use_the_existing_bond_contract() {
    let mut zero = bond(settings(27), 0, WeekendsOnly::new());
    attach_curve(&mut zero, 0.04, Compounding::Continuous);
    let price = zero.bond_mut().clean_price().unwrap();
    let base = zero.into_bond();
    let yield_rate = base
        .yield_rate(
            BondPrice::Clean(price),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
            None,
            Some(1e-12),
            Some(100),
            Some(0.05),
        )
        .unwrap();
    assert!((yield_rate - 0.04).abs() < 1e-10);
    close(base.notional(Some(reference() + 31)).unwrap(), 100.0);
    close(base.notional(Some(reference() + 32)).unwrap(), 0.0);
}

#[test]
fn unset_settings_engine_and_invalid_cashflows_return_errors() {
    let mut zero = bond(shared(Settings::new()), 0, NullCalendar::new());
    assert!(zero.bond().settlement_date(None).is_err());
    assert_eq!(
        zero.bond().settlement_date(Some(reference())).unwrap(),
        reference()
    );
    zero.bond().settings().set_evaluation_date(reference());
    assert!(zero.bond_mut().npv().is_err());
    for (face, redemption) in [(f64::NAN, 100.0), (100.0, f64::INFINITY), (f64::MAX, 100.0)] {
        assert!(
            ZeroCouponBond::new(
                0,
                NullCalendar::new(),
                face,
                reference(),
                Bdc::Unadjusted,
                redemption,
                None,
                settings(0),
            )
            .is_err()
        );
    }
    assert!(
        ZeroCouponBond::new(
            0,
            NullCalendar::new(),
            100.0,
            Date::null(),
            Bdc::Unadjusted,
            100.0,
            None,
            settings(0),
        )
        .is_err()
    );
}
