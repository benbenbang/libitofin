use std::cell::Cell;

use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::patterns::observable::{AsObservable, Observer};
use libitofin::processes::{ForwardMeasureProcess1D, HullWhiteForwardProcess};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::stochasticprocess::StochasticProcess1D;
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::yields::{FlatForward, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn reference() -> Date {
    Date::new(9, Month::October, 2026)
}

fn flat(rate: f64) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        reference(),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

fn nonflat() -> Shared<ZeroCurve> {
    shared(
        ZeroCurve::new(
            vec![
                reference(),
                reference() + 365,
                reference() + 1095,
                reference() + 3650,
            ],
            vec![0.01, 0.025, 0.04, 0.035],
            Actual365Fixed::new(),
            Linear,
        )
        .unwrap(),
    )
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

struct Counter(Shared<Cell<usize>>);

impl Observer for Counter {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn zero_mean_reversion_is_the_finite_ho_lee_limit() {
    let curve = flat(0.03);
    let mut process = HullWhiteForwardProcess::new(curve.clone(), 0.0, 0.02).unwrap();
    process.set_forward_measure_time(5.0).unwrap();
    close(process.alpha(1.0).unwrap(), 0.0302, 1e-12);
    assert_eq!(process.b(1.0, 5.0).unwrap(), 4.0);
    close(process.m_t(1.0, 2.0, 5.0).unwrap(), 0.0014, 1e-18);
    let curve = curve.current_link().unwrap();
    let f = |t| {
        curve
            .forward_rate(t, t, Compounding::Continuous, Frequency::NoFrequency, false)
            .unwrap()
            .rate()
    };
    close(
        process.drift(1.0, 0.03).unwrap(),
        -0.0012 + (f(1.0001) - f(1.0)) / 0.0001,
        1e-16,
    );
    close(process.expectation(1.0, 0.03, 1.0).unwrap(), 0.0292, 1e-12);
    assert_eq!(process.variance(1.0, 0.03, 1.0).unwrap(), 0.0004);
    close(process.std_deviation(1.0, 0.03, 1.0).unwrap(), 0.02, 1e-18);
}

#[test]
fn tiny_mean_reversion_matches_independent_high_precision_integrals() {
    for (a, adjustment) in LIMITS {
        let process = HullWhiteForwardProcess::new(flat(0.03), a, 0.02).unwrap();
        close(process.m_t(1.0, 2.0, 5.0).unwrap(), adjustment, 5e-18);
    }
    let process = HullWhiteForwardProcess::new(flat(0.03), 1e-14, 0.02).unwrap();
    close(process.b(1.0, 5.0).unwrap(), 4.0 - 8e-14, 1e-15);
    let integrand = |u: f64| (-0.2 * (2.0 - u)).exp() * (1.0 - (-0.2 * (5.0 - u)).exp()) / 0.2;
    let intervals = 10_000;
    let h = 1.0 / f64::from(intervals);
    let integral = (1..intervals)
        .map(|i| {
            let weight = if i % 2 == 0 { 2.0 } else { 4.0 };
            weight * integrand(1.0 + f64::from(i) * h)
        })
        .sum::<f64>()
        + integrand(1.0)
        + integrand(2.0);
    let ordinary = HullWhiteForwardProcess::new(flat(0.03), 0.2, 0.02).unwrap();
    close(
        ordinary.m_t(1.0, 2.0, 5.0).unwrap(),
        0.0004 * h * integral / 3.0,
        1e-17,
    );
}

#[test]
fn native_ou_variance_branch_and_exact_transition_overrides_are_retained() {
    for a in [0.0, 1e-16, f64::EPSILON.sqrt() * 0.999] {
        let process = HullWhiteForwardProcess::new(flat(0.03), a, 0.02).unwrap();
        assert_eq!(process.variance(1.0, 0.03, 0.7).unwrap(), 0.02 * 0.02 * 0.7);
    }
    let process = HullWhiteForwardProcess::new(flat(0.03), 0.5, 0.1).unwrap();
    let exact = process.expectation(1.0, -0.1, 1.0).unwrap();
    let euler = -0.1 + process.drift(1.0, -0.1).unwrap();
    assert!((exact - euler).abs() > 0.001);
    close(
        process.evolve(1.0, -0.1, 1.0, 0.75).unwrap(),
        exact + process.std_deviation(1.0, -0.1, 1.0).unwrap() * 0.75,
        1e-16,
    );
}

#[test]
fn live_quotes_relinks_and_horizons_notify_without_resetting_initial_state() {
    let quote = shared(SimpleQuote::new(0.03));
    let curve = shared(FlatForward::new(
        reference(),
        Handle::new(quote.clone() as Shared<dyn Quote>),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let handle = RelinkableHandle::new(curve.clone() as Shared<dyn YieldTermStructure>);
    let mut process = HullWhiteForwardProcess::new(handle.handle(), 0.1, 0.02).unwrap();
    let initial = process.x0().unwrap();
    let count = shared(Cell::new(0));
    let observer = shared_mut(Counter(count.clone()));
    process
        .observable()
        .register_observer(&(observer.clone() as _));
    let first = process.alpha(1.0).unwrap();
    let forward_before = curve
        .forward_rate(
            1.0,
            1.0,
            Compounding::Continuous,
            Frequency::NoFrequency,
            false,
        )
        .unwrap()
        .rate();
    quote.set_value(0.04);
    assert!(count.get() > 0);
    let forward_after = curve
        .forward_rate(
            1.0,
            1.0,
            Compounding::Continuous,
            Frequency::NoFrequency,
            false,
        )
        .unwrap()
        .rate();
    close(
        process.alpha(1.0).unwrap() - first,
        forward_after - forward_before,
        1e-16,
    );
    assert!(process.alpha(1.0).unwrap() - first > 0.009);
    assert_eq!(process.x0().unwrap(), initial);
    handle.link_to(nonflat() as Shared<dyn YieldTermStructure>);
    assert_eq!(process.x0().unwrap(), initial);
    assert!((process.alpha(1.0).unwrap() - first).abs() > 0.005);
    let before = count.get();
    process.set_forward_measure_time(5.0).unwrap();
    assert_eq!(count.get(), before + 1);
    let drift5 = process.drift(0.7, 0.03).unwrap();
    process.set_forward_measure_time(2.0).unwrap();
    close(
        process.drift(0.7, 0.03).unwrap() - drift5,
        0.0004 * (process.b(0.7, 5.0).unwrap() - process.b(0.7, 2.0).unwrap()),
        1e-16,
    );
    let before = count.get();
    process.set_forward_measure_time(2.0).unwrap();
    assert_eq!(count.get(), before + 1);
    assert!(process.set_forward_measure_time(f64::NAN).is_err());
    assert_eq!(count.get(), before + 1);
    assert_eq!(process.forward_measure_time(), 2.0);
    handle.reset();
    assert!(process.alpha(1.0).is_err());
    assert!(process.drift(1.0, 0.03).is_err());
    assert!(process.expectation(1.0, 0.03, 0.1).is_err());
    assert_eq!(process.x0().unwrap(), initial);
    assert_eq!(process.diffusion(1.0, 0.03).unwrap(), 0.02);
}

#[test]
fn domains_zero_steps_and_curve_bounds_are_checked_without_extrapolating() {
    let curve = nonflat();
    let process = HullWhiteForwardProcess::new(Handle::new(curve.clone() as _), 0.1, 0.02).unwrap();
    assert!(process.time(&reference()).is_err());
    assert!(process.alpha(10.01).is_err());
    assert!(process.drift(10.0, 0.03).is_err());
    curve.enable_extrapolation();
    assert!(process.drift(10.0, 0.03).is_ok());
    assert_eq!(process.expectation(1e200, 0.03, 0.0).unwrap(), 0.03);
    assert_eq!(process.evolve(1e200, 0.03, 0.0, 1e200).unwrap(), 0.03);
    assert_eq!(process.variance(1e200, 0.03, 0.0).unwrap(), 0.0);
    assert_eq!(process.m_t(1.0, 1.0, 0.0).unwrap(), 0.0);
    assert!(process.b(2.0, 1.0).unwrap() < 0.0);
    for invalid in [-1.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(HullWhiteForwardProcess::new(flat(0.03), invalid, 0.02).is_err());
        assert!(HullWhiteForwardProcess::new(flat(0.03), 0.1, invalid).is_err());
        assert!(process.alpha(invalid).is_err());
        assert!(process.b(invalid, 1.0).is_err());
        assert!(process.b(1.0, invalid).is_err());
        assert!(process.m_t(invalid, 2.0, 5.0).is_err());
        assert!(process.m_t(1.0, invalid, 5.0).is_err());
        assert!(process.m_t(1.0, 2.0, invalid).is_err());
        assert!(process.drift(invalid, 0.03).is_err());
        assert!(process.diffusion(invalid, 0.03).is_err());
        assert!(process.expectation(1.0, 0.03, invalid).is_err());
        assert!(process.variance(1.0, 0.03, invalid).is_err());
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(process.drift(1.0, invalid).is_err());
        assert!(process.diffusion(1.0, invalid).is_err());
        assert!(process.expectation(1.0, invalid, 0.1).is_err());
        assert!(process.variance(1.0, invalid, 0.1).is_err());
        assert!(process.evolve(1.0, 0.03, 0.1, invalid).is_err());
    }
    assert!(process.m_t(2.0, 1.0, 5.0).is_err());
    assert!(process.expectation(f64::MAX, 0.03, f64::MAX).is_err());
    assert!(HullWhiteForwardProcess::new(Handle::empty(), 0.1, 0.02).is_err());
}

#[test]
fn nonfinite_results_are_errors_and_zero_volatility_is_deterministic() {
    let process = HullWhiteForwardProcess::new(flat(0.03), 1000.0, 0.02).unwrap();
    assert!(process.b(10.0, 0.0).is_err());
    assert!(process.m_t(1.0, 10.0, 0.0).is_err());
    assert!(process.drift(0.0, f64::MAX).is_err());
    let huge = HullWhiteForwardProcess::new(flat(0.03), 0.0, f64::MAX).unwrap();
    assert!(huge.alpha(1.0).is_err());
    assert!(huge.variance(0.0, 0.03, 1.0).is_err());
    assert!(huge.m_t(0.0, 1.0, 2.0).is_err());
    let zero = HullWhiteForwardProcess::new(flat(0.03), 1000.0, 0.0).unwrap();
    assert_eq!(zero.m_t(1.0, 10.0, 0.0).unwrap(), 0.0);
    assert_eq!(zero.variance(1.0, 0.03, 1.0).unwrap(), 0.0);
    assert_eq!(zero.diffusion(1.0, -0.03).unwrap(), 0.0);
}

const LIMITS: [(f64, f64); 5] = [
    (0.0, 0.0014),
    (1e-16, 0.0013999999999999998),
    (1e-14, 0.001399999999999968),
    (1e-10, 0.00139999999968),
    (1e-07, 0.001399999680000045),
];

const NATIVE: [(bool, [f64; 6], [f64; 9]); 7] = [
    (
        false,
        [0.1, 0.01, 5.0, 1.0, 0.02, 0.7],
        [
            0.02999999999926708,
            0.030045279584267396,
            3.2967995396436067,
            0.00020645447519439028,
            0.0007609550024872639,
            0.0205495923683269,
            6.532088230059708e-05,
            0.008082133524051497,
            0.015700312253896002,
        ],
    ),
    (
        false,
        [0.7, 0.15, 3.0, 0.2, -0.05, 1.3],
        [
            0.02999999999926708,
            0.030391850603335276,
            1.2273451129699358,
            0.020702588175032594,
            0.03230963911439586,
            -0.013360142269018037,
            0.013467443288562631,
            0.11604931403744975,
            -0.08298973069148788,
        ],
    ),
    (
        false,
        [0.05, 0.0, 0.0, 1.0, -0.02, 0.2],
        [
            0.02999999999926708,
            0.02999999999923708,
            -1.0254219275204823,
            0.0,
            0.0025000003330287613,
            -0.01950249168743269,
            0.0,
            0.0,
            -0.01950249168743269,
        ],
    ),
    (
        false,
        [0.1, 0.02, 0.5, 1.0, 0.04, 0.8],
        [
            0.02999999999926708,
            0.030181118339358336,
            -0.5127109637602412,
            -0.0002923108509877026,
            -0.00043237678766125165,
            0.03989899907940211,
            0.0002957124220675773,
            0.017196290939257144,
            0.02958122451584782,
        ],
    ),
    (
        true,
        [0.1, 0.01, 8.0, 2.0, 0.025, 0.4],
        [
            0.010001500000464613,
            0.04766429269868508,
            4.511883639059736,
            0.0001725224094479366,
            0.016963647161511268,
            0.03177950286874503,
            3.844182680668212e-05,
            0.006200147321369236,
            0.028059414475923486,
        ],
    ),
    (
        true,
        [0.25, 0.03, 4.0, 0.0, 0.01, 0.5],
        [
            0.010001500000464613,
            0.010001500000464613,
            2.5284822353142307,
            0.001028135826076683,
            0.012724727226058427,
            0.024069950668643324,
            0.0003981585904714712,
            0.019953911658406008,
            0.01209760367359972,
        ],
    ),
    (
        true,
        [0.3, 0.02, 10.0, 4.0, -0.01, 0.3],
        [
            0.010001500000464613,
            0.03751374815974765,
            2.782337039261378,
            0.0003163192126150941,
            0.01199326147304227,
            -0.0065734321894060765,
            0.00010981985905915201,
            0.010479497080449615,
            -0.012861130437675845,
        ],
    ),
];

#[test]
fn compiled_quantlib_forward_process_matches_ordinary_flat_and_nonflat_cases() {
    for (curved, [a, sigma, maturity, t, x, dt], expected) in NATIVE {
        let curve = if curved {
            Handle::new(nonflat() as Shared<dyn YieldTermStructure>)
        } else {
            flat(0.03)
        };
        let mut process = HullWhiteForwardProcess::new(curve, a, sigma).unwrap();
        process.set_forward_measure_time(maturity).unwrap();
        let actual = [
            process.x0().unwrap(),
            process.alpha(t).unwrap(),
            process.b(t, maturity).unwrap(),
            process.m_t(t, t + dt, maturity).unwrap(),
            process.drift(t, x).unwrap(),
            process.expectation(t, x, dt).unwrap(),
            process.variance(t, x, dt).unwrap(),
            process.std_deviation(t, x, dt).unwrap(),
            process.evolve(t, x, dt, -0.6).unwrap(),
        ];
        for (value, oracle) in actual.into_iter().zip(expected) {
            close(value, oracle, 2e-12);
        }
    }
}

#[test]
fn stable_small_a_factors_depend_on_a_times_duration_not_a_alone() {
    let process = HullWhiteForwardProcess::new(flat(0.03), 1e-16, 0.0).unwrap();
    close(
        process.b(0.0, 1e16).unwrap() / 1e16,
        0.6321205588285577,
        1e-16,
    );
    let small = HullWhiteForwardProcess::new(flat(0.03), 1e-10, 0.0).unwrap();
    assert_eq!(small.b(0.0, 1e-320).unwrap(), 1e-320);
}
