use super::*;
use crate::processes::DiscretizedProcess1D;
use crate::test_support::{Flag, as_observer};
use crate::time::date::{Date, Month};

fn process() -> GeometricBrownianMotionProcess {
    GeometricBrownianMotionProcess::new(100.0, 0.05, 0.2).unwrap()
}

#[test]
fn parameters_and_coefficients_use_the_supplied_state() {
    let p = process();
    assert_eq!(p.x0().unwrap(), 100.0);
    assert_eq!(p.mu(), 0.05);
    assert_eq!(p.volatility(), 0.2);
    assert_eq!(p.drift(2.0, 50.0).unwrap(), 2.5);
    assert_eq!(p.diffusion(2.0, 50.0).unwrap(), 10.0);
}

#[test]
fn transitions_are_euler_not_exact_lognormal() {
    let p = process();
    assert_eq!(p.expectation(1.0, 100.0, 0.25).unwrap(), 101.25);
    assert_eq!(p.variance(1.0, 100.0, 0.25).unwrap(), 100.0);
    assert_eq!(p.std_deviation(1.0, 100.0, 0.25).unwrap(), 10.0);
    assert_eq!(p.evolve(1.0, 100.0, 0.25, -0.75).unwrap(), 93.75);
    assert!((p.expectation(0.0, 100.0, 1.0).unwrap() - 100.0 * 0.05_f64.exp()).abs() > 0.1);
    let exact = 100.0 * ((0.05_f64 - 0.5 * 0.2 * 0.2) * 0.25 - 0.2 * 0.5 * 0.75).exp();
    assert!((p.evolve(0.0, 100.0, 0.25, -0.75).unwrap() - exact).abs() > 0.2);
}

#[test]
fn negative_states_keep_signed_diffusion_and_euler_shocks() {
    let p = GeometricBrownianMotionProcess::new(-100.0, 0.05, 0.2).unwrap();
    assert_eq!(p.x0().unwrap(), -100.0);
    assert_eq!(p.drift(0.0, -100.0).unwrap(), -5.0);
    assert_eq!(p.diffusion(0.0, -100.0).unwrap(), -20.0);
    assert_eq!(p.std_deviation(0.0, -100.0, 0.25).unwrap(), -10.0);
    assert_eq!(p.variance(0.0, -100.0, 0.25).unwrap(), 100.0);
    assert_eq!(p.evolve(0.0, -100.0, 0.25, -0.75).unwrap(), -93.75);
}

#[test]
fn a_large_negative_shock_can_cross_zero_and_continue() {
    let p = process();
    let next = p.evolve(0.0, 100.0, 1.0, -6.0).unwrap();
    assert_eq!(next, -15.0);
    assert_eq!(p.diffusion(1.0, next).unwrap(), -3.0);
    assert_eq!(p.evolve(1.0, next, 1.0, 1.0).unwrap(), -18.75);
}

#[test]
fn zero_state_is_absorbing() {
    let p = GeometricBrownianMotionProcess::new(0.0, f64::MAX, f64::MAX).unwrap();
    assert_eq!(p.drift(0.0, 0.0).unwrap(), 0.0);
    assert_eq!(p.diffusion(0.0, 0.0).unwrap(), 0.0);
    assert_eq!(p.expectation(0.0, 0.0, 1.0).unwrap(), 0.0);
    assert_eq!(p.variance(0.0, 0.0, 1.0).unwrap(), 0.0);
    assert_eq!(p.evolve(0.0, 0.0, 1.0, f64::MAX).unwrap(), 0.0);
}

#[test]
fn zero_volatility_produces_deterministic_euler_drift() {
    let p = GeometricBrownianMotionProcess::new(100.0, -0.1, 0.0).unwrap();
    assert_eq!(p.variance(0.0, 100.0, 0.5).unwrap(), 0.0);
    assert_eq!(p.std_deviation(0.0, 100.0, 0.5).unwrap(), 0.0);
    assert_eq!(p.evolve(0.0, 100.0, 0.5, f64::MAX).unwrap(), 95.0);
}

#[test]
fn zero_steps_preserve_state_bits_without_coefficient_overflow() {
    let p = GeometricBrownianMotionProcess::new(f64::MAX, f64::MAX, f64::MAX).unwrap();
    for x in [f64::MAX, -f64::MAX, -0.0, 0.0] {
        for dt in [0.0, -0.0] {
            assert_eq!(p.expectation(0.0, x, dt).unwrap().to_bits(), x.to_bits());
            assert_eq!(
                p.evolve(0.0, x, dt, f64::MAX).unwrap().to_bits(),
                x.to_bits()
            );
            assert_eq!(p.variance(0.0, x, dt).unwrap(), 0.0);
            assert_eq!(p.std_deviation(0.0, x, dt).unwrap(), 0.0);
        }
    }
}

#[test]
fn constructor_rejects_nonfinite_parameters_and_negative_volatility() {
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(GeometricBrownianMotionProcess::new(invalid, 0.05, 0.2).is_err());
        assert!(GeometricBrownianMotionProcess::new(100.0, invalid, 0.2).is_err());
        assert!(GeometricBrownianMotionProcess::new(100.0, 0.05, invalid).is_err());
    }
    assert!(GeometricBrownianMotionProcess::new(100.0, 0.05, -0.1).is_err());
    assert!(GeometricBrownianMotionProcess::new(-100.0, -0.05, -0.0).is_ok());
}

#[test]
fn coefficient_calls_reject_invalid_times_and_states() {
    let p = process();
    for invalid in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(p.drift(invalid, 100.0).is_err());
        assert!(p.diffusion(invalid, 100.0).is_err());
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(p.drift(0.0, invalid).is_err());
        assert!(p.diffusion(0.0, invalid).is_err());
    }
}

#[test]
fn transitions_reject_invalid_times_steps_states_and_shocks() {
    let p = process();
    for invalid in [-0.1, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for (t, dt) in [(invalid, 0.25), (0.0, invalid)] {
            assert!(p.expectation(t, 100.0, dt).is_err());
            assert!(p.variance(t, 100.0, dt).is_err());
            assert!(p.std_deviation(t, 100.0, dt).is_err());
            assert!(p.evolve(t, 100.0, dt, 1.0).is_err());
        }
    }
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for dt in [0.0, 0.25] {
            assert!(p.expectation(0.0, invalid, dt).is_err());
            assert!(p.variance(0.0, invalid, dt).is_err());
            assert!(p.std_deviation(0.0, invalid, dt).is_err());
            assert!(p.evolve(0.0, invalid, dt, 1.0).is_err());
            assert!(p.evolve(0.0, 100.0, dt, invalid).is_err());
        }
    }
}

#[test]
fn overflowing_coefficients_and_transition_outputs_return_errors() {
    let p = GeometricBrownianMotionProcess::new(1.0, f64::MAX, f64::MAX).unwrap();
    assert!(p.drift(0.0, 2.0).is_err());
    assert!(p.diffusion(0.0, 2.0).is_err());
    assert!(p.expectation(0.0, 1.0, 2.0).is_err());
    assert!(p.std_deviation(0.0, 1.0, 4.0).is_err());
    assert!(p.variance(0.0, 1.0, 1.0).is_err());
    let drift = GeometricBrownianMotionProcess::new(f64::MAX, 1.0, 0.0).unwrap();
    assert!(drift.expectation(0.0, f64::MAX, 1.0).is_err());
    let shock = GeometricBrownianMotionProcess::new(1.0, 0.0, 2.0).unwrap();
    assert!(shock.evolve(0.0, 1.0, 1.0, f64::MAX).is_err());
    let addition = GeometricBrownianMotionProcess::new(f64::MAX, 0.0, 1.0).unwrap();
    assert!(addition.evolve(0.0, f64::MAX, 1.0, 1.0).is_err());
}

#[test]
fn euler_adapter_and_trait_object_follow_the_same_transitions() {
    let p = shared(process());
    let generic: &dyn StochasticProcess1D = &*p;
    let adapter = DiscretizedProcess1D::new(p.clone());
    assert_eq!(generic.x0().unwrap(), adapter.x0().unwrap());
    for x in [-100.0, 0.0, 100.0] {
        assert_eq!(
            generic.expectation(0.0, x, 0.25).unwrap(),
            adapter.expectation(0.0, x, 0.25).unwrap()
        );
        assert_eq!(
            generic.variance(0.0, x, 0.25).unwrap(),
            adapter.variance(0.0, x, 0.25).unwrap()
        );
        assert_eq!(
            generic.std_deviation(0.0, x, 0.25).unwrap(),
            adapter.std_deviation(0.0, x, 0.25).unwrap()
        );
        assert_eq!(
            generic.evolve(0.0, x, 0.25, -0.75).unwrap(),
            adapter.evolve(0.0, x, 0.25, -0.75).unwrap()
        );
    }
    assert_eq!(generic.apply(-1.0, -2.0), -3.0);
}

#[test]
fn observable_protocol_supports_registration_without_mutable_market_inputs() {
    let p = process();
    let flag = Flag::new();
    p.observable().register_observer(&as_observer(&flag));
    p.evolve(0.0, 100.0, 0.25, 0.5).unwrap();
    assert!(!Flag::is_up(&flag));
    p.observable().notify_observers();
    assert!(Flag::is_up(&flag));
}

#[test]
fn date_to_time_conversion_is_not_supported() {
    let error = process()
        .time(&Date::new(6, Month::October, 2026))
        .unwrap_err();
    assert_eq!(error.message(), "date/time conversion not supported");
}
