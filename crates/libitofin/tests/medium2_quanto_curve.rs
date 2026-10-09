use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::matrix::Matrix;
use libitofin::shared::{Shared, shared};
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
