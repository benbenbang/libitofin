use super::*;
use crate::handle::RelinkableHandle;
use crate::quotes::{SimpleQuote, make_quote_handle};
use crate::termstructures::yields::FlatForward;
use crate::test_support::{Flag, as_observer};
use crate::time::{date::Month, daycounters::actual360::Actual360};

fn parameters() -> GjrGarchParameters {
    GjrGarchParameters {
        v0: 0.04 / 252.0,
        omega: 2e-6,
        alpha: 0.04,
        beta: 0.88,
        gamma: 0.08,
        lambda: -0.4,
        days_per_year: 252.0,
    }
}
fn reference() -> Date {
    Date::new(15, Month::June, 2026)
}
fn curve(rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        reference(),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
fn process(scheme: GjrGarchDiscretization) -> GjrGarchProcess {
    GjrGarchProcess::new(
        Handle::new(curve(0.03)),
        Handle::new(curve(0.01)),
        make_quote_handle(100.0).handle(),
        parameters(),
        scheme,
    )
    .unwrap()
}
fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-12 + expected.abs() * 2e-13,
        "{actual:.17e} != {expected:.17e}"
    );
}

#[test]
fn quantlib_executable_transitions_match_all_variance_schemes() {
    let fixtures = [
        (
            GjrGarchDiscretization::PartialTruncation,
            0.04,
            -0.41134244368033546,
            0.2,
            100.88581745008614,
            0.03744064871654711,
        ),
        (
            GjrGarchDiscretization::PartialTruncation,
            -0.04,
            0.6653584436803355,
            1e-8,
            100.00793682288563,
            -0.03735968871555422,
        ),
        (
            GjrGarchDiscretization::FullTruncation,
            -0.04,
            0.12700799999999998,
            1e-8,
            100.00793682288563,
            -0.039496,
        ),
        (
            GjrGarchDiscretization::Reflection,
            -0.04,
            -0.41134244368033557,
            -0.2,
            100.88581745008614,
            0.03744064871654712,
        ),
    ];
    for (scheme, variance, drift_v, sigma_s, spot, next_v) in fixtures {
        let p = process(scheme);
        let state = Array::from([100.0, variance]);
        close(p.drift(0.0, &state).unwrap()[1], drift_v);
        let diffusion = p.diffusion(0.0, &state).unwrap();
        close(diffusion[(0, 0)], sigma_s);
        if variance.abs() > 0.0 && sigma_s.abs() > 1e-8 {
            close(diffusion[(1, 0)], -0.0030924899620225947);
            close(diffusion[(1, 1)], 0.06275780391639234);
        }
        let evolved = p
            .evolve(0.0, &state, 1.0 / 252.0, &Array::from([0.7, -0.2]))
            .unwrap();
        close(evolved[0], spot);
        close(evolved[1], next_v);
    }
}

#[test]
fn getters_initialization_and_clock_preserve_daily_parameters() {
    let p = process(GjrGarchDiscretization::FullTruncation);
    assert_eq!(p.size(), 2);
    assert_eq!(p.factors(), 2);
    assert_eq!(p.parameters(), parameters());
    close(p.initial_values().unwrap()[1], 0.04);
    close(p.time(&(reference() + 180)).unwrap(), 0.5);
    assert_eq!(p.discretization(), GjrGarchDiscretization::FullTruncation);
    close(p.v0(), parameters().v0);
    close(p.omega(), parameters().omega);
    close(p.alpha(), parameters().alpha);
    close(p.beta(), parameters().beta);
    close(p.gamma(), parameters().gamma);
    close(p.lambda(), parameters().lambda);
    close(p.days_per_year(), parameters().days_per_year);
}

#[test]
fn leverage_and_lambda_coefficients_match_independent_quantlib_cases() {
    let mut params = parameters();
    params.lambda = 0.3;
    params.gamma = -0.02;
    close(
        params
            .evolve(
                [100.0, 0.04],
                1.0 / 252.0,
                0.02,
                [0.7, -0.2],
                GjrGarchDiscretization::FullTruncation,
            )
            .unwrap()[1],
        0.036463403577582515,
    );
    params.lambda = 0.0;
    params.gamma = 0.08;
    close(
        params
            .evolve(
                [100.0, 0.04],
                1.0 / 252.0,
                0.02,
                [0.7, -0.2],
                GjrGarchDiscretization::FullTruncation,
            )
            .unwrap()[1],
        0.03618634052853801,
    );
}

#[test]
fn zero_variance_intercept_and_nonstationarity_are_supported() {
    let mut params = parameters();
    params.v0 = 0.0;
    params.omega = 0.0;
    params.beta = 1.2;
    params.validate().unwrap();
    let state = params
        .evolve(
            [100.0, 0.0],
            1.0,
            0.02,
            [5.0, -5.0],
            GjrGarchDiscretization::FullTruncation,
        )
        .unwrap();
    close(state[0], 100.0 * 0.02_f64.exp());
    assert_eq!(state[1], 0.0);
}

#[test]
fn zero_step_reflection_follows_quantlib_raw_negative_variance_rule() {
    for scheme in [
        GjrGarchDiscretization::PartialTruncation,
        GjrGarchDiscretization::FullTruncation,
        GjrGarchDiscretization::Reflection,
    ] {
        let state = parameters()
            .evolve([100.0, -0.04], 0.0, 0.02, [0.0, 0.0], scheme)
            .unwrap();
        assert_eq!(state[0], 100.0);
        close(
            state[1],
            if scheme == GjrGarchDiscretization::Reflection {
                0.04
            } else {
                -0.04
            },
        );
    }
}

#[test]
fn invalid_parameters_states_steps_and_overflow_return_errors() {
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut params = parameters();
        params.lambda = value;
        assert!(params.validate().is_err());
    }
    for params in [
        GjrGarchParameters {
            v0: -1.0,
            ..parameters()
        },
        GjrGarchParameters {
            omega: -1.0,
            ..parameters()
        },
        GjrGarchParameters {
            alpha: -1.0,
            ..parameters()
        },
        GjrGarchParameters {
            beta: -1.0,
            ..parameters()
        },
        GjrGarchParameters {
            gamma: -1.0,
            ..parameters()
        },
        GjrGarchParameters {
            days_per_year: 0.0,
            ..parameters()
        },
        GjrGarchParameters {
            days_per_year: f64::MAX,
            ..parameters()
        },
        GjrGarchParameters {
            lambda: f64::MAX,
            ..parameters()
        },
    ] {
        assert!(params.validate().is_err());
    }
    let p = process(GjrGarchDiscretization::FullTruncation);
    let valid = Array::from([100.0, 0.04]);
    for invalid in [
        Array::new(),
        Array::from([1.0]),
        Array::from([1.0, 2.0, 3.0]),
        Array::from([0.0, 0.04]),
        Array::from([100.0, f64::NAN]),
    ] {
        assert!(p.drift(0.0, &invalid).is_err());
        assert!(p.diffusion(0.0, &invalid).is_err());
        assert!(p.evolve(0.0, &invalid, 0.1, &valid).is_err());
        assert!(p.expectation(0.0, &invalid, 0.1).is_err());
    }
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(p.evolve(0.0, &valid, invalid, &valid).is_err());
        assert!(p.std_deviation(0.0, &valid, invalid).is_err());
        assert!(p.covariance(0.0, &valid, invalid).is_err());
        assert!(p.drift(invalid, &valid).is_err());
    }
    assert!(p.apply(&valid, &Array::from([1000.0, 0.0])).is_err());
    assert!(
        p.evolve(0.0, &valid, 1.0, &Array::from([f64::MAX; 2]))
            .is_err()
    );
    let dyn_process: &dyn StochasticProcess = &p;
    assert!(
        dyn_process
            .apply(&Array::new(), &Array::new())
            .iter()
            .all(|x| x.is_nan())
    );
}

#[test]
fn handles_stay_live_and_forward_quote_curve_and_relink_updates() {
    let spot = shared(SimpleQuote::new(100.0));
    let rate_quote = shared(SimpleQuote::new(0.03));
    let rate_curve: Shared<dyn YieldTermStructure> = shared(FlatForward::new(
        reference(),
        Handle::new(rate_quote.clone() as Shared<dyn Quote>),
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let rf = RelinkableHandle::new(rate_curve);
    let q = RelinkableHandle::new(curve(0.01));
    let p = GjrGarchProcess::new(
        rf.handle(),
        q.handle(),
        Handle::new(spot.clone() as Shared<dyn Quote>),
        parameters(),
        GjrGarchDiscretization::FullTruncation,
    )
    .unwrap();
    let flag = Flag::new();
    p.observable().register_observer(&as_observer(&flag));
    spot.set_value(120.0);
    assert!(Flag::is_up(&flag));
    close(p.initial_values().unwrap()[0], 120.0);
    Flag::lower(&flag);
    rate_quote.set_value(0.04);
    assert!(Flag::is_up(&flag));
    close(p.drift(0.0, &Array::from([120.0, 0.04])).unwrap()[0], 0.01);
    Flag::lower(&flag);
    rf.link_to(curve(0.05));
    assert!(Flag::is_up(&flag));
    close(p.drift(0.0, &Array::from([120.0, 0.04])).unwrap()[0], 0.02);
    Flag::lower(&flag);
    q.link_to(curve(0.02));
    assert!(Flag::is_up(&flag));
    close(p.drift(0.0, &Array::from([120.0, 0.04])).unwrap()[0], 0.01);
    assert!(
        !p.s0()
            .points_to_same_link(&Handle::new(spot as Shared<dyn Quote>))
    );
    assert!(p.risk_free_rate().points_to_same_link(&rf.handle()));
    assert!(p.dividend_yield().points_to_same_link(&q.handle()));
}

#[test]
fn base_euler_quantities_are_checked_and_covariance_is_symmetric() {
    let p = process(GjrGarchDiscretization::FullTruncation);
    let state = Array::from([100.0, 0.04]);
    let dt = 0.01;
    let expectation = p.expectation(0.0, &state, dt).unwrap();
    let drift = p.drift(0.0, &state).unwrap();
    close(expectation[0], state[0] * (drift[0] * dt).exp());
    close(expectation[1], state[1] + drift[1] * dt);
    let deviation = p.std_deviation(0.0, &state, dt).unwrap();
    let covariance = p.covariance(0.0, &state, dt).unwrap();
    close(covariance[(0, 1)], covariance[(1, 0)]);
    close(covariance[(0, 0)], deviation[(0, 0)] * deviation[(0, 0)]);
}
