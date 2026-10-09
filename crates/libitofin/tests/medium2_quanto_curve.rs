use std::cell::Cell;

use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::matrix::Matrix;
use libitofin::patterns::observable::{AsObservable, Observer};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::volatility::{
    BlackConstantVol, BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure,
};
use libitofin::termstructures::yields::{
    FlatForward, QuantoTermStructure, ZeroCurve, ZeroYieldStructure,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn reference() -> Date {
    Date::new(15, Month::June, 2026)
}

fn flat(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        reference(),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::NoFrequency,
    ))
}

fn vol(value: f64) -> Shared<dyn BlackVolTermStructure> {
    shared(BlackConstantVol::new(
        reference(),
        None,
        value,
        Actual360::new(),
    ))
}

fn basic(rho: f64) -> QuantoTermStructure {
    QuantoTermStructure::new(
        Handle::new(flat(0.01)),
        Handle::new(flat(0.04)),
        Handle::new(flat(0.02)),
        Handle::new(vol(0.2)),
        100.0,
        Handle::new(vol(0.15)),
        1.2,
        rho,
    )
    .unwrap()
}

fn nonflat(mixed: bool, strike: f64, rho: f64) -> QuantoTermStructure {
    let offsets = if mixed { [0, 30, -20, 15, -10] } else { [0; 5] };
    let dc = |i: usize| -> DayCounter {
        if mixed && (i == 1 || i == 3) {
            Actual365Fixed::new()
        } else {
            Actual360::new()
        }
    };
    let yields: Vec<_> = [
        [0.01, 0.018, 0.025],
        [0.04, 0.035, 0.05],
        [-0.005, 0.01, 0.02],
    ]
    .iter()
    .enumerate()
    .map(|(i, rates)| {
        let dates = [0, 360, 1080].map(|n| reference() + offsets[i] + n);
        Handle::new(
            shared(ZeroCurve::new(dates.to_vec(), rates.to_vec(), dc(i), Linear).unwrap())
                as Shared<dyn YieldTermStructure>,
        )
    })
    .collect();
    let asset_reference = reference() + offsets[3];
    let matrix = Matrix::from([[0.18, 0.22, 0.28], [0.25, 0.29, 0.35]]);
    let asset = shared(
        BlackVarianceSurface::new(
            asset_reference,
            Some(NullCalendar::new()),
            &[180, 360, 720].map(|n| asset_reference + n),
            vec![80.0, 120.0],
            &matrix,
            dc(3),
        )
        .unwrap(),
    );
    let fx_reference = reference() + offsets[4];
    let fx = shared(
        BlackVarianceCurve::new(
            fx_reference,
            &[120, 240, 540].map(|n| fx_reference + n),
            &[0.12, 0.15, 0.19],
            dc(4),
            true,
        )
        .unwrap(),
    );
    QuantoTermStructure::new(
        yields[0].clone(),
        yields[1].clone(),
        yields[2].clone(),
        Handle::new(asset as Shared<dyn BlackVolTermStructure>),
        strike,
        Handle::new(fx as Shared<dyn BlackVolTermStructure>),
        1.2,
        rho,
    )
    .unwrap()
}

#[test]
fn compiled_quantlib_nonflat_mixed_clock_and_strike_extrapolation() {
    let fixtures: &[(bool, f64, f64, f64, f64, f64)] = &[
        (false, 100.0, 0.4, 0.0, 1.0, 0.06545465003488546),
        (
            false,
            100.0,
            0.4,
            1e-08,
            0.9999999993454415,
            0.06545586293061285,
        ),
        (
            false,
            100.0,
            0.4,
            0.0001,
            0.9999934545564181,
            0.06545465003488546,
        ),
        (
            false,
            100.0,
            0.4,
            0.1,
            0.9935950149217954,
            0.06425585003718016,
        ),
        (
            false,
            100.0,
            0.4,
            0.5,
            0.9698340685438274,
            0.06126057094918497,
        ),
        (
            false,
            100.0,
            0.4,
            1.0,
            0.9407972306659816,
            0.061027645436939414,
        ),
        (
            false,
            100.0,
            0.4,
            1.6,
            0.8945602202052252,
            0.06963940971466065,
        ),
        (
            false,
            100.0,
            0.4,
            2.0,
            0.8640067921408138,
            0.07308732446744552,
        ),
        (true, 140.0, -0.7, 0.0, 1.0, 0.031606640524712196),
        (
            true,
            140.0,
            -0.7,
            1e-08,
            0.9999999996839215,
            0.03160784966593849,
        ),
        (
            true,
            140.0,
            -0.7,
            0.0001,
            0.9999968393409425,
            0.031606640524712196,
        ),
        (
            true,
            140.0,
            -0.7,
            0.1,
            0.9969645260927252,
            0.03040090302527437,
        ),
        (
            true,
            140.0,
            -0.7,
            0.5,
            0.9893483464666166,
            0.021417576956330606,
        ),
        (
            true,
            140.0,
            -0.7,
            1.0,
            0.9962623301557927,
            0.003744672386378839,
        ),
        (
            true,
            140.0,
            -0.7,
            1.6,
            1.0032589663094655,
            -0.0020335421176947534,
        ),
        (
            true,
            140.0,
            -0.7,
            2.0,
            1.0027195108855154,
            -0.001357909853203608,
        ),
        (true, 60.0, 1.0, 0.0, 1.0, 0.07080512090148826),
        (
            true,
            60.0,
            1.0,
            1e-08,
            0.9999999992919367,
            0.07080633854508304,
        ),
        (
            true,
            60.0,
            1.0,
            0.0001,
            0.9999929195129766,
            0.07080512090148826,
        ),
        (
            true,
            60.0,
            1.0,
            0.1,
            0.9930642259374884,
            0.06959938340234069,
        ),
        (
            true,
            60.0,
            1.0,
            0.5,
            0.9667203626554725,
            0.06769201128752819,
        ),
        (
            true,
            60.0,
            1.0,
            1.0,
            0.9286149045602586,
            0.07406115295409449,
        ),
        (
            true,
            60.0,
            1.0,
            1.6,
            0.8667236740714005,
            0.08939691751827715,
        ),
        (
            true,
            60.0,
            1.0,
            2.0,
            0.8280997544685933,
            0.09431082772111637,
        ),
    ];
    for &(mixed, strike, rho, t, discount, zero) in fixtures {
        let curve = nonflat(mixed, strike, rho);
        assert!(
            (curve.discount(t, true).unwrap() - discount).abs() <= 2.0e-14,
            "discount mismatch: mixed={mixed}, strike={strike}, t={t}"
        );
        if t != 1e-8 {
            let actual = curve
                .zero_rate(t, Compounding::Continuous, Frequency::NoFrequency, true)
                .unwrap()
                .rate();
            assert!(
                (actual - zero).abs() <= 2.0e-12,
                "zero yield mismatch at t={t}"
            );
        }
    }
}

#[test]
fn formula_correlation_boundaries_and_zero_time_short_rate_conventions() {
    for rho in [-1.0, 0.0, 1.0] {
        let curve = basic(rho);
        assert!((curve.zero_yield_impl(1.0).unwrap() - (0.03 + rho * 0.03)).abs() < 1e-14);
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);
        assert!((curve.zero_yield_impl(0.0).unwrap() - (0.03 + rho * 0.03)).abs() < 2e-12);
        let expected = curve.zero_yield_impl(1e-4).unwrap();
        assert!(
            (curve
                .zero_rate(0.0, Compounding::Continuous, Frequency::NoFrequency, false)
                .unwrap()
                .rate()
                - expected)
                .abs()
                < 2e-12
        );
        assert!(curve.zero_yield_impl(1e-8).unwrap().is_finite());
    }
}

struct Counter(Shared<Cell<usize>>);

impl Observer for Counter {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

fn subscribe(
    curve: &QuantoTermStructure,
    upstream: bool,
) -> (Shared<Cell<usize>>, SharedMut<dyn Observer>) {
    let count = shared(Cell::new(0));
    let observer = shared_mut(Counter(count.clone())) as SharedMut<dyn Observer>;
    if upstream {
        curve.register_upstream(&observer);
    } else {
        curve.observable().register_observer(&observer);
    }
    (count, observer)
}

#[test]
fn all_five_quotes_relinks_and_upstream_notifications_are_live() {
    let quotes: Vec<_> = [0.01, 0.04, 0.02, 0.2, 0.15]
        .map(|x| shared(SimpleQuote::new(x)))
        .into();
    let yields: Vec<_> = quotes[..3]
        .iter()
        .map(|q| {
            RelinkableHandle::new(shared(FlatForward::new(
                reference(),
                Handle::new(q.clone() as Shared<dyn Quote>),
                Actual360::new(),
                Compounding::Continuous,
                Frequency::NoFrequency,
            )) as Shared<dyn YieldTermStructure>)
        })
        .collect();
    let vols: Vec<_> = quotes[3..]
        .iter()
        .map(|q| {
            RelinkableHandle::new(shared(BlackConstantVol::with_quote(
                reference(),
                None,
                Handle::new(q.clone() as Shared<dyn Quote>),
                Actual360::new(),
            )) as Shared<dyn BlackVolTermStructure>)
        })
        .collect();
    let curve = QuantoTermStructure::new(
        yields[0].handle(),
        yields[1].handle(),
        yields[2].handle(),
        vols[0].handle(),
        100.0,
        vols[1].handle(),
        1.2,
        0.4,
    )
    .unwrap();
    let (count, _subscriber) = subscribe(&curve, false);
    let (upstream_count, _sibling) = subscribe(&curve, true);
    for quote in &quotes {
        let before = curve.discount(1.0, false).unwrap();
        let notifications = count.get();
        quote.set_value(quote.value().unwrap() + 0.01);
        assert_eq!(count.get(), notifications + 1);
        assert_eq!(upstream_count.get(), notifications + 1);
        assert_ne!(curve.discount(1.0, false).unwrap(), before);
    }
    for (i, handle) in yields.iter().enumerate() {
        let before = curve.discount(1.0, false).unwrap();
        handle.link_to(flat(0.06 + i as f64 * 0.01));
        assert_ne!(curve.discount(1.0, false).unwrap(), before);
    }
    for (i, handle) in vols.iter().enumerate() {
        let before = curve.discount(1.0, false).unwrap();
        handle.link_to(vol(0.4 + i as f64 * 0.1));
        assert_ne!(curve.discount(1.0, false).unwrap(), before);
    }
    assert_eq!(count.get(), 10);
    assert_eq!(upstream_count.get(), 10);
    let weak = SharedMut::downgrade(&curve.updater());
    drop(curve);
    assert!(weak.upgrade().is_none());
}

#[test]
fn dividend_metadata_settings_and_relinks_follow_live_curve() {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let dividend = RelinkableHandle::new(shared(FlatForward::moving_with_rate(
        2,
        NullCalendar::new(),
        0.01,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::NoFrequency,
        settings.clone(),
    )) as Shared<dyn YieldTermStructure>);
    let curve = QuantoTermStructure::new(
        dividend.handle(),
        Handle::new(flat(0.04)),
        Handle::new(flat(0.02)),
        Handle::new(vol(0.2)),
        100.0,
        Handle::new(vol(0.15)),
        1.2,
        0.4,
    )
    .unwrap();
    let (count, _subscriber) = subscribe(&curve, false);
    assert_eq!(curve.reference_date().unwrap(), reference() + 2);
    assert_eq!(curve.settlement_days().unwrap(), 2);
    assert_eq!(curve.calendar().unwrap().name(), "Null");
    assert_eq!(curve.day_counter().unwrap().name(), "Actual/360");
    settings.set_evaluation_date(reference() + 5);
    assert_eq!(count.get(), 1);
    assert_eq!(curve.reference_date().unwrap(), reference() + 7);
    dividend.link_to(shared(FlatForward::with_rate(
        reference() + 30,
        0.02,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::NoFrequency,
    )));
    assert_eq!(count.get(), 2);
    assert_eq!(curve.reference_date().unwrap(), reference() + 30);
    assert_eq!(curve.day_counter().unwrap().name(), "Actual/365 (Fixed)");
    assert!(curve.calendar().is_none());
    assert!(curve.settlement_days().is_err());
    settings.set_evaluation_date(reference() + 10);
    assert_eq!(count.get(), 2);
}

#[test]
fn calendar_maximum_and_public_extrapolation_are_independent_of_input_domains() {
    let curve = nonflat(true, 140.0, -0.7);
    assert_eq!(curve.max_date(), reference() + 530);
    assert_eq!(curve.max_time().unwrap(), 530.0 / 360.0);
    assert!(curve.discount(530.0 / 360.0, false).is_ok());
    assert!(curve.discount(1.5, false).is_err());
    assert!(curve.discount(1.5, true).is_ok());
    curve.enable_extrapolation();
    assert!(curve.discount(2.0, false).is_ok());
    curve.disable_extrapolation();
    assert!(curve.discount(2.0, false).is_err());
    assert!(curve.discount_date(reference() - 1, true).is_err());
    let underlying = flat(0.01);
    underlying.enable_extrapolation();
    let adapted = QuantoTermStructure::new(
        Handle::new(underlying.clone()),
        Handle::new(flat(0.04)),
        Handle::new(flat(0.02)),
        Handle::new(vol(0.2)),
        100.0,
        Handle::new(vol(0.15)),
        1.2,
        0.4,
    )
    .unwrap();
    assert!(!adapted.allows_extrapolation());
    underlying.disable_extrapolation();
    underlying.observable().notify_observers();
    assert!(!adapted.allows_extrapolation());
}

#[test]
fn missing_each_input_is_checked_and_can_be_relinked_later() {
    for missing in 0..5 {
        let yields: Vec<_> = (0..3)
            .map(|i| {
                if i == missing {
                    RelinkableHandle::empty()
                } else {
                    RelinkableHandle::new(flat(0.02))
                }
            })
            .collect();
        let vols: Vec<_> = (3..5)
            .map(|i| {
                if i == missing {
                    RelinkableHandle::empty()
                } else {
                    RelinkableHandle::new(vol(0.2))
                }
            })
            .collect();
        let curve = QuantoTermStructure::new(
            yields[0].handle(),
            yields[1].handle(),
            yields[2].handle(),
            vols[0].handle(),
            100.0,
            vols[1].handle(),
            1.2,
            0.4,
        )
        .unwrap();
        assert_eq!(curve.max_date(), Date::null());
        assert!(curve.discount(1.0, true).is_err());
        assert_eq!(curve.discount(0.0, true).unwrap(), 1.0);
        assert!(
            curve
                .zero_rate(0.0, Compounding::Continuous, Frequency::NoFrequency, true)
                .is_err()
        );
        if missing == 0 {
            assert!(curve.day_counter().is_none());
            assert!(curve.calendar().is_none());
            assert!(curve.reference_date().is_err());
            assert!(curve.settlement_days().is_err());
        }
        let (count, _subscriber) = subscribe(&curve, false);
        if missing < 3 {
            yields[missing].link_to(flat(0.02));
        } else {
            vols[missing - 3].link_to(vol(0.2));
        }
        assert_eq!(count.get(), 1);
        assert!(curve.discount(1.0, false).is_ok());
    }
}

#[test]
fn checked_scalars_nonfinite_market_values_and_discount_overflow() {
    let make = |strike, fx_atm, rho, rate, sigma| {
        QuantoTermStructure::new(
            Handle::new(flat(rate)),
            Handle::new(flat(0.04)),
            Handle::new(flat(0.02)),
            Handle::new(vol(sigma)),
            strike,
            Handle::new(vol(0.15)),
            fx_atm,
            rho,
        )
    };
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(make(bad, 1.2, 0.4, 0.01, 0.2).is_err());
        assert!(make(100.0, bad, 0.4, 0.01, 0.2).is_err());
        assert!(make(100.0, 1.2, bad, 0.01, 0.2).is_err());
        assert!(
            make(100.0, 1.2, 0.4, bad, 0.2)
                .unwrap()
                .discount(1.0, true)
                .is_err()
        );
        assert!(
            make(100.0, 1.2, 0.4, 0.01, bad)
                .unwrap()
                .discount(1.0, true)
                .is_err()
        );
    }
    for rho in [-1.0001, 1.0001] {
        assert!(make(100.0, 1.2, rho, 0.01, 0.2).is_err());
    }
    for strike in [-100.0, 0.0] {
        assert!(
            make(strike, -1.2, 0.4, -0.01, 0.2)
                .unwrap()
                .discount(1.0, true)
                .is_ok()
        );
    }
    assert!(
        make(100.0, 1.2, 0.4, 0.01, -0.2)
            .unwrap()
            .discount(1.0, true)
            .is_err()
    );
    for rate in [-1000.0, 1000.0] {
        assert!(
            make(100.0, 1.2, 0.4, rate, 0.2)
                .unwrap()
                .discount(1.0, true)
                .is_err()
        );
    }
    let curve = basic(0.4);
    for t in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(curve.discount(t, true).is_err());
        assert!(curve.discount_impl(t).is_err());
        assert!(curve.zero_yield_impl(t).is_err());
    }
}
