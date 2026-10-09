use libitofin::currency::Currency;
use libitofin::errors::QlResult;
use libitofin::exchangerate::ExchangeRate;
use libitofin::fxforward::FxForward;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::patterns::observable::{AsObservable, Observable};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::termstructures::{TermStructure, TermStructureBase};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn today() -> Date {
    Date::new(15, Month::June, 2026)
}

fn flat(reference: Date, rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        reference,
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn spot(value: f64) -> ExchangeRate {
    ExchangeRate::new(Currency::eur(), Currency::usd(), value)
}

fn forward(settlement: Date, delivery: Date, long: bool) -> FxForward {
    FxForward::new(
        spot(1.1),
        Handle::new(flat(today(), 0.02)),
        Handle::new(flat(today(), 0.05)),
        1_000_000.0,
        1.15,
        settlement,
        delivery,
        long,
    )
    .unwrap()
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn compiled_quantlib_forward_vectors_follow_native_engine_algebra() {
    for &(
        base_ref_days,
        quote_ref_days,
        settlement_days,
        delivery_days,
        base_rate,
        quote_rate,
        s,
        k,
        n,
        fair,
        value,
    ) in &[
        (
            0,
            0,
            0,
            365,
            0.02,
            0.05,
            1.1,
            1.15,
            1000000.0,
            1.1334999873488687,
            -15695.297538389907,
        ),
        (
            0,
            0,
            2,
            365,
            0.02,
            0.05,
            1.1,
            1.15,
            1000000.0,
            1.133313673897679,
            -15872.524375342062,
        ),
        (
            -10,
            -20,
            2,
            365,
            0.02,
            0.05,
            1.1,
            1.15,
            1000000.0,
            1.133313673897679,
            -15829.097523192979,
        ),
        (
            -20,
            -10,
            10,
            400,
            -0.03,
            0.01,
            0.8,
            0.9,
            250000.0,
            0.8349329762752038,
            -16085.05588495773,
        ),
        (
            0,
            0,
            0,
            0,
            0.02,
            0.05,
            1.1,
            1.15,
            1000000.0,
            1.1,
            -49999.999999999956,
        ),
        (
            -10,
            0,
            20,
            20,
            0.04,
            -0.02,
            1.3,
            1.25,
            100.0,
            1.3,
            5.005482455591369,
        ),
        (
            0,
            0,
            2,
            30,
            -0.02,
            -0.04,
            150.0,
            148.0,
            10000.0,
            149.77003946688774,
            17758.68351201454,
        ),
        (
            0,
            0,
            2,
            730,
            0.1,
            0.2,
            0.006,
            0.007,
            1000.0,
            0.007324402078204705,
            0.2174532159962341,
        ),
        (
            -365,
            -100,
            0,
            366,
            0.05,
            0.05,
            1.2,
            1.2,
            500.0,
            1.2000000000000004,
            1.3456812245700626e-13,
        ),
        (
            1,
            2,
            3,
            365,
            0.03,
            0.07,
            1.1,
            1.2,
            1000000.0,
            1.14451551067875,
            -51753.24169576007,
        ),
        (
            0,
            0,
            0,
            365,
            -0.05,
            0.07,
            1.0,
            1.0,
            1e-100,
            1.1274968515793757,
            1.188772764700759e-101,
        ),
        (
            0,
            0,
            0,
            365,
            0.05,
            -0.07,
            1.0,
            1.0,
            1e+100,
            0.8869204367171576,
            -1.2127875675350255e+99,
        ),
    ] {
        let contract = FxForward::new(
            spot(s),
            Handle::new(flat(today() + base_ref_days, base_rate)),
            Handle::new(flat(today() + quote_ref_days, quote_rate)),
            n,
            k,
            today() + settlement_days,
            today() + delivery_days,
            true,
        )
        .unwrap();
        close(contract.fair_forward_rate().unwrap(), fair, 2e-15);
        let npv = contract.npv(today()).unwrap();
        assert_eq!(npv.currency(), &Currency::usd());
        let value: f64 = value;
        let tolerance = if n < 1e-50 {
            value.abs() * 2e-15
        } else {
            1e-9_f64.max(value.abs() * 2e-15)
        };
        close(npv.value(), value, tolerance);
    }
}

#[test]
fn direction_fair_strike_and_explicit_settlement_are_consistent() {
    let delivery = today() + 365;
    let settlement = today() + 2;
    let long = forward(settlement, delivery, true);
    let short = forward(settlement, delivery, false);
    close(
        long.npv(today()).unwrap().value(),
        -short.npv(today()).unwrap().value(),
        0.0,
    );
    close(
        long.fair_forward_rate().unwrap(),
        1.1 * (0.03_f64 * 363.0 / 365.0).exp(),
        2e-15,
    );
    let at_fair = FxForward::new(
        spot(1.1),
        Handle::new(flat(today(), 0.02)),
        Handle::new(flat(today(), 0.05)),
        1_000_000.0,
        long.fair_forward_rate().unwrap(),
        settlement,
        delivery,
        true,
    )
    .unwrap();
    close(at_fair.npv(today()).unwrap().value(), 0.0, 3e-10);
    assert_eq!(long.base_notional(), 1_000_000.0);
    assert_eq!(long.strike(), 1.15);
    assert_eq!(long.spot_settlement_date(), settlement);
    assert_eq!(long.delivery_date(), delivery);
    assert!(long.is_long());
    assert_eq!(long.spot().rate(), 1.1);
    assert_ne!(
        long.fair_forward_rate().unwrap(),
        forward(today(), delivery, true)
            .fair_forward_rate()
            .unwrap()
    );
}

#[test]
fn live_curve_quotes_and_relinks_are_read_without_cached_results() {
    let quote = shared(SimpleQuote::new(0.02));
    let base = RelinkableHandle::<dyn YieldTermStructure>::new(shared(FlatForward::new(
        today(),
        Handle::new(quote.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )));
    let target = RelinkableHandle::<dyn YieldTermStructure>::new(flat(today(), 0.05));
    let contract = FxForward::new(
        spot(1.1),
        base.handle(),
        target.handle(),
        100.0,
        1.15,
        today() + 2,
        today() + 365,
        true,
    )
    .unwrap();
    let original = contract.npv(today()).unwrap().value();
    quote.set_value(0.03);
    assert_ne!(contract.npv(today()).unwrap().value(), original);
    let updated = contract.fair_forward_rate().unwrap();
    base.link_to(flat(today(), -0.01));
    assert_ne!(contract.fair_forward_rate().unwrap(), updated);
    let updated = contract.npv(today()).unwrap().value();
    target.link_to(flat(today() - 20, 0.07));
    assert_ne!(contract.npv(today()).unwrap().value(), updated);
    assert_eq!(contract.spot().rate(), 1.1);
    base.reset();
    assert!(contract.npv(today()).is_err());
    base.link_to(flat(today(), 0.02));
    assert!(contract.npv(today()).is_ok());
}

#[test]
fn expiry_is_strict_and_zero_does_not_resolve_market_handles() {
    let delivery = today() + 10;
    let contract = FxForward::new(
        spot(1.1),
        Handle::empty(),
        Handle::empty(),
        10.0,
        1.2,
        today(),
        delivery,
        true,
    )
    .unwrap();
    assert!(!contract.is_expired(delivery).unwrap());
    assert!(contract.npv(delivery).is_err());
    assert!(contract.is_expired(delivery + 1).unwrap());
    let expired = contract.npv(delivery + 1).unwrap();
    assert_eq!(expired.value(), 0.0);
    assert_eq!(expired.currency(), &Currency::usd());
    assert!(contract.npv(Date::default()).is_err());
    let same_day = forward(today(), today(), true);
    close(same_day.fair_forward_rate().unwrap(), 1.1, 0.0);
    close(same_day.npv(today()).unwrap().value(), -50_000.0, 3e-10);
    assert!(same_day.fair_forward_rate().is_ok());
}

#[test]
fn construction_rejects_invalid_rates_notionals_dates_and_currencies() {
    let build = |rate, notional, strike, settlement, delivery| {
        FxForward::new(
            rate,
            Handle::empty(),
            Handle::empty(),
            notional,
            strike,
            settlement,
            delivery,
            true,
        )
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0.0, -1.0] {
        assert!(build(spot(bad), 1.0, 1.0, today(), today()).is_err());
        assert!(build(spot(1.1), bad, 1.0, today(), today()).is_err());
        assert!(build(spot(1.1), 1.0, bad, today(), today()).is_err());
    }
    assert!(build(spot(1.1), f64::MAX, 2.0, today(), today()).is_err());
    assert!(
        build(
            spot(1.1),
            f64::MIN_POSITIVE,
            f64::MIN_POSITIVE,
            today(),
            today()
        )
        .is_err()
    );
    assert!(build(spot(1.1), 1.0, 1.0, Date::default(), today()).is_err());
    assert!(build(spot(1.1), 1.0, 1.0, today(), Date::default()).is_err());
    assert!(build(spot(1.1), 1.0, 1.0, today(), today() - 1).is_err());
    assert!(
        build(
            ExchangeRate::new(Currency::eur(), Currency::eur(), 1.0),
            1.0,
            1.0,
            today(),
            today()
        )
        .is_err()
    );
    let blank = Currency::new("", "", 0, "", "", 1);
    assert!(
        build(
            ExchangeRate::new(blank, Currency::usd(), 1.0),
            1.0,
            1.0,
            today(),
            today()
        )
        .is_err()
    );
    assert!(build(spot(1.1), 1e-100, 1e100, today(), today()).is_ok());
    for edge in [Date::min_date(), Date::max_date()] {
        let contract = build(spot(1.1), 1.0, 1.0, edge, edge).unwrap();
        assert!(!contract.is_expired(edge).unwrap());
        assert!(contract.npv(edge).is_err());
    }
}

struct BoundaryCurve {
    base: TermStructureBase,
    discount: f64,
    max: Date,
}
impl AsObservable for BoundaryCurve {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl TermStructure for BoundaryCurve {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        self.max
    }
}
impl YieldTermStructure for BoundaryCurve {
    fn discount_impl(&self, _: f64) -> QlResult<f64> {
        Ok(self.discount)
    }
}
fn boundary(discount: f64, reference: Date, max: Date) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(BoundaryCurve {
        base: TermStructureBase::with_reference_date(reference, None, Some(Actual365Fixed::new())),
        discount,
        max,
    }))
}

#[test]
fn checked_curve_domains_and_intermediate_arithmetic_do_not_silently_price() {
    let build = |curve| {
        FxForward::new(
            spot(1.1),
            curve,
            Handle::new(flat(today(), 0.05)),
            100.0,
            1.15,
            today(),
            today() + 365,
            true,
        )
        .unwrap()
    };
    for bad in [f64::NAN, f64::INFINITY, -1.0, 0.0] {
        let contract = build(boundary(bad, today(), Date::max_date()));
        assert!(contract.fair_forward_rate().is_err());
        assert!(contract.npv(today()).is_err());
    }
    assert!(
        build(Handle::new(flat(today() + 1, 0.02)))
            .fair_forward_rate()
            .is_err()
    );
    let truncated = boundary(0.9, today(), today() + 30);
    assert!(build(truncated.clone()).npv(today()).is_err());
    truncated
        .current_link()
        .unwrap()
        .base()
        .enable_extrapolation();
    assert!(build(truncated).npv(today()).is_ok());
    let overflow = FxForward::new(
        spot(f64::MAX),
        Handle::new(flat(today(), -1.0)),
        Handle::new(flat(today(), 0.0)),
        1.0,
        1.0,
        today(),
        today() + 365,
        true,
    )
    .unwrap();
    assert!(overflow.fair_forward_rate().is_err());
    let overflow = FxForward::new(
        spot(1.0),
        Handle::new(flat(today(), -1.0)),
        Handle::new(flat(today(), 0.0)),
        f64::MAX,
        1.0,
        today(),
        today() + 365,
        true,
    )
    .unwrap();
    assert!(overflow.npv(today()).is_err());
    let underflow = FxForward::new(
        spot(f64::from_bits(1)),
        Handle::new(flat(today(), 1.0)),
        Handle::new(flat(today(), 0.0)),
        1.0,
        1.0,
        today(),
        today() + 365,
        true,
    )
    .unwrap();
    assert!(underflow.fair_forward_rate().is_err());
}
