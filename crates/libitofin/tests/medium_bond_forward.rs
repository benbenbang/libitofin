use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::FixedRateCoupon;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, BondForward};
use libitofin::interestrate::Compounding;
use libitofin::position::Position;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::calendars::weekendsonly::WeekendsOnly;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn d(y: i32, m: Month, day: i32) -> Date {
    Date::new(day, m, y)
}
fn today() -> Date {
    d(2025, Month::January, 2)
}
fn delivery() -> Date {
    d(2025, Month::July, 2)
}
fn flat(rate: f64) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        today(),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}
fn settings() -> Shared<Settings<Date>> {
    let s = shared(Settings::new());
    s.set_evaluation_date(today());
    s
}
fn bond(
    s: &Shared<Settings<Date>>,
    nominal: &[f64; 4],
    ex: bool,
    curve: Handle<dyn YieldTermStructure>,
) -> SharedMut<Bond> {
    let dates = [
        d(2024, Month::July, 2),
        today(),
        delivery(),
        d(2026, Month::January, 2),
        d(2026, Month::July, 2),
    ];
    let coupons: Leg = nominal
        .iter()
        .enumerate()
        .map(|(i, n)| {
            shared(FixedRateCoupon::from_rate(
                dates[i + 1],
                *n,
                0.05,
                Actual365Fixed::new(),
                dates[i],
                dates[i + 1],
                None,
                None,
                ex.then_some(dates[i + 1] - 7),
            )) as Shared<dyn CashFlow>
        })
        .collect();
    let mut b =
        Bond::from_coupons(2, NullCalendar::new(), Some(dates[0]), coupons, s.clone()).unwrap();
    b.base_mut().set_pricing_engine(
        shared_mut(DiscountingBondEngine::new(curve, None, s.clone()))
            as SharedMut<dyn PricingEngine>,
    );
    shared_mut(b)
}
fn forward(
    b: SharedMut<Bond>,
    value: Date,
    end: Date,
    position: Position,
    fin: Handle<dyn YieldTermStructure>,
    inc: Handle<dyn YieldTermStructure>,
) -> BondForward {
    BondForward::with_income_curve(
        b,
        value,
        end,
        position,
        105.,
        0,
        Actual365Fixed::new(),
        NullCalendar::new(),
        Bdc::Unadjusted,
        fin,
        inc,
    )
    .unwrap()
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2e-11,
        "actual {actual:0.17}, expected {expected:0.17}"
    );
}
fn check(f: &mut BondForward, expected: [f64; 3]) {
    close(f.fair_forward_price().unwrap(), expected[0]);
    close(f.clean_forward_price().unwrap(), expected[1]);
    close(f.npv().unwrap(), expected[2]);
}

#[test]
fn compiled_quantlib_prices_pin_income_dates_signs_and_raw_notional_conventions() {
    let s = settings();
    let cases = [
        (
            [100.; 4],
            false,
            today(),
            delivery() - 1,
            Position::Long,
            [
                104.93770365378091,
                102.47195022912338,
                -0.061079527740859514,
            ],
        ),
        (
            [100.; 4],
            false,
            today(),
            delivery(),
            Position::Long,
            [102.4512404406221, 102.4512404406221, -2.498701450365973],
        ),
        (
            [100.; 4],
            false,
            today(),
            delivery(),
            Position::Short,
            [102.4512404406221, 102.4512404406221, 2.498701450365973],
        ),
        (
            [100.; 4],
            false,
            delivery(),
            delivery() + 1,
            Position::Long,
            [104.96070621915436, 104.94700758901737, -0.03851782236188845],
        ),
        (
            [250.; 4],
            false,
            today(),
            delivery(),
            Position::Long,
            [98.70429464216471, 98.70429464216471, -6.172056540531232],
        ),
        (
            [100., 80., 60., 40.],
            false,
            today(),
            delivery(),
            Position::Long,
            [82.07652678602628, 82.07652678602628, -22.473251961500168],
        ),
        (
            [100.; 4],
            false,
            today(),
            d(2026, Month::July, 2),
            Position::Long,
            [-0.7824599005110053, -0.7824599005110053, -99.63854660262638],
        ),
        (
            [100.; 4],
            true,
            d(2025, Month::June, 27),
            delivery() + 1,
            Position::Long,
            [102.46246858905084, 102.44876995891386, -2.487421215805406],
        ),
    ];
    for (nominals, ex, value, end, position, expected) in cases {
        let b = bond(&s, &nominals, ex, flat(0.03));
        let mut f = forward(b, value, end, position, flat(0.04), flat(0.025));
        check(&mut f, expected);
    }
}

#[test]
fn retained_bond_quotes_relinks_and_settings_invalidate_previously_priced_forwards() {
    let s = settings();
    let spot = shared(SimpleQuote::new(0.03));
    let finance = shared(SimpleQuote::new(0.04));
    let curve = |q: Shared<SimpleQuote>| {
        Handle::new(shared(FlatForward::new(
            today(),
            Handle::new(q as Shared<dyn Quote>),
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let income = RelinkableHandle::new(flat(0.025).current_link().unwrap());
    let b = bond(&s, &[100.; 4], false, curve(spot.clone()));
    let mut f = forward(
        b.clone(),
        today(),
        delivery(),
        Position::Long,
        curve(finance.clone()),
        income.handle(),
    );
    let mut later = forward(
        b,
        today(),
        d(2026, Month::January, 3),
        Position::Long,
        f.discount_curve().clone(),
        income.handle(),
    );
    later.npv().unwrap();
    check(
        &mut f,
        [102.4512404406221, 102.4512404406221, -2.498701450365973],
    );
    spot.set_value(0.06);
    check(
        &mut f,
        [97.97042367275718, 97.97042367275718, -6.89151414840683],
    );
    finance.set_value(0.07);
    check(
        &mut f,
        [99.43879668272126, 99.43879668272126, -5.371473040109597],
    );
    income.link_to(flat(0.08).current_link().unwrap());
    check(
        &mut f,
        [99.50701270919993, 99.50701270919993, -5.3055843239040446],
    );
    s.set_evaluation_date(delivery());
    check(
        &mut later,
        [103.69146889331043, 103.67777026317344, -1.219832354146487],
    );
    assert_eq!(f.npv().unwrap(), 0.);
    assert!(f.fair_forward_price().is_err());
}

#[test]
fn financing_relink_and_underlying_engine_replacement_refresh_live_prices() {
    let s = settings();
    let b = bond(&s, &[100.; 4], false, flat(0.03));
    let financing = RelinkableHandle::new(flat(0.04).current_link().unwrap());
    let mut f = forward(
        b.clone(),
        today(),
        delivery(),
        Position::Long,
        financing.handle(),
        flat(0.025),
    );
    let before = f.fair_forward_price().unwrap();
    financing.link_to(flat(0.07).current_link().unwrap());
    let after = f.fair_forward_price().unwrap();
    assert!(after > before);
    b.borrow_mut()
        .base_mut()
        .set_pricing_engine(
            shared_mut(DiscountingBondEngine::new(flat(0.06), None, s.clone()))
                as SharedMut<dyn PricingEngine>,
        );
    assert!(f.fair_forward_price().unwrap() < after);
}

#[test]
fn convenience_constructor_uses_the_financing_handle_for_income() {
    let s = settings();
    let b = bond(&s, &[100.; 4], false, flat(0.03));
    let fin = flat(0.04);
    let mut f = BondForward::new(
        b.clone(),
        today(),
        delivery(),
        Position::Long,
        105.,
        0,
        Actual365Fixed::new(),
        NullCalendar::new(),
        Bdc::Unadjusted,
        fin.clone(),
    )
    .unwrap();
    let mut explicit = forward(b, today(), delivery(), Position::Long, fin.clone(), fin);
    close(f.npv().unwrap(), explicit.npv().unwrap());
    close(
        f.fair_forward_price().unwrap(),
        explicit.fair_forward_price().unwrap(),
    );
}

#[test]
fn calendar_settlement_delivery_and_expiry_follow_forward_not_bond_dates() {
    let s = settings();
    let b = bond(&s, &[100.; 4], false, flat(0.03));
    let mut f = BondForward::new(
        b.clone(),
        today(),
        d(2025, Month::January, 4),
        Position::Long,
        105.,
        2,
        Actual365Fixed::new(),
        WeekendsOnly::new(),
        Bdc::Following,
        flat(0.04),
    )
    .unwrap();
    assert_eq!(f.delivery_date(), d(2025, Month::January, 6));
    assert_eq!(f.settlement_date().unwrap(), d(2025, Month::January, 6));
    assert!(f.is_expired().unwrap());
    assert_eq!(f.npv().unwrap(), 0.);
    assert!(f.clean_forward_price().is_err());
    s.set_include_reference_date_events(true);
    f.recalculate().unwrap();
    assert!(!f.is_expired().unwrap());
    assert!(f.fair_forward_price().is_ok());
    s.set_evaluation_date(d(2025, Month::January, 4));
    let f0 = BondForward::new(
        b,
        today(),
        delivery(),
        Position::Long,
        105.,
        0,
        Actual365Fixed::new(),
        WeekendsOnly::new(),
        Bdc::Unadjusted,
        flat(0.04),
    )
    .unwrap();
    assert_eq!(f0.settlement_date().unwrap(), d(2025, Month::January, 6));
}

#[test]
fn checked_construction_and_pricing_reject_malformed_inputs_and_borrow_conflicts() {
    let s = settings();
    let b = bond(&s, &[100.; 4], false, flat(0.03));
    for strike in [f64::NAN, f64::INFINITY, -1.] {
        assert!(
            BondForward::new(
                b.clone(),
                today(),
                delivery(),
                Position::Long,
                strike,
                0,
                Actual365Fixed::new(),
                NullCalendar::new(),
                Bdc::Unadjusted,
                flat(0.04)
            )
            .is_err()
        );
    }
    for (value, end) in [
        (Date::null(), delivery()),
        (today(), Date::null()),
        (delivery(), today()),
    ] {
        assert!(
            BondForward::new(
                b.clone(),
                value,
                end,
                Position::Long,
                105.,
                0,
                Actual365Fixed::new(),
                NullCalendar::new(),
                Bdc::Unadjusted,
                flat(0.04)
            )
            .is_err()
        );
    }
    let guard = b.borrow_mut();
    assert!(
        BondForward::new(
            b.clone(),
            today(),
            delivery(),
            Position::Long,
            105.,
            0,
            Actual365Fixed::new(),
            NullCalendar::new(),
            Bdc::Unadjusted,
            flat(0.04)
        )
        .is_err()
    );
    drop(guard);
    let mut f = forward(
        b.clone(),
        today(),
        delivery(),
        Position::Long,
        flat(0.04),
        flat(0.025),
    );
    let guard = b.borrow();
    assert!(f.npv().is_err());
    drop(guard);
    assert!(f.npv().is_ok());
    let mut empty = forward(
        b.clone(),
        today(),
        delivery(),
        Position::Long,
        Handle::empty(),
        Handle::empty(),
    );
    assert!(empty.npv().is_err());
    let mut underflow = forward(
        b,
        today(),
        delivery(),
        Position::Long,
        flat(1e6),
        flat(0.025),
    );
    assert!(underflow.npv().is_err());
    s.reset_evaluation_date();
    assert!(f.npv().is_err());
}

#[test]
fn null_evaluation_and_date_range_overflow_are_checked_before_calendar_arithmetic() {
    let s = settings();
    let b = bond(&s, &[100.; 4], false, flat(0.03));
    let f = BondForward::new(
        b.clone(),
        today(),
        Date::max_date(),
        Position::Long,
        105.,
        u32::MAX,
        Actual365Fixed::new(),
        NullCalendar::new(),
        Bdc::Unadjusted,
        flat(0.04),
    )
    .unwrap();
    assert!(f.settlement_date().is_err());
    s.set_evaluation_date(Date::null());
    assert!(f.settlement_date().is_err());
    s.set_evaluation_date(Date::max_date());
    assert!(f.settlement_date().is_err());
    let boundary = BondForward::new(
        b,
        today(),
        Date::max_date(),
        Position::Long,
        105.,
        0,
        Actual365Fixed::new(),
        WeekendsOnly::new(),
        Bdc::Following,
        flat(0.04),
    );
    if Date::max_date().weekday() == libitofin::time::weekday::Weekday::Sunday {
        assert!(boundary.is_err());
    }
}

#[test]
fn compiled_ex_coupon_and_today_override_semantics_are_not_silently_corrected() {
    let s = settings();
    s.set_evaluation_date(d(2025, Month::June, 27));
    let b = bond(&s, &[100.; 4], true, flat(0.03));
    let mut f = forward(
        b,
        today(),
        delivery() + 1,
        Position::Long,
        flat(0.04),
        flat(0.025),
    );
    check(
        &mut f,
        [101.46306126895901, 101.44936263882202, -3.467092624206924],
    );
    s.set_evaluation_date(delivery());
    let b = bond(&s, &[100.; 4], false, flat(0.03));
    let mut f = forward(
        b,
        today(),
        d(2026, Month::January, 3),
        Position::Long,
        flat(0.04),
        flat(0.025),
    );
    s.set_include_todays_cash_flows(Some(true));
    check(
        &mut f,
        [101.01444972395771, 101.00075109382072, -3.8288549908924607],
    );
    s.set_include_todays_cash_flows(Some(false));
    f.recalculate().unwrap();
    check(
        &mut f,
        [103.56357410159957, 103.54987547146258, -1.3799515974489525],
    );
}
