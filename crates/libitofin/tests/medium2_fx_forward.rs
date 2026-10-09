use libitofin::currency::Currency;
use libitofin::exchangerate::ExchangeRate;
use libitofin::fxforward::FxForward;
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
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
