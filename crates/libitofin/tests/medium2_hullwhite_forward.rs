use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::processes::{ForwardMeasureProcess1D, HullWhiteForwardProcess};
use libitofin::shared::{Shared, shared};
use libitofin::stochasticprocess::StochasticProcess1D;
use libitofin::termstructures::yields::FlatForward;
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

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
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

const LIMITS: [(f64, f64); 5] = [
    (0.0, 0.0014),
    (1e-16, 0.0013999999999999998),
    (1e-14, 0.001399999999999968),
    (1e-10, 0.00139999999968),
    (1e-07, 0.001399999680000045),
];
