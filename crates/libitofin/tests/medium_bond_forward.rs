use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::FixedRateCoupon;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{Bond, BondForward};
use libitofin::interestrate::Compounding;
use libitofin::position::Position;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::bond::DiscountingBondEngine;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention as Bdc;
use libitofin::time::calendars::nullcalendar::NullCalendar;
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
